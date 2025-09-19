use nih_plug::prelude::*;
use realfft::{num_complex::Complex32, RealFftPlanner, RealToComplex};
use std::num::NonZeroUsize;
use std::sync::Arc;
use triple_buffer::TripleBuffer;

use super::errors::{SpectrumError, SpectrumResult};
use super::window_functions::{AdaptiveWindowStrategy, AdaptiveWindows, WindowData};

/// The size of our FFT analysis window
/// 2048 gives us 23.4Hz resolution at 48kHz (good for 20Hz-20kHz range)
pub const SPECTRUM_WINDOW_SIZE: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(2048) };

/// Legacy usize version for compatibility with existing code
pub const SPECTRUM_WINDOW_SIZE_USIZE: usize = SPECTRUM_WINDOW_SIZE.get();

/// Number of frequency bins produced by the FFT (N/2 + 1 for real FFT)
pub const SPECTRUM_BINS: usize = SPECTRUM_WINDOW_SIZE_USIZE / 2 + 1;

/// Pink noise tilt compensation in dB per octave to make spectrum appear flatter
const SPECTRUM_TILT_DB_PER_OCT: f32 = 4.5;

/// Spectrum analyser floor prevents log(0) in FFT calculations
const SPECTRUM_FLOOR_DB: f32 = -120.0;

/// FFT overlap factor for smoother spectrum updates (50% overlap)
const FFT_OVERLAP_FACTOR: f32 = 0.5;

/// Ring buffer size multiplier (2x for 50% overlap)
const RING_BUFFER_SIZE_MULTIPLIER: usize = 2;

/// Minimum amplitude threshold to avoid log(0) errors
const MIN_AMPLITUDE_THRESHOLD: f32 = 1e-8;

/// dB conversion factor (20 * log10)
const DB_CONVERSION_FACTOR: f32 = 20.0;

/// Reference frequency for tilt compensation (1kHz standard)
const TILT_REFERENCE_FREQ_HZ: f32 = 1000.0;

/// Minimum frequency threshold to avoid log(0) in tilt calculation
const MIN_FREQ_THRESHOLD: f32 = 0.001;

/// Debug logging frequency range bounds (for 1kHz region)
// const DEBUG_FREQ_LOWER_HZ: f32 = 950.0;
// const DEBUG_FREQ_UPPER_HZ: f32 = 1050.0;

/// Frequency thresholds for smoothing regions
const SMOOTHING_HIGH_FREQ_HZ: f32 = 5000.0;
const SMOOTHING_MID_HIGH_FREQ_HZ: f32 = 2000.0;
const SMOOTHING_MID_FREQ_HZ: f32 = 800.0;

/// Smoothing kernel sizes for different frequency regions
const HIGH_FREQ_KERNEL_SIZE: usize = 9;
const MID_HIGH_FREQ_KERNEL_SIZE: usize = 7;

/// Gaussian sigma values for smoothing weights
const GAUSSIAN_SIGMA_STRONG: f32 = 1.0; // For high frequency aggressive smoothing
const GAUSSIAN_SIGMA_MODERATE: f32 = 2.0; // For mid-high frequency smoothing

/// Smoothing weights for 5-point mid frequency kernel
const SMOOTH_WEIGHT_OUTER: f32 = 0.1; // Weight for samples at ±2 positions
const SMOOTH_WEIGHT_INNER: f32 = 0.2; // Weight for samples at ±1 positions
const SMOOTH_WEIGHT_CENTER: f32 = 0.4; // Weight for center sample

/// The spectrum analyser's frequency data - array of magnitude values in dB
pub type SpectrumData = [f32; SPECTRUM_BINS];

/// Cloneable wrapper for spectrum output channel (UI thread reads from this)
/// Uses Arc<Mutex<>> wrapper to allow cloning for editor initialization
#[derive(Clone)]
pub struct SpectrumConsumer {
    output: Arc<std::sync::Mutex<triple_buffer::Output<SpectrumData>>>,
}

impl SpectrumConsumer {
    fn new(output: triple_buffer::Output<SpectrumData>) -> Self {
        Self {
            output: Arc::new(std::sync::Mutex::new(output)),
        }
    }

