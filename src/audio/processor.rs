use nih_plug::buffer::Buffer;

use crate::audio::buffer::WaveformBuffer;
use crate::audio::fft::FftProcessor;
use std::sync::{Arc, Mutex, RwLock};

pub struct AudioProcessor {
    // Stores the last 2048 audio samples in a lock-free triple buffer
    // Used as input for the FFT to generate frequency spectrum
    waveform_buffer: Arc<Mutex<WaveformBuffer>>,

    // Temporary storage for converting stereo to mono within process_buffer()
    // Pre-allocated to avoid allocations in the audio thread (real-time constraint)
    mono_scratch: Vec<f32>,

    // Performs Fast Fourier Transform to convert audio samples to frequency data
    // Takes 2048 time-domain samples, outputs 1025 frequency bins
    fft_processor: FftProcessor,
}

impl AudioProcessor {
    pub fn new(waveform_buffer: Arc<Mutex<WaveformBuffer>>, max_buffer_size: usize) -> Self {
        // FFT of N real samples produces N/2 + 1 complex frequency bins
        // let fft_output_size = WAVEFORM_BUFFER_SIZE / 2 + 1;
        Self {
            waveform_buffer,
            mono_scratch: Vec::with_capacity(max_buffer_size), // Pre-allocate reasonable size
            fft_processor: FftProcessor::new(),
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

        if let Ok(mut buffer) = self.waveform_buffer.lock() {
            buffer.write_samples(&self.mono_scratch);

            // Get the latest samples for FFT processing
            // read_samples() returns the last 2048 samples
            let samples_for_fft = buffer.read_samples();

            // Drop the lock before doing FFT (release it ASAP)
            drop(buffer);

            // Run FFT on the samples to get frequency data
            let spectrum = self.fft_processor.process(&samples_for_fft);

            // Log FFT data occasionally to check it's working
            static mut FFT_LOG_COUNTER: u32 = 0;
            unsafe {
                FFT_LOG_COUNTER += 1;
                if FFT_LOG_COUNTER >= 1000 {
                    // Log every ~10 seconds at 100Hz
                    FFT_LOG_COUNTER = 0;
                    if spectrum.len() > 10 {
                        let max_magnitude =
                            spectrum.iter().take(100).fold(0.0f32, |a, &b| a.max(b));
                        nih_plug::nih_log!(
                            "FFT: max magnitude in first 100 bins: {} dB",
                            max_magnitude
                        );
                    }
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
