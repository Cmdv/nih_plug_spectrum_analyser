use nih_plug::prelude::*;
use realfft::{num_complex::Complex32, RealFftPlanner, RealToComplex};
use std::sync::Arc;
use triple_buffer::TripleBuffer;

use super::window_functions::{AdaptiveWindowStrategy, AdaptiveWindows, WindowData};

/// The size of our FFT analysis window
/// 2048 gives us 23.4Hz resolution at 48kHz (good for 20Hz-20kHz range)
pub const SPECTRUM_WINDOW_SIZE: usize = 2048;

/// Number of frequency bins produced by the FFT (N/2 + 1 for real FFT)
pub const SPECTRUM_BINS: usize = SPECTRUM_WINDOW_SIZE / 2 + 1;

/// Pink noise tilt compensation in dB per octave to make spectrum appear flatter
const SPECTRUM_TILT_DB_PER_OCT: f32 = 4.5;

/// Spectrum analyser floor prevents log(0) in FFT calculations
const SPECTRUM_FLOOR_DB: f32 = -120.0;

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
    pub fn read(&self) -> SpectrumData {
        if let Ok(mut output) = self.output.try_lock() {
            *output.read()
        } else {
            // Return silence if unable to lock (shouldn't happen in normal operation)
            [SPECTRUM_FLOOR_DB; SPECTRUM_BINS]
        }
    }
}

/// Spectrum analyser speed presets for temporal smoothing
#[derive(Debug, Clone, Copy, PartialEq)]
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
}

impl SpectrumProducer {
    /// Create a new spectrum analyser and output pair
    /// Returns (analyser for audio thread, output for UI thread)
    pub fn new() -> (Self, SpectrumConsumer) {
        // Create lock-free communication channel
        let (spectrum_producer, spectrum_consumer) =
            TripleBuffer::new(&[SPECTRUM_FLOOR_DB; SPECTRUM_BINS]).split();

        // Initialize FFT processor
        let mut fft_planner = RealFftPlanner::<f32>::new();
        let fft_processor = fft_planner.plan_fft_forward(SPECTRUM_WINDOW_SIZE);

        // Initialize adaptive window strategy
        let window_strategy = AdaptiveWindowStrategy::default();

        // Pre-compute windows for different frequency ranges
        let low_coeffs = window_strategy
            .low_freq_window
            .generate(SPECTRUM_WINDOW_SIZE);
        let mid_coeffs = window_strategy
            .mid_freq_window
            .generate(SPECTRUM_WINDOW_SIZE);
        let high_coeffs = window_strategy
            .high_freq_window
            .generate(SPECTRUM_WINDOW_SIZE);

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

        // TODO: Implement dynamic window size calculation based on sample rate
        // spectrum-analyser uses: window_size = sample_rate / frequency_resolution
        // This gives better frequency resolution at different sample rates
        // Example: 48000 Hz / 23.4 Hz = 2048 samples (current fixed size)

        let analyser = Self {
            fft_processor,
            window_strategy,
            adaptive_windows,
            ring_buffer: vec![0.0; SPECTRUM_WINDOW_SIZE * 2], // 2x size for overlap
            ring_buffer_pos: 0,
            samples_since_fft: 0,
            time_domain_buffer: vec![0.0; SPECTRUM_WINDOW_SIZE],
            frequency_domain_buffer: vec![Complex32::new(0.0, 0.0); SPECTRUM_BINS],
            spectrum_result: [SPECTRUM_FLOOR_DB; SPECTRUM_BINS],
            previous_spectrum: [SPECTRUM_FLOOR_DB; SPECTRUM_BINS],
            spectrum_producer,
            speed: SpectrumSpeed::Medium, // Default to balanced Medium speed
        };

        (analyser, SpectrumConsumer::new(spectrum_consumer))
    }

