use crate::audio::constants::WAVEFORM_BUFFER_SIZE;
use apodize::blackman_iter;
use realfft::{num_complex::Complex32, RealFftPlanner, RealToComplex};
use std::sync::Arc;

pub struct FftEngine {
    // FFT planner and instance
    planner: RealFftPlanner<f32>,
    fft: Arc<dyn RealToComplex<f32>>,

    //Buffers
    input_buffer: Vec<f32>,        // Windowed samples (2048
    output_buffer: Vec<Complex32>, // FFT output (~1025 bins)

    // Window function (precomputed for efficiency)
    window: Vec<f32>,

    // FFT size
    size: usize,
}

impl FftEngine {
    pub fn new() -> Self {
        let size = WAVEFORM_BUFFER_SIZE;

        // Create FFT planner and get FFT instance
        let mut planner = RealFftPlanner::<f32>::new();
        let fft = planner.plan_fft_forward(size);

        // Pre-compute Hann window
        let window: Vec<f32> = blackman_iter(size).map(|w| w as f32).collect();

        // Pre-allocate buffers
        let input_buffer = vec![0.0; size];

        // Real FFT of size N produces N/2+1 complex outputs
        let output_size = size / 2 + 1;
        let output_buffer = vec![Complex32::new(0.0, 0.0); output_size];

        Self {
            planner,
            fft,
            input_buffer,
            output_buffer,
            window,
            size,
        }
    }

    /// Process audio samples and return frequency spectrum
    /// Input: slice of audio samples (should be same length as FFT size)
    /// Output: Vec of magnitudes in dB (length will be size/2 + 1
    pub fn process(&mut self, audio_samples: &[f32]) -> Vec<f32> {
        // Step 1: Apply window function to input
        for (i, sample) in audio_samples.iter().enumerate().take(self.size) {
            self.input_buffer[i] = sample * self.window[i];
        }

        // Step 2: Run FFT (time domain -> frequency domain)
        self.fft
            .process(&mut self.input_buffer, &mut self.output_buffer)
            .expect("FFT processing failed");

        // Step 3: Calculate magnitudes and convert to dB
        let mut magnitudes = Vec::with_capacity(self.output_buffer.len());

        for complex_sample in &self.output_buffer {
            let magnitude = (complex_sample.re * complex_sample.re
                + complex_sample.im * complex_sample.im)
                .sqrt()
                / (self.size as f32).sqrt();

            // Convert to decibels (with floor to avoid log(0))
            let db = 20.0 * (magnitude.max(1e-10).log10());
            magnitudes.push(db);
        }
        magnitudes
    }
}
