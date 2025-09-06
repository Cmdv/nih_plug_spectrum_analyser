use crate::audio::constants;
use std::sync::{Arc, RwLock};

/// Pure audio processing logic for spectrum analysis
/// Handles FFT data processing, smoothing, and A-weighting
/// Separates audio logic from UI rendering
pub struct SpectrumEngine {
    // Frequency data from FFT (shared between threads)
    frequency_bins: Arc<RwLock<Vec<f32>>>,

    // Internal processing state (not shared - owned by engine)
    smoothed_bins: Vec<f32>,
}

impl SpectrumEngine {
    pub fn new(frequency_bins: Arc<RwLock<Vec<f32>>>) -> Self {
        Self {
            frequency_bins,
            smoothed_bins: Vec::new(),
        }
    }

    /// Update processing - apply smoothing and other audio processing
    /// Call this from UI thread before rendering
    pub fn update(&mut self) {
        let bins = match self.frequency_bins.read() {
            Ok(data) => data,
            Err(_) => return, // Lock poisoned, skip this update
        };

        if bins.len() < 2 {
            return;
        }

        // Initialize smoothed_bins if needed
        if self.smoothed_bins.len() != bins.len() {
            self.smoothed_bins = vec![-100.0; bins.len()];
        }

        // Apply smoothing with different attack/release times
        let attack = constants::SPECTRUM_ATTACK; // Fast attack
        let release = constants::SPECTRUM_RELEASE; // Slow release
        for i in 0..bins.len() {
            if bins[i] > self.smoothed_bins[i] {
                // Attack: follow quickly when louder
                self.smoothed_bins[i] = bins[i] * attack + self.smoothed_bins[i] * (1.0 - attack);
            } else {
                // Release: decay slowly when quieter
                self.smoothed_bins[i] = bins[i] * release + self.smoothed_bins[i] * (1.0 - release);
            }
        }

        // Log spectrum data occasionally
        static mut DRAW_LOG_COUNTER: u32 = 0;
        unsafe {
            DRAW_LOG_COUNTER += 1;
            if DRAW_LOG_COUNTER >= 600 {
                // Log every ~10 seconds at 60fps
                DRAW_LOG_COUNTER = 0;
                let max_val = bins.iter().take(100).fold(0.0f32, |a, &b| a.max(b));
                nih_plug::nih_log!(
                    "Processing spectrum, max value in first 100 bins: {}",
                    max_val
                );
            }
        }
    }

    /// Get processed spectrum data for rendering
    /// Returns a copy of smoothed frequency bins
    pub fn get_spectrum_data(&self) -> Vec<f32> {
        self.smoothed_bins.clone()
    }

    /// Apply A-weighting to frequency response for perceptual accuracy
    /// Based on IEC 61672-1:2013 standard
    pub fn apply_a_weighting(freq_hz: f32, db_value: f32) -> f32 {
        if freq_hz <= 0.0 {
            return db_value - 50.0; // Heavily attenuate invalid frequencies
        }

        let f = freq_hz as f64;
        let f2 = f * f;
        let f4 = f2 * f2;

        // A-weighting formula (IEC 61672-1 standard)
        let numerator = 12194.0_f64.powi(2) * f4;
        let denominator = (f2 + 20.6_f64.powi(2))
            * (f2 + 12194.0_f64.powi(2))
            * (f2 + 107.7_f64.powi(2)).sqrt()
            * (f2 + 737.9_f64.powi(2)).sqrt();

        if denominator == 0.0 {
            return db_value - 50.0;
        }

        let ra = numerator / denominator;
        let a_weighting_db = 20.0 * ra.log10() + 2.00; // +2dB normalization

        db_value + a_weighting_db as f32
    }

    /// Calculate a spectrum point with logarithmic frequency scaling and A-weighting
    /// Used by display components for rendering
    pub fn calculate_spectrum_point(bins: &[f32], i: usize, num_points: usize) -> (f32, f32) {
        // Logarithmic frequency mapping (like Pro-Q 3)
        // Map 20Hz to 20kHz logarithmically across the display
        let min_freq = constants::MIN_FREQUENCY;
        let max_freq = constants::MAX_FREQUENCY;
        let nyquist = constants::NYQUIST_FREQUENCY;

        // Calculate the frequency for this display point (logarithmic)
        let norm_pos = i as f32 / num_points as f32;
        let freq = min_freq * (max_freq / min_freq as f32).powf(norm_pos);

        // Convert frequency to bin index with interpolation
        let bin_position = (freq / nyquist) * bins.len() as f32;
        let bin_index = bin_position.floor() as usize;
        let bin_fraction = bin_position.fract(); // For interpolation

        // Get interpolated dB value
        let raw_db_value = if bin_index + 1 < bins.len() {
            // Linear interpolation between two bins
            let current_bin = bins[bin_index];
            let next_bin = bins[bin_index + 1];
            current_bin + (next_bin - current_bin) * bin_fraction
        } else if bin_index < bins.len() {
            bins[bin_index]
        } else {
            -100.0
        };

        // Apply A-weighting for perceptual accuracy (like Pro-Q 3)
        let db_value = Self::apply_a_weighting(freq, raw_db_value);

        // Return frequency and processed dB value
        (freq, db_value)
    }
}