    /// Compute spectrum from audio buffer and send to UI thread
    /// Called from audio thread - must be real-time safe (no allocations)
    pub fn process(&mut self, buffer: &Buffer, sample_rate: f32) {
        // Add incoming samples to ring buffer
        self.add_samples_to_ring_buffer(buffer);

        // Check if we should process FFT (50% overlap = every WINDOW_SIZE/2 samples)
        if self.samples_since_fft >= SPECTRUM_WINDOW_SIZE / 2 {
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

        for sample_idx in 0..num_samples {
            // Sum all channels for mono mix
            let mut sample_sum = 0.0f32;
            for channel_idx in 0..num_channels {
                sample_sum += channel_slices[channel_idx][sample_idx];
            }

            // Normalize by channel count and add to ring buffer
            let mono_sample = sample_sum / num_channels as f32;
            self.ring_buffer[self.ring_buffer_pos] = mono_sample;

            // Advance ring buffer position (wrap around)
            self.ring_buffer_pos = (self.ring_buffer_pos + 1) % self.ring_buffer.len();

            self.samples_since_fft += 1;
        }
    }

    /// Copy most recent SPECTRUM_WINDOW_SIZE samples from ring buffer to FFT buffer
    fn copy_from_ring_buffer(&mut self) {
        let ring_len = self.ring_buffer.len();

        // Start position: current pos minus window size
        let start_pos = if self.ring_buffer_pos >= SPECTRUM_WINDOW_SIZE {
            self.ring_buffer_pos - SPECTRUM_WINDOW_SIZE
        } else {
            ring_len - (SPECTRUM_WINDOW_SIZE - self.ring_buffer_pos)
        };

        // Copy samples (handle wrap-around)
        for i in 0..SPECTRUM_WINDOW_SIZE {
            let ring_idx = (start_pos + i) % ring_len;
            self.time_domain_buffer[i] = self.ring_buffer[ring_idx];
        }
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
            SPECTRUM_WINDOW_SIZE,
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
            SPECTRUM_WINDOW_SIZE,
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
            let db_value = if amplitude > 1e-8 {
                20.0 * amplitude.log10()
            } else {
                SPECTRUM_FLOOR_DB
            };
            // Calculate frequency for this bin
            let freq_hz = (bin_idx as f32 * sample_rate) / window_size as f32;

            // Debug logging for 1kHz region
            if freq_hz >= 950.0 && freq_hz <= 1050.0 && amplitude > 1e-8 {
                nih_plug::nih_log!("FFT bin {}: freq={:.1}Hz, magnitude={:.6}, scaling={:.6}, coherent_gain={:.6}, amplitude={:.6}, db={:.1}dB",
                    bin_idx, freq_hz, magnitude, scaling, window_coherent_gain, amplitude, db_value);
            }

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
    // Use 1kHz as reference frequency (standard in audio)
    const REFERENCE_FREQ: f32 = 1000.0;

    // Avoid log(0) for DC bin
    if freq_hz < 0.001 {
        return magnitude_db;
    }

    // Calculate octaves from reference frequency
    // log2(2000/1000) = 1 octave up
    // log2(500/1000) = -1 octave down
    let octaves_from_reference = libm::log2f(freq_hz / REFERENCE_FREQ);

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
    // FFT happens every WINDOW_SIZE/2 samples
    let fft_frame_rate = sample_rate / (SPECTRUM_WINDOW_SIZE as f32 / 2.0);

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
    let window_size = SPECTRUM_WINDOW_SIZE;

    // Apply frequency-dependent smoothing kernel
    for i in 1..spectrum.len() - 1 {
        let freq = (i as f32 * sample_rate) / window_size as f32;

        if freq > 5000.0 {
            // High frequencies (>5kHz): Very aggressive 9-point smoothing
            let kernel_size = (9_usize).min(spectrum.len() - 1);
            let half_kernel = kernel_size / 2;
            let start_idx = i.saturating_sub(half_kernel);
            let end_idx = (i + half_kernel).min(spectrum.len() - 1);

            let mut sum = 0.0;
            let mut weight_sum = 0.0;

            // Very strong Gaussian weighting for aggressive smoothing
            for j in start_idx..=end_idx {
                let distance = (j as i32 - i as i32).abs() as f32;
                let weight = (-distance * distance / 1.0).exp(); // Stronger Gaussian (smaller sigma)
                sum += spectrum[j] * weight;
                weight_sum += weight;
            }

            if weight_sum > 0.0 {
                smoothed[i] = sum / weight_sum;
            }
        } else if freq > 2000.0 {
            // Mid-high frequencies (2-5kHz): 7-point smoothing
            let kernel_size = (7_usize).min(spectrum.len() - 1);
            let half_kernel = kernel_size / 2;
            let start_idx = i.saturating_sub(half_kernel);
            let end_idx = (i + half_kernel).min(spectrum.len() - 1);

            let mut sum = 0.0;
            let mut weight_sum = 0.0;

            for j in start_idx..=end_idx {
                let distance = (j as i32 - i as i32).abs() as f32;
                let weight = (-distance * distance / 2.0).exp();
                sum += spectrum[j] * weight;
                weight_sum += weight;
            }

            if weight_sum > 0.0 {
                smoothed[i] = sum / weight_sum;
            }
        } else if freq > 800.0 {
            // Mid frequencies (800Hz-2kHz): 5-point smoothing
            let prev2 = spectrum.get(i.saturating_sub(2)).unwrap_or(&spectrum[i]);
            let prev = spectrum[i - 1];
            let curr = spectrum[i];
            let next = spectrum[i + 1];
            let next2 = spectrum.get(i + 2).unwrap_or(&spectrum[i]);

            smoothed[i] = prev2 * 0.1 + prev * 0.2 + curr * 0.4 + next * 0.2 + next2 * 0.1;
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
