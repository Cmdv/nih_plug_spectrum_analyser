use core::f32::consts::PI;
use libm::cosf;
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

/// Generate Hann window coefficients for spectral analysis
/// The Hann window (also called Hanning) provides good frequency resolution
/// with -32dB sidelobe suppression.
/// Returns a vector of window coefficients that will be multiplied with audio samples.
#[must_use]
fn generate_hann_window(window_size: usize) -> Vec<f32> {
    // Pre-allocate vector for efficiency
    let mut window = Vec::with_capacity(window_size);

    // Convert window size to float once to avoid repeated casting
    let window_size_f32 = window_size as f32;

    // Generate each window coefficient
    for i in 0..window_size {
        // Calculate normalized position in window (0 to 1)
        // This represents how far through the window we are
        let position = i as f32 / window_size_f32;

        // Calculate the cosine term for this position
        // cos(2πi/N) creates a cosine wave over the window length
        let cos_term = cosf(2.0 * PI * position);

        // Apply Hann formula: 0.5 * (1 - cos(2πi/N))
        // This creates a raised cosine shape:
        // - Starts at 0 (when cos = 1, result = 0)
        // - Peaks at 1 in the middle (when cos = -1, result = 1)
        // - Returns to 0 at the end (when cos = 1 again)
        let coefficient = 0.5 * (1.0 - cos_term);

        window.push(coefficient);
    }

    window
}

/// Continuously computes frequency spectrum and sends to [`SpectrumOutput`] (audio thread writes to this)
pub struct SpectrumAnalyzer {
    /// FFT processing engine
    fft_processor: Arc<dyn RealToComplex<f32>>,

    /// Pre-computed Hann window for spectral leakage reduction
    window_function: Vec<f32>,

    /// Window coherent gain for amplitude compensation
    /// Hann window reduces amplitude by ~50%, this value compensates for it
    window_coherent_gain: f32,

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
        let window_function: Vec<f32> = generate_hann_window(SPECTRUM_WINDOW_SIZE);
        // Calculate actual coherent gain (sum of coefficients / size)
        let coherent_gain: f32 = window_function.iter().sum::<f32>() / SPECTRUM_WINDOW_SIZE as f32;

        nih_plug::nih_log!("Window coherent gain: {:.4}", coherent_gain);

        // TODO: Implement dynamic window size calculation based on sample rate
        // spectrum-analyzer uses: window_size = sample_rate / frequency_resolution
        // This gives better frequency resolution at different sample rates
        // Example: 48000 Hz / 23.4 Hz = 2048 samples (current fixed size)

