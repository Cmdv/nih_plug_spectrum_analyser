/// Window functions for FFT spectral analysis
///
/// This module provides various window functions and adaptive windowing
/// strategies for optimizing spectrum analysis at different frequency ranges.
use core::f32::consts::PI;
use libm::cosf;

/// Pre-computed window function data for efficient FFT processing
///
/// Window functions shape audio signals before FFT to reduce spectral leakage.
/// This struct holds both the window shape (coefficients) and the compensation
/// factor (coherent_gain) needed to restore correct amplitude measurements.
///
/// # Why Pre-compute?
/// Computing window functions involves expensive trig operations (sin/cos).
/// By pre-computing once at initialization, we avoid these calculations
/// in the real-time audio thread.
pub struct WindowData {
    /// Window function values [0.0..1.0] that multiply with audio samples
    /// Length matches FFT size (e.g., 2048 values for 2048-point FFT)
    /// Edge values approach 0.0 (fade out), center values approach 1.0 (preserve signal)
    pub coefficients: Vec<f32>,

    /// Average window value, used to compensate for amplitude reduction
    /// Since windows reduce signal energy (most values < 1.0), we need this
    /// to restore correct dB measurements after FFT
    /// Typical values: Hann ~0.5, Blackman ~0.42, Rectangular 1.0
    pub coherent_gain: f32,
}

/// Collection of pre-computed windows optimized for different frequency ranges
///
/// Different frequency ranges benefit from different window characteristics:
/// - Low frequencies: Need sharp frequency resolution (narrow window main lobe)
/// - Mid frequencies: Balanced resolution and leakage suppression
/// - High frequencies: Need clean display (strong sidelobe suppression)
///
/// By using different windows and blending their results, we get optimal
/// spectrum analysis across the entire frequency range.
pub struct AdaptiveWindows {
    /// Window optimized for bass frequencies (20-500Hz)
    /// Prioritizes frequency resolution to distinguish close bass notes
    pub low_freq: WindowData,

    /// Window for midrange frequencies (500-5000Hz)
    /// Balanced trade-off between resolution and spectral leakage
    pub mid_freq: WindowData,

    /// Window for treble frequencies (5000Hz+)
    /// Prioritizes clean display with minimal spectral artifacts
    pub high_freq: WindowData,
}

/// Window function types for FFT analysis
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum WindowType {
    /// Rectangular: No windowing, maximum frequency resolution
    #[allow(dead_code)]
    Rectangular,
    /// Hann: Good general-purpose balance
    Hann,
    /// Hamming: Better sidelobe suppression
    #[allow(dead_code)]
    Hamming,
    /// Blackman: Excellent sidelobe suppression, wider main lobe
    Blackman,
}

impl WindowType {
    /// Generate window coefficients for this window type
    pub fn generate(self, window_size: usize) -> Vec<f32> {
        match self {
            Self::Rectangular => vec![1.0; window_size],
            Self::Hann => generate_hann_window(window_size),
            Self::Hamming => generate_hamming_window(window_size),
            Self::Blackman => generate_blackman_window(window_size),
        }
    }

    /// Get the coherent gain for this window type
    pub fn coherent_gain(self, coefficients: &[f32]) -> f32 {
        coefficients.iter().sum::<f32>() / coefficients.len() as f32
    }
}

/// Generates Hann window coefficients for reducing spectral leakage in FFT analysis
///
/// The Hann window (named after Julius von Hann) tapers signal edges to zero using a
/// raised cosine function. This reduces discontinuities at frame boundaries that cause
/// spectral leakage - the spreading of energy across frequency bins.
///
/// # Parameters
/// * `window_size` - Number of samples in the FFT window (typically power of 2)
///
/// # Returns
/// Vector of window coefficients [0.0..1.0] to multiply with time-domain samples
///
/// # Mathematical Background
/// Hann formula: w[n] = 0.5 * (1 - cos(2πn/N)) where n=[0..N-1]
/// - Main lobe width: 4 bins (2x wider than rectangular window)
/// - Sidelobe suppression: -31.5 dB (good balance)
/// - Coherent gain: 0.5 (50% amplitude reduction)
/// - Scalloping loss: 1.42 dB (frequency response between bins)
///
/// # Trade-offs
/// - Better frequency isolation than rectangular window
/// - Slightly wider peaks than rectangular (4 bins vs 2 bins)
/// - Good general-purpose window for audio analysis
pub fn generate_hann_window(window_size: usize) -> Vec<f32> {
    let window_size_f32 = window_size as f32;

    (0..window_size)
        .map(|i| {
            let position = i as f32 / window_size_f32;
            0.5 * (1.0 - cosf(2.0 * PI * position))
        })
        .collect()
}

