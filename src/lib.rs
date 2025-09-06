mod audio;
mod editor;
mod ui;

use crate::audio::sample_buffer_engine;
use atomic_float::AtomicF32;
use audio::audio_engine::AudioEngine;
use editor::EditorInitFlags;
use editor::PluginEditor;
use nih_plug::prelude::*;
use nih_plug_iced::{create_iced_editor, IcedState};
use std::sync::{atomic::Ordering, Arc, Mutex, RwLock};

struct PluginLearn {
    params: Arc<PluginLearnParams>,
    sample_buffer_engine: Arc<Mutex<sample_buffer_engine::SampleBufferEngine>>,
    audio_engine: Option<Arc<Mutex<AudioEngine>>>,
    iced_state: Arc<IcedState>,
    spectrum_data: Arc<RwLock<Vec<f32>>>,

    /// Peak level tracking for the level meters (left, right)
    peak_level_left: Arc<AtomicF32>,
    peak_level_right: Arc<AtomicF32>,
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
        Self {
            params: Arc::new(PluginLearnParams::default()),
            sample_buffer_engine: Arc::new(Mutex::new(
                sample_buffer_engine::SampleBufferEngine::new(),
            )),
            audio_engine: None,
            iced_state: IcedState::from_size(800, 600),
            spectrum_data: Arc::new(RwLock::new(vec![0.0; 1025])),
            peak_level_left: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
            peak_level_right: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
        }
    }
}

impl Default for PluginLearnParams {
    fn default() -> Self {
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
        // Use nih_plug's logging system properly
        nih_plug::nih_log!(
            "Plugin initialize called, buffer_size: {}",
            buffer_config.max_buffer_size
        );

        // Resize buffers and perform other potentially expensive initialization operations here.
        // The `reset()` function is always called right after this function. You can remove this
        // function if you do not need it.
        self.audio_engine = Some(Arc::new(Mutex::new(AudioEngine::new(
            self.sample_buffer_engine.clone(),
            buffer_config.max_buffer_size as usize,
        ))));

        nih_plug::nih_log!("Plugin initialized successfully");
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
        if let Some(processor) = &mut self.audio_engine {
            // First, collect pre-gain audio for FFT analysis
            let mut processor = processor.lock().unwrap();

            // Get the current "target" gain value (not smoothed) for future pre/post gain visualization
            let _target_gain = self.params.gain.value();

            // Process the buffer to collect PRE-GAIN samples for FFT
            processor.process_buffer_pre_gain(buffer, self.spectrum_data.clone());
            drop(processor);

            // Now apply smoothed gain to the actual output (per-sample for smooth transition)
            let mut peak_left = 0.0f32;
            let mut peak_right = 0.0f32;

            for (_sample_idx, mut channel_samples) in buffer.iter_samples().enumerate() {
                // Call next() for EACH sample - this is what makes gain changes smooth
                let gain = self.params.gain.smoothed.next();

                for (channel_idx, sample) in channel_samples.iter_mut().enumerate() {
                    *sample *= gain;

                    // Track post-gain peak levels for meters
                    let abs_sample = sample.abs();
                    match channel_idx {
                        0 => peak_left = peak_left.max(abs_sample), // Left channel
                        1 => peak_right = peak_right.max(abs_sample), // Right channel
                        _ => {}                                     // Ignore additional channels
                    }
                }
            }

            // Convert to dB and update atomic levels for the UI
            let left_db = if peak_left > 0.0 {
                util::gain_to_db(peak_left)
            } else {
                util::MINUS_INFINITY_DB
            };
            let right_db = if peak_right > 0.0 {
                util::gain_to_db(peak_right)
            } else {
                util::MINUS_INFINITY_DB
            };

            self.peak_level_left.store(left_db, Ordering::Relaxed);
            self.peak_level_right.store(right_db, Ordering::Relaxed);
        }
        ProcessStatus::Normal
    }

    fn editor(&mut self, _async_executor: AsyncExecutor<Self>) -> Option<Box<dyn Editor>> {
        nih_plug::nih_log!("Editor requested");

        // Editor can be requested before initialize() is called, so we need to handle
        // the case where audio_engine is None by creating one for the editor
        let audio_engine = match &self.audio_engine {
            Some(processor) => {
                nih_plug::nih_log!("Using existing audio_engine");
                processor.clone()
            }
            None => {
                nih_plug::nih_log!("Creating new audio_engine for editor");
                // Create audio processor for the editor if not initialized yet
                Arc::new(Mutex::new(AudioEngine::new(
                    self.sample_buffer_engine.clone(),
                    1024, // default buffer size
                )))
            }
        };

        let init_flags = EditorInitFlags {
            audio_engine,
            params: self.params.clone(),
            spectrum_data: self.spectrum_data.clone(),
            peak_level_left: self.peak_level_left.clone(),
            peak_level_right: self.peak_level_right.clone(),
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
