use nih_plug::buffer::Buffer;

use crate::audio::fft_engine::FftEngine;
use crate::audio::sample_buffer_engine::SampleBufferEngine;
use std::sync::{Arc, Mutex, RwLock};

pub struct AudioEngine {
    // Stores the last 2048 audio samples in a lock-free triple buffer
    // Used as input for the FFT to generate frequency spectrum
    sample_buffer_engine: Arc<Mutex<SampleBufferEngine>>,
    // Performs Fast Fourier Transform to convert audio samples to frequency data
    // Takes 2048 time-domain samples, outputs 1025 frequency bins
    fft_engine: FftEngine,
    // Temporary storage for converting stereo to mono within process_buffer()
    // Pre-allocated to avoid allocations in the audio thread (real-time constraint)
    mono_scratch: Vec<f32>,
}

impl AudioEngine {
    pub fn new(
        sample_buffer_engine: Arc<Mutex<SampleBufferEngine>>,
        max_buffer_size: usize,
    ) -> Self {
        Self {
            sample_buffer_engine,
            mono_scratch: Vec::with_capacity(max_buffer_size), // Pre-allocate reasonable size
            fft_engine: FftEngine::new(),
        }
    }

    pub fn process_buffer_pre_gain(
        &mut self,
        buffer: &mut Buffer,
        spectrum_data: Arc<RwLock<Vec<f32>>>,
    ) {
        self.mono_scratch.clear();

        // Process PRE-GAIN audio for spectrum analysis (we won't modify the buffer)
        for channel_samples in buffer.iter_samples() {
            // Collect into small fixed array (no heap allocation!)
            let mut channel_values = [0.0f32; 2];
            let mut channel_count = 0;

            // Read samples without modifying them
            for (i, sample) in channel_samples.into_iter().enumerate() {
                if i < 2 {
                    channel_values[i] = *sample;
                    channel_count += 1;
                }
            }
            // Calculate mono from our fixed array
            let mono = if channel_count >= 2 {
                (channel_values[0] + channel_values[1]) * 0.5
            } else if channel_count >= 1 {
                channel_values[0]
            } else {
                0.0
            };

            self.mono_scratch.push(mono);
        }

        if let Ok(mut buffer) = self.sample_buffer_engine.lock() {
            buffer.write_samples(&self.mono_scratch);

            // Get the latest samples for FFT processing
            // read_samples() returns the last 2048 samples
            let samples_for_fft = buffer.read_samples();

            // Drop the lock before doing FFT (release it ASAP)
            drop(buffer);

            // Run FFT on the samples to get frequency data
            let spectrum = self.fft_engine.process(&samples_for_fft);

            // Log FFT data occasionally to understand what we're getting
            static mut FFT_LOG_COUNTER: u32 = 0;
            unsafe {
                FFT_LOG_COUNTER += 1;
                if FFT_LOG_COUNTER >= 500 {
                    // Log every 5 seconds
                    FFT_LOG_COUNTER = 0;

                    nih_plug::nih_log!("=== FFT Data Analysis ===");
                    nih_plug::nih_log!("Total bins: {}", spectrum.len());

                    // Show first 10 bins (lowest frequencies)
                    for i in 0..10 {
                        if i < spectrum.len() {
                            let raw_value = spectrum[i];
                            let db_value = if raw_value > 0.0 {
                                20.0 * raw_value.log10()
                            } else {
                                -100.0
                            };
                            nih_plug::nih_log!(
                                "Bin {}: raw={:.6}, dB={:.1}",
                                i,
                                raw_value,
                                db_value
                            );
                        }
                    }

                    // Find the loudest bin
                    let mut max_value = 0.0;
                    let mut max_bin = 0;
                    for (i, &value) in spectrum.iter().enumerate() {
                        if value > max_value {
                            max_value = value;
                            max_bin = i;
                        }
                    }

                    let max_db = if max_value > 0.0 {
                        20.0 * max_value.log10()
                    } else {
                        -100.0
                    };

                    nih_plug::nih_log!(
                        "Loudest: Bin {} = {:.6} ({:.1} dB)",
                        max_bin,
                        max_value,
                        max_db
                    );
                }
            }

            // Update the shared spectrum data for the UI
            // write() gets exclusive write access
            if let Ok(mut spectrum_data) = spectrum_data.write() {
                // Copy the new spectrum into the shared buffer
                *spectrum_data = spectrum;
            }
        }
    }
}