        let analyzer = Self {
            fft_processor,
            window_function,
            window_coherent_gain: coherent_gain,
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

    /// Extract mono mix from stereo buffer and store in internal buffer
    fn extract_mono_samples(&mut self, buffer: &Buffer) {
        // Use the pure function and copy result into internal buffer
        let mono_samples = extract_mono_samples(buffer, SPECTRUM_WINDOW_SIZE);
        self.time_domain_buffer.copy_from_slice(&mono_samples);
    }

    /// Apply windowing and store result in internal buffer
    fn apply_window(&mut self) {
        let windowed = apply_window(&self.time_domain_buffer, &self.window_function);
        self.time_domain_buffer.copy_from_slice(&windowed);
    }

    /// Convert complex FFT output to magnitude spectrum and store in internal buffer
    fn compute_magnitude_spectrum(&mut self, _sample_rate: f32) {
        let magnitude_spectrum =
            compute_magnitude_spectrum(&self.frequency_domain_buffer, SPECTRUM_WINDOW_SIZE);
        self.spectrum_result.copy_from_slice(&magnitude_spectrum);
    }

    /// Apply perceptual smoothing and update internal state
    fn apply_spectrum_smoothing(&mut self) {
        let (smoothed_spectrum, updated_previous) =
            apply_spectrum_smoothing(&self.spectrum_result, &self.previous_spectrum);
        self.spectrum_result.copy_from_slice(&smoothed_spectrum);
        self.previous_spectrum.copy_from_slice(&updated_previous);
    }
}

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

/// Extract mono mix from stereo buffer for spectral analysis
///
/// Professional spectrum analyzers typically analyze the sum of all channels.
/// Returns a vector of mono samples, zero-padded to the specified window size.
/// Channels are summed and normalized to prevent clipping.
pub fn extract_mono_samples(buffer: &Buffer, window_size: usize) -> Vec<f32> {
    let num_channels = buffer.channels();
    let num_samples = buffer.samples().min(window_size);

    // Initialize with zeros (handles padding automatically)
    let mut mono_samples = vec![0.0; window_size];

    if num_channels == 0 {
        return mono_samples;
    }

    // Get immutable access to channel slices
    let channel_slices = buffer.as_slice_immutable();

    // Sum all channels into mono mix
    for channel_idx in 0..num_channels {
        let channel = &channel_slices[channel_idx];
        for (sample_idx, &sample) in channel.iter().enumerate().take(num_samples) {
            mono_samples[sample_idx] += sample;
        }
    }

    // Normalize by channel count to prevent clipping
    let normalization = 1.0 / num_channels as f32;
    for sample in mono_samples.iter_mut().take(num_samples) {
        *sample *= normalization;
    }

    mono_samples
}

/// Apply windowing function to time domain samples
///
/// Windowing reduces spectral leakage by tapering the signal edges to zero.
/// The Hann window provides good frequency resolution with -32dB sidelobe suppression.
/// Each sample is multiplied by the corresponding window coefficient.
pub fn apply_window(samples: &[f32], window_function: &[f32]) -> Vec<f32> {
    samples
        .iter()
        .zip(window_function.iter())
        .map(|(&sample, &window_coeff)| sample * window_coeff)
        .collect()
}

/// Convert complex FFT output to magnitude spectrum in dB
///
/// Calculates magnitude from complex FFT bins and converts to dB scale.
/// Applies proper FFT normalization and gain compensation to match professional
/// spectrum analyzer behavior. Uses a floor value to prevent log(0) errors.
pub fn compute_magnitude_spectrum(frequency_bins: &[Complex32], window_size: usize) -> Vec<f32> {
    frequency_bins
        .iter()
        .map(|&complex_bin| {
            // Calculate magnitude: sqrt(re² + im²)
            let magnitude =
                (complex_bin.re * complex_bin.re + complex_bin.im * complex_bin.im).sqrt();

            // Normalize by square root of FFT size for proper scaling (standard FFT normalization)
            let normalized_magnitude = magnitude / (window_size as f32).sqrt();

            // Convert to dB with proper floor to avoid log(0)
            // Add gain compensation to match professional analyzer levels
            // Pro-Q and similar analyzers apply significant gain compensation
            let db_value = if normalized_magnitude > 0.0 {
                20.0 * normalized_magnitude.log10() + 36.0 // +36dB total compensation to match Pro-Q levels
            } else {
                SPECTRUM_FLOOR_DB
            };

            db_value.max(SPECTRUM_FLOOR_DB)
        })
        .collect()
}

/// Apply perceptual smoothing with attack/release characteristics
///
/// Smoothing prevents spectrum flickering and makes the display easier to read.
/// Uses different time constants for attack (fast response to increases) and
/// release (slow decay), similar to analog spectrum analyzers and Pro-Q.
/// Returns (smoothed_spectrum, updated_previous_spectrum) tuple.
pub fn apply_spectrum_smoothing(
    current_spectrum: &[f32],
    previous_spectrum: &[f32],
) -> (Vec<f32>, Vec<f32>) {
    let smoothed: Vec<f32> = current_spectrum
        .iter()
        .zip(previous_spectrum.iter())
        .map(|(&current_db, &previous_db)| {
            if current_db > previous_db {
                // Attack: fast response to increases (like a peak meter)
                previous_db + (current_db - previous_db) * SPECTRUM_ATTACK
            } else {
                // Release: slow decay (prevents flickering, easier to read)
                previous_db + (current_db - previous_db) * SPECTRUM_RELEASE
            }
        })
        .collect();

    // Return both the smoothed result and the updated previous spectrum
    (smoothed.clone(), smoothed)
}