    /// Read latest spectrum data for UI display
    /// Called from UI thread only
    #[must_use]
    pub fn read(&self) -> SpectrumResult<SpectrumData> {
        self.output
            .try_lock()
            .map(|mut output| *output.read())
            .map_err(|_| SpectrumError::LockFailed {
                resource: "spectrum output".to_string(),
            })
    }

    /// Read latest spectrum data with fallback to silence
    /// Convenience method for when you want to always get data
    #[must_use]
    pub fn read_or_silence(&self) -> SpectrumData {
        self.read().unwrap_or([SPECTRUM_FLOOR_DB; SPECTRUM_BINS])
    }
}

/// Spectrum analyser speed presets for temporal smoothing
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(dead_code)]
pub enum SpectrumSpeed {
    VerySlow,
    Slow,
    Medium,
    Fast,
    VeryFast,
}

impl SpectrumSpeed {
    /// Get attack and release time constants in milliseconds
    /// Professional spectrum analyser speed settings
    fn time_constants_ms(&self) -> (f32, f32) {
        match self {
            Self::VerySlow => (100.0, 2000.0), // Very slow, smooth display
            Self::Slow => (50.0, 1000.0),      // Slow, good for overall monitoring
            Self::Medium => (20.0, 400.0),     // Medium, balanced response
            Self::Fast => (5.0, 100.0),        // Fast, good for transients
            Self::VeryFast => (1.0, 20.0),     // Very fast, immediate response
        }
    }
}

/// Continuously computes frequency spectrum and sends to [`SpectrumConsumer`] (audio thread writes to this)
pub struct SpectrumProducer {
    /// FFT processing engine
    fft_processor: Arc<dyn RealToComplex<f32>>,
    /// Adaptive window strategy for frequency-dependent windowing
    window_strategy: AdaptiveWindowStrategy,
    /// Pre-computed windows for different frequency ranges
    adaptive_windows: AdaptiveWindows,
    /// Ring buffer for accumulating samples across multiple process calls
    /// Size is 2x window size for 50% overlap
    ring_buffer: Vec<f32>,
    /// Write position in ring buffer
    ring_buffer_pos: usize,
    /// Sample counter for triggering FFT processing
    samples_since_fft: usize,
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
    /// Speed setting for temporal smoothing
    speed: SpectrumSpeed,
    /// Count of FFT failures (for debugging without impacting performance)
    fft_failure_count: std::sync::atomic::AtomicU32,
}

/// Builder for configuring SpectrumProducer initialization
pub struct SpectrumProducerBuilder {
    window_size: NonZeroUsize,
    speed: SpectrumSpeed,
    window_strategy: Option<AdaptiveWindowStrategy>,
}

impl Default for SpectrumProducerBuilder {
    fn default() -> Self {
        Self {
            window_size: SPECTRUM_WINDOW_SIZE,
            speed: SpectrumSpeed::Medium,
            window_strategy: None,
        }
    }
}

impl SpectrumProducerBuilder {
    /// Create a new builder with default settings
    pub fn new() -> Self {
        Self::default()
    }

    /// Set the FFT window size (must be power of 2)
    #[must_use = "Builder methods must be chained"]
    #[allow(dead_code)]
    pub fn window_size(mut self, size: NonZeroUsize) -> Self {
        debug_assert!(size.is_power_of_two(), "Window size must be power of 2");
        self.window_size = size;
        self
    }

    /// Set the spectrum speed (temporal smoothing)
    #[must_use = "Builder methods must be chained"]
    pub fn speed(mut self, speed: SpectrumSpeed) -> Self {
        self.speed = speed;
        self
    }

    /// Set a custom window strategy
    #[must_use = "Builder methods must be chained"]
    #[allow(dead_code)]
    pub fn window_strategy(mut self, strategy: AdaptiveWindowStrategy) -> Self {
        self.window_strategy = Some(strategy);
        self
    }