/// Generates Hamming window coefficients for improved sidelobe suppression
///
/// The Hamming window provides better sidelobe suppression (-41dB) than Hann
/// at the cost of slightly worse rolloff (6dB/octave vs 18dB/octave).
/// Optimized coefficients (0.54, 0.46) minimize the first sidelobe.
///
/// # Mathematical Background
/// Hamming formula: w[n] = 0.54 - 0.46*cos(2πn/N)
/// - Main lobe width: 4 bins (same as Hann)
/// - First sidelobe: -41dB (vs -31dB for Hann)
/// - Rolloff: 6dB/octave (vs 18dB/octave for Hann)
///
/// # When to Use
/// - Better for detecting weak signals near strong ones
/// - Good for harmonic analysis where sidelobe rejection matters
/// - Preferred when frequency accuracy more important than amplitude accuracy
pub fn generate_hamming_window(window_size: usize) -> Vec<f32> {
    let window_size_f32 = window_size as f32;

    (0..window_size)
        .map(|i| {
            let position = i as f32 / window_size_f32;
            0.54 - 0.46 * cosf(2.0 * PI * position)
        })
        .collect()
}

/// Generates Blackman window coefficients for excellent sidelobe suppression
///
/// The Blackman window provides excellent sidelobe suppression (-58dB) at the
/// cost of a wider main lobe (6 bins vs 4 for Hann/Hamming).
///
/// # Mathematical Background
/// Blackman formula: w[n] = 0.42 - 0.5*cos(2πn/N) + 0.08*cos(4πn/N)
/// - Main lobe width: 6 bins (50% wider than Hann)
/// - First sidelobe: -58dB (excellent suppression)
/// - Good for situations requiring minimal spectral leakage
///
/// # When to Use
/// - High-frequency analysis where leakage is problematic
/// - When you need clean spectrum display
/// - Trade frequency resolution for cleaner appearance
pub fn generate_blackman_window(window_size: usize) -> Vec<f32> {
    let window_size_f32 = window_size as f32;

    (0..window_size)
        .map(|i| {
            let position = i as f32 / window_size_f32;
            0.42 - 0.5 * cosf(2.0 * PI * position) + 0.08 * cosf(4.0 * PI * position)
        })
        .collect()
}

/// Adaptive window selector based on frequency analysis goals
///
/// Different frequency ranges benefit from different window characteristics:
/// - Low frequencies: Need sharp frequency resolution (narrow main lobe)
/// - High frequencies: Need clean display (low sidelobes)
pub struct AdaptiveWindowStrategy {
    /// Window for low frequencies (20-500Hz)
    pub low_freq_window: WindowType,
    /// Window for mid frequencies (500-5000Hz)  
    pub mid_freq_window: WindowType,
    /// Window for high frequencies (5000Hz+)
    pub high_freq_window: WindowType,
    /// Crossfade regions for smooth transitions (Hz)
    pub low_mid_crossfade: (f32, f32),
    pub mid_high_crossfade: (f32, f32),
}

impl Default for AdaptiveWindowStrategy {
    fn default() -> Self {
        Self {
            // Use Hann for everything initially to test blending
            low_freq_window: WindowType::Hann,
            mid_freq_window: WindowType::Hann,
            high_freq_window: WindowType::Blackman,
            // Narrower crossfade regions to reduce artifacts
            low_mid_crossfade: (480.0, 520.0),
            mid_high_crossfade: (8000.0, 9000.0),
        }
    }
}

impl AdaptiveWindowStrategy {
    /// Blends three frequency-optimized spectrums into one composite spectrum
    ///
    /// Each spectrum was computed with a window optimized for different frequency ranges.
    /// This function smoothly blends them together based on frequency to get the best
    /// characteristics from each window type.
    ///
    /// # Parameters
    /// * `low_spectrum` - Spectrum computed with low-frequency optimized window
    /// * `mid_spectrum` - Spectrum computed with mid-frequency optimized window  
    /// * `high_spectrum` - Spectrum computed with high-frequency optimized window
    /// * `sample_rate` - Sample rate for frequency calculations
    /// * `window_size` - FFT window size for frequency calculations
    ///
    /// # Returns
    /// Blended spectrum combining the best parts of each window's analysis
    pub fn blend_frequency_spectrums(
        &self,
        low_spectrum: &[f32],
        mid_spectrum: &[f32],
        high_spectrum: &[f32],
        sample_rate: f32,
        window_size: usize,
    ) -> Vec<f32> {
        let mut blended = Vec::with_capacity(low_spectrum.len());

        for bin_idx in 0..low_spectrum.len() {
            let freq_hz = (bin_idx as f32 * sample_rate) / window_size as f32;

            let value = if freq_hz < self.low_mid_crossfade.0 {
                // Pure low frequency window
                low_spectrum[bin_idx]
            } else if freq_hz < self.low_mid_crossfade.1 {
                // Crossfade between low and mid
                let factor = (freq_hz - self.low_mid_crossfade.0)
                    / (self.low_mid_crossfade.1 - self.low_mid_crossfade.0);
                low_spectrum[bin_idx] * (1.0 - factor) + mid_spectrum[bin_idx] * factor
            } else if freq_hz < self.mid_high_crossfade.0 {
                // Pure mid frequency window
                mid_spectrum[bin_idx]
            } else if freq_hz < self.mid_high_crossfade.1 {
                // Crossfade between mid and high
                let factor = (freq_hz - self.mid_high_crossfade.0)
                    / (self.mid_high_crossfade.1 - self.mid_high_crossfade.0);
                mid_spectrum[bin_idx] * (1.0 - factor) + high_spectrum[bin_idx] * factor
            } else {
                // Pure high frequency window
                high_spectrum[bin_idx]
            };

            blended.push(value);
        }

        blended
    }
}
