mod audio;
mod editor;
mod ui;

use atomic_float::AtomicF32;
use audio::meter::{create_meter_channels, MeterConsumer, MeterProducer};
use audio::spectrum::{SpectrumConsumer, SpectrumProducer};
use editor::EditorInitFlags;
use editor::PluginEditor;
use nih_plug::prelude::*;
use nih_plug_iced::{create_iced_editor, IcedState};
use std::sync::{atomic::Ordering, Arc};

struct SAPlugin {
    // PLUGIN PARAMETERS (empty for now, but keeps the structure)
    params: Arc<SAPluginParams>,

    // SHARED STATE (thread-safe, read by both audio and UI)
    sample_rate: Arc<AtomicF32>,

    // AUDIO THREAD WRITERS (produce data)
    audio_spectrum_producer: SpectrumProducer, // Writes spectrum data from audio thread
    audio_meter_producer: MeterProducer,       // Writes meter levels from audio thread

    // UI THREAD READERS (consume data)
    ui_spectrum_consumer: SpectrumConsumer, // Reads spectrum data in UI thread
    ui_meter_consumer: MeterConsumer,       // Reads meter levels in UI thread

    // UI STATE
    iced_state: Arc<IcedState>,
}

#[derive(Params)]
struct SAPluginParams {}

impl Default for SAPlugin {
    fn default() -> Self {
        let sample_rate = Arc::new(AtomicF32::new(44100.0));

        // Use the builder pattern to configure the spectrum analyzer
        // This demonstrates how to customize the analyzer settings
        let (audio_spectrum_producer, ui_spectrum_consumer) = SpectrumProducer::builder()
            .speed(audio::spectrum::SpectrumSpeed::Medium)  // Default speed for balanced response
            .build();

        let (audio_meter_producer, ui_meter_consumer) = create_meter_channels();

        Self {
            // CORE COMPONENTS
            params: Arc::new(SAPluginParams::default()),

            // SHARED STATE
            sample_rate,

            // AUDIO/UI COMMUNICATION
            audio_spectrum_producer,
            audio_meter_producer,
            ui_spectrum_consumer,
            ui_meter_consumer,

            // UI STATE
            iced_state: IcedState::from_size(800, 600),
        }
    }
}

impl Default for SAPluginParams {
    fn default() -> Self {
        SAPluginParams {}
    }
}

impl Plugin for SAPlugin {
    const NAME: &'static str = "spectrum_analyser";
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
        // Analyze the input signal
        let sample_rate = self.sample_rate.load(Ordering::Relaxed);
        self.audio_spectrum_producer.process(buffer, sample_rate);
        self.audio_meter_producer.update_peaks(buffer);

        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        let init_flags = EditorInitFlags {
            sample_rate: self.sample_rate.clone(),
            spectrum_output: self.ui_spectrum_consumer.clone(),
            meter_output: self.ui_meter_consumer.clone(),
        };

        create_iced_editor::<PluginEditor>(
            self.iced_state.clone(),
            init_flags,
            Vec::new(), // fonts
        )
    }
}

impl ClapPlugin for SAPlugin {
    const CLAP_ID: &'static str = "com.your-domain.spectrum-analyser";
    const CLAP_DESCRIPTION: Option<&'static str> = Some("A real-time spectrum analyser");
    const CLAP_MANUAL_URL: Option<&'static str> = Some(Self::URL);
    const CLAP_SUPPORT_URL: Option<&'static str> = None;

    // Don't forget to change these features
    const CLAP_FEATURES: &'static [ClapFeature] = &[ClapFeature::Analyzer, ClapFeature::Stereo];
}

impl Vst3Plugin for SAPlugin {
    const VST3_CLASS_ID: [u8; 16] = *b"Exactly16Chars!!";

    // And also don't forget to change these categories
    const VST3_SUBCATEGORIES: &'static [Vst3SubCategory] =
        &[Vst3SubCategory::Analyzer, Vst3SubCategory::Tools];
}

nih_export_clap!(SAPlugin);
