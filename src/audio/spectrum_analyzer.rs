use crate::audio::constants;
use apodize::blackman_iter;
use nih_plug::prelude::*;
use realfft::{num_complex::Complex32, RealFftPlanner, RealToComplex};
use std::sync::Arc;
use triple_buffer::TripleBuffer;

/// The size of our FFT analysis window
pub const SPECTRUM_WINDOW_SIZE: usize = 2048;

/// Number of frequency bins produced by the FFT (N/2 + 1 for real FFT)
pub const SPECTRUM_BINS: usize = SPECTRUM_WINDOW_SIZE / 2 + 1;

/// Spectrum analyzer floor prevents log(0) in FFT calculations
const SPECTRUM_FLOOR_DB: f32 = -120.0;

/// Time constant for spectrum attack (fast response to increases)
const SPECTRUM_ATTACK: f32 = 0.1;

/// Time constant for spectrum release (slow decay)  
const SPECTRUM_RELEASE: f32 = 0.01;

/// The spectrum analyzer's frequency data - array of magnitude values in dB
pub type SpectrumData = [f32; SPECTRUM_BINS];

/// Cloneable wrapper for spectrum output channel (UI thread reads from this)
/// Uses Arc<Mutex<>> wrapper to allow cloning for editor initialization
#[derive(Clone)]
pub struct SpectrumOutput {
    output: Arc<std::sync::Mutex<triple_buffer::Output<SpectrumData>>>,
}

impl SpectrumOutput {
    fn new(output: triple_buffer::Output<SpectrumData>) -> Self {
        Self {
            output: Arc::new(std::sync::Mutex::new(output)),
        }
    }

    /// Read latest spectrum data for UI display
    /// Called from UI thread only
    pub fn read(&self) -> SpectrumData {
        if let Ok(mut output) = self.output.try_lock() {
            *output.read()
        } else {
            // Return silence if unable to lock (shouldn't happen in normal operation)
            [SPECTRUM_FLOOR_DB; SPECTRUM_BINS]
        }
    }
}

/// Continuously computes frequency spectrum and sends to [`SpectrumOutput`] (audio thread writes to this)
pub struct SpectrumAnalyzer {
    /// FFT processing engine
    fft_processor: Arc<dyn RealToComplex<f32>>,

    /// Pre-computed Blackman window for spectral leakage reduction
    window_function: Vec<f32>,

    /// Input buffer for windowed samples (time domain)
    time_domain_buffer: Vec<f32>,

    /// Output buffer for FFT results (frequency domain)
    frequency_domain_buffer: Vec<Complex32>,

    /// Current spectrum result with smoothing applied
    spectrum_result: SpectrumData,

    /// Previous spectrum for smoothing calculations
    previous_spectrum: SpectrumData,

    /// Triple buffer producer for lock-free communication to UI
    spectrum_producer: triple_buffer::Input<SpectrumData>,
}

impl SpectrumAnalyzer {
    /// Create a new spectrum analyzer and output pair
    /// Returns (analyzer for audio thread, output for UI thread)
    pub fn new() -> (Self, SpectrumOutput) {
        // Create lock-free communication channel
        let (spectrum_producer, spectrum_consumer) =
            TripleBuffer::new(&[SPECTRUM_FLOOR_DB; SPECTRUM_BINS]).split();

        // Initialize FFT processor
        let mut fft_planner = RealFftPlanner::<f32>::new();
        let fft_processor = fft_planner.plan_fft_forward(SPECTRUM_WINDOW_SIZE);

        // Pre-compute Blackman window for better frequency resolution
        // Blackman window provides good side-lobe suppression for spectrum analysis
        let window_function: Vec<f32> = blackman_iter(SPECTRUM_WINDOW_SIZE)
            .map(|w| w as f32)
            .collect();

        let analyzer = Self {
            fft_processor,
            window_function,
            time_domain_buffer: vec![0.0; SPECTRUM_WINDOW_SIZE],
            frequency_domain_buffer: vec![Complex32::new(0.0, 0.0); SPECTRUM_BINS],
            spectrum_result: [SPECTRUM_FLOOR_DB; SPECTRUM_BINS],
            previous_spectrum: [SPECTRUM_FLOOR_DB; SPECTRUM_BINS],
            spectrum_producer,
        };

        (analyzer, SpectrumOutput::new(spectrum_consumer))
    }