    /// Build the SpectrumProducer and consumer pair
    #[must_use = "SpectrumProducer and consumer must be used"]
    pub fn build(self) -> (SpectrumProducer, SpectrumConsumer) {
        // For now, we keep the window size fixed to SPECTRUM_WINDOW_SIZE
        // Future enhancement: support dynamic window sizes
        assert_eq!(
            self.window_size.get(),
            SPECTRUM_WINDOW_SIZE_USIZE,
            "Dynamic window sizes not yet supported"
        );

        // Create lock-free communication channel
        let (spectrum_producer, spectrum_consumer) =
            TripleBuffer::new(&[SPECTRUM_FLOOR_DB; SPECTRUM_BINS]).split();

        // Initialize FFT processor
        let mut fft_planner = RealFftPlanner::<f32>::new();
        let fft_processor = fft_planner.plan_fft_forward(SPECTRUM_WINDOW_SIZE_USIZE);

        // Use provided strategy or default
        let window_strategy = self.window_strategy.unwrap_or_default();

        // Pre-compute windows for different frequency ranges
        let low_coeffs = window_strategy
            .low_freq_window
            .generate(SPECTRUM_WINDOW_SIZE_USIZE);
        let mid_coeffs = window_strategy
            .mid_freq_window
            .generate(SPECTRUM_WINDOW_SIZE_USIZE);
        let high_coeffs = window_strategy
            .high_freq_window
            .generate(SPECTRUM_WINDOW_SIZE_USIZE);

        let adaptive_windows = AdaptiveWindows {
            low_freq: WindowData {
                coherent_gain: window_strategy.low_freq_window.coherent_gain(&low_coeffs),
                coefficients: low_coeffs,
            },
            mid_freq: WindowData {
                coherent_gain: window_strategy.mid_freq_window.coherent_gain(&mid_coeffs),
                coefficients: mid_coeffs,
            },
            high_freq: WindowData {
                coherent_gain: window_strategy.high_freq_window.coherent_gain(&high_coeffs),
                coefficients: high_coeffs,
            },
        };

        let analyser = SpectrumProducer {
            fft_processor,
            window_strategy,
            adaptive_windows,
            ring_buffer: vec![0.0; SPECTRUM_WINDOW_SIZE_USIZE * RING_BUFFER_SIZE_MULTIPLIER], // 2x size for overlap
            ring_buffer_pos: 0,
            samples_since_fft: 0,
            time_domain_buffer: vec![0.0; SPECTRUM_WINDOW_SIZE_USIZE],
            frequency_domain_buffer: vec![Complex32::new(0.0, 0.0); SPECTRUM_BINS],
            spectrum_result: [SPECTRUM_FLOOR_DB; SPECTRUM_BINS],
            previous_spectrum: [SPECTRUM_FLOOR_DB; SPECTRUM_BINS],
            spectrum_producer,
            speed: self.speed,
            fft_failure_count: std::sync::atomic::AtomicU32::new(0),
        };

        (analyser, SpectrumConsumer::new(spectrum_consumer))
    }
}

impl SpectrumProducer {
    /// Create a new builder for configuring SpectrumProducer
    #[must_use = "Builder must be configured and built"]
    pub fn builder() -> SpectrumProducerBuilder {
        SpectrumProducerBuilder::new()
    }

    /// Write silence to the spectrum buffer (used when plugin is deactivated)
    /// This ensures the UI gets actual silence instead of stale audio data
    pub fn write_silence(&mut self) {
        let silence = [SPECTRUM_FLOOR_DB; SPECTRUM_BINS];
        self.spectrum_producer.write(silence);
    }

