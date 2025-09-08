mod audio;
mod editor;
mod ui;

use atomic_float::AtomicF32;
use audio::audio_engine::AudioEngine;
use audio::meter_communication::{create_meter_channels, MeterInput, MeterOutput};
use audio::spectrum_analyzer::{SpectrumAnalyzer, SpectrumOutput};
use editor::EditorInitFlags;
use editor::PluginEditor;
use nih_plug::prelude::*;
use nih_plug_iced::{create_iced_editor, IcedState};
use std::sync::{atomic::Ordering, Arc};

struct PluginLearn {
    // CORE PLUGIN COMPONENTS
    params: Arc<PluginLearnParams>,

    // AUDIO PROCESSING - Direct ownership, no Option<> wrapper
    audio_engine: AudioEngine,

    // SHARED STATE - Minimal
    sample_rate: Arc<AtomicF32>,

    // UI COMMUNICATION - Separated input/output channels
    spectrum_analyzer: SpectrumAnalyzer, // Audio thread only
    spectrum_output: SpectrumOutput,     // UI thread only
    meter_input: MeterInput,             // Audio thread only
    meter_output: MeterOutput,           // UI thread only

    // UI STATE
    iced_state: Arc<IcedState>,
}

#[derive(Params)]
struct PluginLearnParams {
    /// The parameter's ID is used to identify the parameter in the wrapped plugin API. As long as
    /// these IDs remain constant, you can rename and reorder these fields as you wish. The
    /// parameters are exposed to the host in the same order they were defined. In this case, this
    /// gain parameter is stored as linear gain while the values are displayed in decibels.
    #[id = "gain"]
    pub gain: FloatParam,
}

impl Default for PluginLearn {
    fn default() -> Self {
        let sample_rate = Arc::new(AtomicF32::new(44100.0));
        let (spectrum_analyzer, spectrum_output) = SpectrumAnalyzer::new();
        let (meter_input, meter_output) = create_meter_channels();

        Self {
            // CORE COMPONENTS
            params: Arc::new(PluginLearnParams::new(sample_rate.clone())),

            // AUDIO PROCESSING - Initialize directly (no Option<>)
            audio_engine: AudioEngine::new(),

            // SHARED STATE
            sample_rate,

            // COMMUNICATION CHANNELS - Separated
            spectrum_analyzer,
            spectrum_output,
            meter_input,
            meter_output,

            // UI STATE
            iced_state: IcedState::from_size(800, 600),
        }
    }
}

impl PluginLearnParams {
    pub fn new(_sample_rate: Arc<AtomicF32>) -> Self {
        Self {
            // This gain is stored as linear gain. NIH-plug comes with useful conversion functions
            // to treat these kinds of parameters as if we were dealing with decibels. Storing this
            // as decibels is easier to work with, but requires a conversion for every sample.
            gain: FloatParam::new(
                "Gain",
                util::db_to_gain(0.0),
                FloatRange::Skewed {
                    min: util::db_to_gain(-30.0),
                    max: util::db_to_gain(30.0),
                    // This makes the range appear as if it was linear when displaying the values as
                    // decibels
                    factor: FloatRange::gain_skew_factor(-30.0, 30.0),
                },
            )
            // Because the gain parameter is stored as linear gain instead of storing the value as
            // decibels, we need logarithmic smoothing (reduced from 50ms to 5ms for faster response)
            .with_smoother(SmoothingStyle::Logarithmic(5.0))
            .with_unit(" dB")
            // There are many predefined formatters we can use here. If the gain was stored as
            // decibels instead of as a linear gain value, we could have also used the
            // `.with_step_size(0.1)` function to get internal rounding.
            .with_value_to_string(formatters::v2s_f32_gain_to_db(2))
            .with_string_to_value(formatters::s2v_f32_gain_to_db()),
        }
    }
}

impl Default for PluginLearnParams {
    fn default() -> Self {
        Self::new(Arc::new(AtomicF32::new(44100.0)))
    }
}