    /// Compute spectrum from audio buffer and send to UI thread
    /// Called from audio thread - must be real-time safe (no allocations)
    pub fn process(&mut self, buffer: &Buffer, sample_rate: f32) {
        // Extract mono mix from stereo buffer for spectrum analysis
        // This follows the same pattern as professional spectrum analyzers
        self.extract_mono_samples(buffer);

        // Apply windowing to reduce spectral leakage
        self.apply_window();

        // Perform FFT: time domain -> frequency domain
        if let Err(_) = self.fft_processor.process(
            &mut self.time_domain_buffer,
            &mut self.frequency_domain_buffer,
        ) {
            // FFT failed - skip this frame to maintain real-time safety
            return;
        }

        // Convert complex FFT output to magnitude spectrum in dB
        self.compute_magnitude_spectrum(sample_rate);

        // Apply perceptual smoothing (attack/release envelope)
        self.apply_spectrum_smoothing();

        // Send result to UI thread (lock-free)
        self.spectrum_producer.write(self.spectrum_result);
    }

    /// Extract mono mix from stereo buffer for spectral analysis
    /// Professional spectrum analyzers typically analyze the sum of all channels
    fn extract_mono_samples(&mut self, buffer: &Buffer) {
        let num_channels = buffer.channels();
        let num_samples = buffer.samples().min(SPECTRUM_WINDOW_SIZE);

        if num_channels == 0 {
            return;
        }

        // Clear the time domain buffer first
        for i in 0..SPECTRUM_WINDOW_SIZE {
            self.time_domain_buffer[i] = 0.0;
        }

        // Get immutable access to channel slices
        let channel_slices = buffer.as_slice_immutable();

        // Sum all channels into mono mix
        for channel_idx in 0..num_channels {
            let channel = &channel_slices[channel_idx];
            for (sample_idx, &sample) in channel.iter().enumerate().take(num_samples) {
                self.time_domain_buffer[sample_idx] += sample;
            }
        }

        // Normalize by channel count
        let normalization = 1.0 / num_channels as f32;
        for sample in self.time_domain_buffer.iter_mut().take(num_samples) {
            *sample *= normalization;
        }

        // Remaining samples are already zero-padded from the clear above
    }

    /// Apply Blackman window to reduce spectral leakage
    /// Essential for accurate frequency analysis
    fn apply_window(&mut self) {
        for (sample, &window_coeff) in self
            .time_domain_buffer
            .iter_mut()
            .zip(self.window_function.iter())
        {
            *sample *= window_coeff;
        }
    }

    /// Convert complex FFT output to magnitude spectrum in dB
    fn compute_magnitude_spectrum(&mut self, sample_rate: f32) {
        for (bin_idx, &complex_bin) in self.frequency_domain_buffer.iter().enumerate() {
            // Calculate magnitude: sqrt(re² + im²)
            let magnitude =
                (complex_bin.re * complex_bin.re + complex_bin.im * complex_bin.im).sqrt();

            // Normalize by square root of FFT size for proper scaling (standard FFT normalization)
            let normalized_magnitude = magnitude / (SPECTRUM_WINDOW_SIZE as f32).sqrt();

            // Convert to dB with proper floor to avoid log(0)
            // Add gain compensation to match professional analyzer levels
            // Pro-Q and similar analyzers apply significant gain compensation
            let db_value = if normalized_magnitude > 0.0 {
                20.0 * normalized_magnitude.log10() + 36.0 // +36dB total compensation to match Pro-Q levels
            } else {
                SPECTRUM_FLOOR_DB
            };

            self.spectrum_result[bin_idx] = db_value.max(SPECTRUM_FLOOR_DB);
            if db_value > -10.0 && db_value < 10.0 {
                // Only log reasonable peaks
                nih_plug::nih_log!(
                    "High energy at bin {}: freq={:.1}Hz, magnitude={:.6}, dB={:.2}",
                    bin_idx,
                    display_utils::bin_to_frequency(bin_idx, sample_rate),
                    normalized_magnitude,
                    db_value
                );
            }
        }
    }