    /// Get the count of FFT failures (for debugging)
    /// Can be safely called from UI thread
    #[allow(dead_code)]
    pub fn fft_failure_count(&self) -> u32 {
        self.fft_failure_count
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    /// Compute spectrum from audio buffer and send to UI thread
    /// Called from audio thread - must be real-time safe (no allocations)
    pub fn process(&mut self, buffer: &Buffer, sample_rate: f32) {
        // Add incoming samples to ring buffer
        self.add_samples_to_ring_buffer(buffer);

        // Check if we should process FFT (50% overlap = every WINDOW_SIZE/2 samples)
        if self.samples_since_fft
            >= (SPECTRUM_WINDOW_SIZE_USIZE as f32 * FFT_OVERLAP_FACTOR) as usize
        {
            self.samples_since_fft = 0;

            // Copy from ring buffer to FFT buffer
            self.copy_from_ring_buffer();

            // Apply windowing to reduce spectral leakage
            self.apply_window();

            // Perform FFT: time domain -> frequency domain
            if let Err(_) = self.fft_processor.process(
                &mut self.time_domain_buffer,
                &mut self.frequency_domain_buffer,
            ) {
                // FFT failed - skip this frame to maintain real-time safety
                // Just increment counter for debugging, no logging in audio thread
                self.fft_failure_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return;
            }

            // Convert complex FFT output to magnitude spectrum in dB
            self.compute_magnitude_spectrum(sample_rate);

            // Apply perceptual smoothing (attack/release envelope)
            self.apply_spectrum_smoothing(sample_rate);
            // Send result to UI thread (lock-free)
            self.spectrum_producer.write(self.spectrum_result);
        }
    }

    /// Add samples from audio buffer to ring buffer
    fn add_samples_to_ring_buffer(&mut self, buffer: &Buffer) {
        let num_channels = buffer.channels();
        let num_samples = buffer.samples();

        if num_channels == 0 || num_samples == 0 {
            return;
        }

        let channel_slices = buffer.as_slice_immutable();

        (0..num_samples).for_each(|sample_idx| {
            // Sum all channels for mono mix using iterator
            let mono_sample = channel_slices
                .iter()
                .map(|channel| channel[sample_idx])
                .sum::<f32>()
                / num_channels as f32;

            // Add to ring buffer
            self.ring_buffer[self.ring_buffer_pos] = mono_sample;

            // Advance ring buffer position (wrap around)
            self.ring_buffer_pos = (self.ring_buffer_pos + 1) % self.ring_buffer.len();
            self.samples_since_fft += 1;
        });
    }

    /// Copy most recent SPECTRUM_WINDOW_SIZE samples from ring buffer to FFT buffer
    fn copy_from_ring_buffer(&mut self) {
        let ring_len = self.ring_buffer.len();

        // Start position: current pos minus window size
        let start_pos = if self.ring_buffer_pos >= SPECTRUM_WINDOW_SIZE_USIZE {
            self.ring_buffer_pos - SPECTRUM_WINDOW_SIZE_USIZE
        } else {
            ring_len - (SPECTRUM_WINDOW_SIZE_USIZE - self.ring_buffer_pos)
        };

        // Copy samples (handle wrap-around) using iterators
        self.time_domain_buffer
            .iter_mut()
            .enumerate()
            .for_each(|(i, sample)| {
                let ring_idx = (start_pos + i) % ring_len;
                *sample = self.ring_buffer[ring_idx];
            });
    }

    /// Apply windowing in-place to time domain buffer
    fn apply_window(&mut self) {
        // Use mid-frequency window as default for initial windowing
        let coeffs = &self.adaptive_windows.mid_freq.coefficients;
        for (sample, &coeff) in self.time_domain_buffer.iter_mut().zip(coeffs.iter()) {
            *sample *= coeff;
        }
    }

    /// Convert complex FFT output to magnitude spectrum and store in internal buffer
    fn compute_magnitude_spectrum(&mut self, sample_rate: f32) {
        // Process with adaptive windowing
        let magnitude_spectrum = self.compute_adaptive_magnitude_spectrum(sample_rate);
        self.spectrum_result.copy_from_slice(&magnitude_spectrum);
    }

    /// Compute spectrum with a specific window and return the result
    fn compute_spectrum_with_window(
        &mut self,
        audio_samples: &[f32],
        window_coeffs: &[f32],
        coherent_gain: f32,
        sample_rate: f32,
    ) -> Vec<f32> {
        // Apply window
        let windowed = apply_window(audio_samples, window_coeffs);
        self.time_domain_buffer.copy_from_slice(&windowed);

        // Perform FFT
        let mut freq_buffer = vec![Complex32::new(0.0, 0.0); SPECTRUM_BINS];
        let _ = self
            .fft_processor
            .process(&mut self.time_domain_buffer, &mut freq_buffer);

        // Convert to magnitude spectrum
        compute_magnitude_spectrum(
            &freq_buffer,
            SPECTRUM_WINDOW_SIZE_USIZE,
            coherent_gain,
            sample_rate,
        )
    }

    /// Compute magnitude spectrum with adaptive windowing for different frequency ranges
    fn compute_adaptive_magnitude_spectrum(&mut self, sample_rate: f32) -> Vec<f32> {
        // Save original audio data
        let original_buffer = self.time_domain_buffer.clone();

        // Extract window data to avoid borrowing issues
        let low_coeffs = self.adaptive_windows.low_freq.coefficients.clone();
        let low_gain = self.adaptive_windows.low_freq.coherent_gain;
        let mid_coeffs = self.adaptive_windows.mid_freq.coefficients.clone();
        let mid_gain = self.adaptive_windows.mid_freq.coherent_gain;
        let high_coeffs = self.adaptive_windows.high_freq.coefficients.clone();
        let high_gain = self.adaptive_windows.high_freq.coherent_gain;

        // Compute spectrum with each window type
        let low_spectrum =
            self.compute_spectrum_with_window(&original_buffer, &low_coeffs, low_gain, sample_rate);

        let mid_spectrum =
            self.compute_spectrum_with_window(&original_buffer, &mid_coeffs, mid_gain, sample_rate);

        let high_spectrum = self.compute_spectrum_with_window(
            &original_buffer,
            &high_coeffs,
            high_gain,
            sample_rate,
        );

        // Restore original buffer state
        self.time_domain_buffer.copy_from_slice(&original_buffer);

        // Blend the three spectrums based on frequency
        self.window_strategy.blend_frequency_spectrums(
            &low_spectrum,
            &mid_spectrum,
            &high_spectrum,
            sample_rate,
            SPECTRUM_WINDOW_SIZE_USIZE,
        )
    }

    /// Apply perceptual smoothing and update internal state
    fn apply_spectrum_smoothing(&mut self, sample_rate: f32) {
        let (smoothed_spectrum, updated_previous) = apply_spectrum_smoothing(
            &self.spectrum_result,
            &self.previous_spectrum,
            self.speed,
            sample_rate,
        );
        self.spectrum_result.copy_from_slice(&smoothed_spectrum);
        self.previous_spectrum.copy_from_slice(&updated_previous);
    }
}

/// Multiplies audio samples by window function coefficients
///
/// Element-wise multiplication of samples and window coefficients. This is the
/// core windowing operation that shapes the time-domain signal before FFT,
/// reducing spectral leakage by smoothly tapering edges to zero.
///
/// # Parameters
/// * `samples` - Time-domain audio samples to be windowed
/// * `window_function` - Pre-computed window coefficients [0.0..1.0]
///
/// # Returns
/// Vector of windowed samples, same length as input
///
/// # Mathematical Background
/// Windowing: y[n] = x[n] * w[n] for n = 0..N-1
/// - Time domain: multiplication
/// - Frequency domain: convolution with window's spectrum
/// - Reduces discontinuity energy at frame boundaries
///
/// # Why Window?
/// - Unwindowed FFT assumes signal is periodic (wraps around)
/// - Real signals aren't periodic in arbitrary frames
/// - Discontinuity at edges spreads energy across all frequencies
/// - Window makes signal smoothly go to zero at edges
///
/// # References
/// - "The Fundamentals of FFT-Based Signal Analysis" - National Instruments
/// - https://www.ni.com/docs/en-US/bundle/labview/page/lvanlsconcepts/windowing.html
pub fn apply_window(samples: &[f32], window_function: &[f32]) -> Vec<f32> {
    samples
        .iter()
        .zip(window_function.iter())
        .map(|(&sample, &window_coeff)| sample * window_coeff)
        .collect()
}

/// Converts complex FFT output to magnitude spectrum in dB with tilt compensation
///
/// Transforms raw FFT complex numbers into a magnitude spectrum suitable for display.
/// Applies proper scaling for single-sided spectrum, compensates for window energy loss,
/// converts to dB scale, and applies frequency tilt for perceptually flat response.
///
/// # Parameters
/// * `frequency_bins` - Complex FFT output bins (N/2+1 for real FFT)
/// * `window_size` - Size of FFT window (for normalization)
/// * `window_coherent_gain` - Window's coherent gain for amplitude correction
/// * `sample_rate` - Sample rate in Hz (for frequency calculation)
///
/// # Returns
/// Vector of magnitude values in dB, with tilt compensation applied
///
/// # Mathematical Background
/// 1. Magnitude: |X[k]| = sqrt(real² + imag²)
/// 2. Single-sided scaling: 2/N for k>0, 1/N for DC (k=0)
/// 3. Window compensation: divide by coherent gain
/// 4. dB conversion: 20*log10(amplitude)
/// 5. Tilt: +4.5dB/octave from 1kHz reference
///
/// # Scaling Explanation
/// - FFT produces two-sided spectrum, we show single-sided
/// - Factor of 2 accounts for negative frequency energy
/// - DC bin (0 Hz) has no negative counterpart, no factor of 2
/// - Window reduces amplitude by coherent gain factor
///
/// # Implementation Notes
/// - Floor at -120dB prevents log(0) errors
/// - Tilt compensation reveals high-frequency detail
/// - Reference: AES17 standard for digital audio measurement
///
/// # References
/// - "Spectral Audio Signal Processing" by Julius O. Smith III
/// - AES17-2015 "AES standard method for digital audio engineering"
/// - https://ccrma.stanford.edu/~jos/sasp/Spectrum_Analysis_Windows.html
pub fn compute_magnitude_spectrum(
    frequency_bins: &[Complex32],
    window_size: usize,
    window_coherent_gain: f32,
    sample_rate: f32,
) -> Vec<f32> {
    let spectrum_with_tilt: Vec<f32> = frequency_bins
        .iter()
        .enumerate()
        .map(|(bin_idx, &complex_bin)| {
            // Calculate magnitude
            let magnitude = complex_bin.norm();
            // According to spectrum.md: Use proper 2/N scaling for single-sided spectrum
            let scaling = if bin_idx == 0 {
                // DC component: no factor of 2
                1.0 / window_size as f32
            } else {
                // All other bins: factor of 2 for single-sided spectrum
                2.0 / window_size as f32
            };
            // Apply window compensation (spectrum.md: divide by coherent gain)
            let amplitude = magnitude * scaling / window_coherent_gain;

            // Convert to dBFS according to AES17 standard (spectrum.md)
            let db_value = if amplitude > MIN_AMPLITUDE_THRESHOLD {
                DB_CONVERSION_FACTOR * amplitude.log10()
            } else {
                SPECTRUM_FLOOR_DB
            };
            // Calculate frequency for this bin
            let freq_hz = (bin_idx as f32 * sample_rate) / window_size as f32;

            // Debug logging for 1kHz region
            // if freq_hz >= DEBUG_FREQ_LOWER_HZ && freq_hz <= DEBUG_FREQ_UPPER_HZ && amplitude > MIN_AMPLITUDE_THRESHOLD {
            //     nih_plug::nih_log!("FFT bin {}: freq={:.1}Hz, magnitude={:.6}, scaling={:.6}, coherent_gain={:.6}, amplitude={:.6}, db={:.1}dB",
            //         bin_idx, freq_hz, magnitude, scaling, window_coherent_gain, amplitude, db_value);
            // }

            // Apply tilt compensation
            let tilted_db = apply_tilt_compensation(db_value, freq_hz, SPECTRUM_TILT_DB_PER_OCT);

            // Apply floor clamping
            tilted_db.max(SPECTRUM_FLOOR_DB)
        })
        .collect();

    spectrum_with_tilt
}

/// Applies frequency-dependent tilt compensation to flatten spectrum display
///
/// Pink noise naturally has -3dB/octave slope. Musical content often has
/// similar characteristics. By applying positive tilt (boost increasing with
/// frequency), we make the spectrum appear flatter and reveal high-frequency
/// detail that would otherwise be hidden.
///
/// # Parameters
/// * `magnitude_db` - Original magnitude in dB
/// * `freq_hz` - Frequency of this bin in Hz
/// * `tilt_db_per_oct` - Tilt amount in dB per octave (typically 3-6)
///
/// # Returns
/// Magnitude with tilt compensation applied
///
/// # Mathematical Background
/// Octaves from reference: log2(freq/ref_freq)
/// Tilt boost: tilt_per_octave * octaves_from_reference
fn apply_tilt_compensation(magnitude_db: f32, freq_hz: f32, tilt_db_per_oct: f32) -> f32 {
    // Avoid log(0) for DC bin
    if freq_hz < MIN_FREQ_THRESHOLD {
        return magnitude_db;
    }

    // Calculate octaves from reference frequency
    // log2(2000/1000) = 1 octave up
    // log2(500/1000) = -1 octave down
    let octaves_from_reference = libm::log2f(freq_hz / TILT_REFERENCE_FREQ_HZ);

    // Apply tilt: positive above 1kHz, negative below
    magnitude_db + (tilt_db_per_oct * octaves_from_reference)
}

/// Applies temporal smoothing using asymmetric attack/release envelope
///
/// Smooths spectrum display over time to reduce visual noise while preserving
/// transient response. Uses faster attack (shows increases quickly) and slower
/// release (decays gradually) mimicking analog spectrum analysers and VU meters.
///
/// # Parameters
/// * `current_spectrum` - New spectrum values from current FFT frame
/// * `previous_spectrum` - Smoothed spectrum from previous frame
///
/// # Returns
/// Tuple of (smoothed_spectrum, updated_previous) for next iteration
///
/// # Mathematical Background
/// Exponential smoothing: y[n] = y[n-1] + α*(x[n] - y[n-1])
/// - Attack (rising): α = SPECTRUM_ATTACK (typically 0.3)
/// - Release (falling): α = SPECTRUM_RELEASE (typically 0.05)
/// - α = 1: no smoothing (instant response)
/// - α = 0: infinite smoothing (no change)
///
/// # Time Constants
/// - Attack: ~3-10 frames to reach 95% of new value
/// - Release: ~60-100 frames to decay to 5% of original
/// - Asymmetry makes peaks visible while reducing flicker
///
/// # Perceptual Rationale
/// - Fast attack ensures transients aren't missed
/// - Slow release makes spectrum easier to read
/// - Similar to peak program meters (PPM) in broadcasting
/// - Reduces eye fatigue from rapidly changing display
///
/// # References
/// - "Digital Audio Metering" by Eddy Brixen
/// - IEC 60268-10 "Peak programme level meters"
pub fn apply_spectrum_smoothing(
    current_spectrum: &[f32],
    previous_spectrum: &[f32],
    speed: SpectrumSpeed,
    sample_rate: f32,
) -> (Vec<f32>, Vec<f32>) {
    // Calculate FFT frame rate (with 50% overlap)
    // FFT happens every WINDOW_SIZE * FFT_OVERLAP_FACTOR samples
    let fft_frame_rate = sample_rate / (SPECTRUM_WINDOW_SIZE_USIZE as f32 * FFT_OVERLAP_FACTOR);

    // Get time constants for selected speed
    let (attack_ms, release_ms) = speed.time_constants_ms();

    // Convert to alpha coefficients
    let attack_alpha = calculate_smoothing_alpha(attack_ms, fft_frame_rate);
    let release_alpha = calculate_smoothing_alpha(release_ms, fft_frame_rate);

    let temporally_smoothed: Vec<f32> = current_spectrum
        .iter()
        .zip(previous_spectrum.iter())
        .map(|(&current_db, &previous_db)| {
            if current_db > previous_db {
                // Attack: fast response to increases
                previous_db + (current_db - previous_db) * attack_alpha
            } else {
                // Release: slow decay
                previous_db + (current_db - previous_db) * release_alpha
            }
        })
        .collect();

    // Apply frequency-dependent smoothing to reduce high-frequency noise
    let smoothed = apply_frequency_dependent_smoothing(&temporally_smoothed, sample_rate);

    let result = smoothed.clone();
    (result.clone(), result)
}

/// Apply frequency-dependent smoothing to reduce high-frequency noise
///
/// Progressive smoothing approach: leave low frequencies sharp for detail,
/// apply increasing smoothing for mid and high frequencies for cleaner appearance.
/// Based on professional spectrum analyser smoothing strategies.
pub fn apply_frequency_dependent_smoothing(spectrum: &[f32], sample_rate: f32) -> Vec<f32> {
    let mut smoothed = spectrum.to_vec();
    let window_size = SPECTRUM_WINDOW_SIZE_USIZE;

    // Apply frequency-dependent smoothing kernel
    for i in 1..spectrum.len() - 1 {
        let freq = (i as f32 * sample_rate) / window_size as f32;

        if freq > SMOOTHING_HIGH_FREQ_HZ {
            // High frequencies (>5kHz): Very aggressive 9-point smoothing
            let kernel_size = HIGH_FREQ_KERNEL_SIZE.min(spectrum.len() - 1);
            let half_kernel = kernel_size / 2;
            let start_idx = i.saturating_sub(half_kernel);
            let end_idx = (i + half_kernel).min(spectrum.len() - 1);

            let mut sum = 0.0;
            let mut weight_sum = 0.0;

            // Very strong Gaussian weighting for aggressive smoothing
            for j in start_idx..=end_idx {
                let distance = (j as i32 - i as i32).abs() as f32;
                let weight = (-distance * distance / GAUSSIAN_SIGMA_STRONG).exp(); // Stronger Gaussian (smaller sigma)
                sum += spectrum[j] * weight;
                weight_sum += weight;
            }

            if weight_sum > 0.0 {
                smoothed[i] = sum / weight_sum;
            }
        } else if freq > SMOOTHING_MID_HIGH_FREQ_HZ {
            // Mid-high frequencies (2-5kHz): 7-point smoothing
            let kernel_size = MID_HIGH_FREQ_KERNEL_SIZE.min(spectrum.len() - 1);
            let half_kernel = kernel_size / 2;
            let start_idx = i.saturating_sub(half_kernel);
            let end_idx = (i + half_kernel).min(spectrum.len() - 1);

            let mut sum = 0.0;
            let mut weight_sum = 0.0;

            for j in start_idx..=end_idx {
                let distance = (j as i32 - i as i32).abs() as f32;
                let weight = (-distance * distance / GAUSSIAN_SIGMA_MODERATE).exp();
                sum += spectrum[j] * weight;
                weight_sum += weight;
            }

            if weight_sum > 0.0 {
                smoothed[i] = sum / weight_sum;
            }
        } else if freq > SMOOTHING_MID_FREQ_HZ {
            // Mid frequencies (800Hz-2kHz): 5-point smoothing
            let prev2 = spectrum.get(i.saturating_sub(2)).unwrap_or(&spectrum[i]);
            let prev = spectrum[i - 1];
            let curr = spectrum[i];
            let next = spectrum[i + 1];
            let next2 = spectrum.get(i + 2).unwrap_or(&spectrum[i]);

            smoothed[i] = prev2 * SMOOTH_WEIGHT_OUTER
                + prev * SMOOTH_WEIGHT_INNER
                + curr * SMOOTH_WEIGHT_CENTER
                + next * SMOOTH_WEIGHT_INNER
                + next2 * SMOOTH_WEIGHT_OUTER;
        }
        // Leave frequencies < 1kHz unchanged for maximum detail
    }

    smoothed
}

/// Converts time constant in milliseconds to exponential filter coefficient
///
/// For exponential smoothing: y[n] = α*x[n] + (1-α)*y[n-1]
/// Where α = 1 - exp(-Δt/τ)
/// - Δt = time between updates (FFT frame period)
/// - τ = time constant (how long to reach 63.2% of target)
///
/// # Parameters
/// * `time_ms` - Time constant in milliseconds
/// * `update_rate_hz` - How often the smoothing is applied (FFT frame rate)
///
/// # Returns
/// Alpha coefficient [0..1] where 1 = instant, 0 = no change
fn calculate_smoothing_alpha(time_ms: f32, update_rate_hz: f32) -> f32 {
    if time_ms <= 0.0 {
        return 1.0; // Instant response
    }

    let tau = time_ms / 1000.0; // Convert to seconds
    let update_period = 1.0 / update_rate_hz;

    // Exponential filter formula
    1.0 - (-update_period / tau).exp()
}