impl Plugin for PluginLearn {
    const NAME: &'static str = "plugin-learn";
    const VENDOR: &'static str = "Cmdv";
    const URL: &'static str = env!("CARGO_PKG_HOMEPAGE");
    const EMAIL: &'static str = "info@cmdv.me";

    const VERSION: &'static str = env!("CARGO_PKG_VERSION");

    // The first audio IO layout is used as the default. The other layouts may be selected either
    // explicitly or automatically by the host or the user depending on the plugin API/backend.
    const AUDIO_IO_LAYOUTS: &'static [AudioIOLayout] = &[AudioIOLayout {
        main_input_channels: NonZeroU32::new(2),
        main_output_channels: NonZeroU32::new(2),

        aux_input_ports: &[],
        aux_output_ports: &[],

        // Individual ports and the layout as a whole can be named here. By default these names
        // are generated as needed. This layout will be called 'Stereo', while a layout with
        // only one input and output channel would be called 'Mono'.
        names: PortNames::const_default(),
    }];

    const MIDI_INPUT: MidiConfig = MidiConfig::None;
    const MIDI_OUTPUT: MidiConfig = MidiConfig::None;

    const SAMPLE_ACCURATE_AUTOMATION: bool = true;

    // If the plugin can send or receive SysEx messages, it can define a type to wrap around those
    // messages here. The type implements the `SysExMessage` trait, which allows conversion to and
    // from plain byte buffers.
    type SysExMessage = ();
    // More advanced plugins can use this to run expensive background tasks. See the field's
    // documentation for more information. `()` means that the plugin does not have any background
    // tasks.
    type BackgroundTask = ();

    fn params(&self) -> Arc<dyn Params> {
        self.params.clone()
    }

    fn initialize(
        &mut self,
        _audio_io_layout: &AudioIOLayout,
        buffer_config: &BufferConfig,
        _context: &mut impl InitContext<Self>,
    ) -> bool {
        // Store sample rate for communication with UI
        self.sample_rate
            .store(buffer_config.sample_rate, Ordering::Relaxed);

        nih_plug::nih_log!(
            "Plugin initialized - sample_rate: {}, buffer_size: {}",
            buffer_config.sample_rate,
            buffer_config.max_buffer_size
        );

        true
    }

    fn reset(&mut self) {
        // Reset buffers and envelopes here. This can be called from the audio thread and may not
        // allocate. You can remove this function if you do not need it.
    }

    fn process(
        &mut self,
        buffer: &mut Buffer,
        _aux: &mut AuxiliaryBuffers,
        _context: &mut impl ProcessContext<Self>,
    ) -> ProcessStatus {
        // CLEAN SEPARATION - Following Diopser pattern
        let sample_rate = self.sample_rate.load(Ordering::Relaxed);
        // 1. Pre-gain spectrum analysis (analyze input signal)
        self.spectrum_analyzer.process(buffer, sample_rate);

        // 2. Apply audio effects (core gain processing)
        self.audio_engine.process(buffer, &self.params.gain);

        // 3. Post-gain meter analysis (analyze output signal)
        self.meter_input.update_peaks(buffer);

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        nih_plug::nih_log!("Editor requested");

        let init_flags = EditorInitFlags {
            params: self.params.clone(),
            sample_rate: self.sample_rate.clone(),
            spectrum_output: self.spectrum_output.clone(),
            meter_output: self.meter_output.clone(),
        };

        create_iced_editor::<PluginEditor>(
            self.iced_state.clone(),
            init_flags,
            Vec::new(), // fonts
        )
    }
}

impl ClapPlugin for PluginLearn {
    const CLAP_ID: &'static str = "com.your-domain.plugin-learn";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A short description of your plugin");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;

    // Don't forget to change these features
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::AudioEffect, ClapFeature::Stereo];
}

impl Vst3Plugin for PluginLearn {
    const VST3_CLASS_ID: [u8; 16] = *b"Exactly16Chars!!";

    // And also don't forget to change these categories
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Fx, Vst3SubCategory::Dynamics];
}

nih_export_clap!(PluginLearn);