    /// Apply perceptual smoothing with attack/release characteristics
    /// Similar to analog spectrum analyzers and professional plugins like Pro-Q
    fn apply_spectrum_smoothing(&mut self) {
        for bin_idx in 0..SPECTRUM_BINS {
            let current_db = self.spectrum_result[bin_idx];
            let previous_db = self.previous_spectrum[bin_idx];

            let smoothed_db = if current_db > previous_db {
                // Attack: fast response to increases (like a peak meter)
                previous_db + (current_db - previous_db) * SPECTRUM_ATTACK
            } else {
                // Release: slow decay (prevents flickering, easier to read)
                previous_db + (current_db - previous_db) * SPECTRUM_RELEASE
            };

            self.spectrum_result[bin_idx] = smoothed_db;
            self.previous_spectrum[bin_idx] = smoothed_db;
        }
    }
}

/// Utility functions for spectrum display calculations
pub mod display_utils {
    use super::*;

    /// Apply A-weighting for perceptually accurate frequency response
    /// Based on IEC 61672-1:2013 standard - used in professional audio measurement
    pub fn apply_a_weighting(frequency_hz: f32, magnitude_db: f32) -> f32 {
        if frequency_hz <= 0.0 {
            return magnitude_db - 50.0; // Heavily attenuate invalid frequencies
        }

        let f = frequency_hz as f64;
        let f2 = f * f;
        let f4 = f2 * f2;

        // A-weighting transfer function (IEC 61672-1 standard)
        let numerator = 12194.0_f64.powi(2) * f4;
        let denominator = (f2 + 20.6_f64.powi(2))
            * (f2 + 12194.0_f64.powi(2))
            * (f2 + 107.7_f64.powi(2)).sqrt()
            * (f2 + 737.9_f64.powi(2)).sqrt();

        if denominator == 0.0 {
            return magnitude_db - 50.0;
        }

        let response_amplitude = numerator / denominator;
        let a_weighting_db = 20.0 * response_amplitude.log10() + 2.00; // +2dB normalization

        magnitude_db + a_weighting_db as f32
    }

    /// Calculate frequency for a given bin index
    /// Used by display components for frequency axis labeling
    pub fn bin_to_frequency(bin_index: usize, sample_rate: f32) -> f32 {
        (bin_index as f32 * sample_rate) / (2.0 * SPECTRUM_WINDOW_SIZE as f32)
    }

    /// Convert linear frequency to logarithmic display position (like Pro-Q)
    /// Maps 20Hz-20kHz logarithmically for musical frequency perception
    pub fn frequency_to_display_position(frequency_hz: f32) -> f32 {
        let min_freq = constants::MIN_FREQUENCY;
        let max_freq = constants::MAX_FREQUENCY;

        if frequency_hz <= min_freq {
            0.0
        } else if frequency_hz >= max_freq {
            1.0
        } else {
            (frequency_hz / min_freq).log10() / (max_freq / min_freq).log10()
        }
    }

    /// Convert display position back to frequency (inverse of above)
    pub fn display_position_to_frequency(position: f32) -> f32 {
        let min_freq = constants::MIN_FREQUENCY;
        let max_freq = constants::MAX_FREQUENCY;
        let log_range = (max_freq / min_freq).log10();

        min_freq * 10.0_f32.powf(position * log_range)
    }
}
