/// Window functions for FFT spectral analysis
///
/// This module provides various window functions and adaptive windowing
/// strategies for optimizing spectrum analysis at different frequency ranges.
use core::f32::consts::PI;
use libm::cosf;

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
    #[allow(dead_code)]
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
