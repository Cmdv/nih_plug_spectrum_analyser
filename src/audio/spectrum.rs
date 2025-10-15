use nih_plug::prelude::*;
use realfft::{num_complex::Complex32, RealFftPlanner, RealToComplex};
use std::num::NonZeroUsize;
use std::sync::*;
use triple_buffer::TripleBuffer;

use super::errors::{SpectrumError, SpectrumResult};
use super::window_functions::WindowType;
use crate::{ResolutionLevel, TiltLevel};

/// Maximum FFT size we support (for buffer allocation)
pub const MAX_FFT_SIZE: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(4096) };

/// Maximum FFT size as usize for convenience
pub const MAX_FFT_SIZE_USIZE: usize = MAX_FFT_SIZE.get();

/// Maximum number of frequency bins (for maximum FFT size)
pub const MAX_SPECTRUM_BINS: usize = MAX_FFT_SIZE_USIZE / 2 + 1;

/// Spectrum analyser floor prevents log(0) in FFT calculations
const SPECTRUM_FLOOR_DB: f32 = -140.0;

/// FFT overlap factor (50% overlap between consecutive FFT windows)
const FFT_OVERLAP_FACTOR: f32 = 0.5;

/// Ring buffer size multiplier to accommodate overlap
const RING_BUFFER_SIZE_MULTIPLIER: usize = 2;

/// Minimum amplitude threshold to avoid log(0) errors
const MIN_AMPLITUDE_THRESHOLD: f32 = 1e-30;

/// Reference frequency for tilt compensation (1kHz standard)
const TILT_REFERENCE_FREQ_HZ: f32 = 1000.0;

/// Minimum frequency threshold to avoid log(0) in tilt calculation
const MIN_FREQ_THRESHOLD: f32 = 0.001;

/// The spectrum analyser's frequency data - vector of magnitude values in dB
/// Variable size based on resolution setting
pub type SpectrumData = Vec<f32>;

/// Cloneable wrapper for spectrum output channel (UI thread reads from this)
/// Uses Arc<Mutex<>> wrapper to allow cloning for editor initialization
#[derive(Clone)]
pub struct SpectrumConsumer {
    output: Arc<Mutex<triple_buffer::Output<SpectrumData>>>,
}

impl SpectrumConsumer {
    fn new(output: triple_buffer::Output<SpectrumData>) -> Self {
        Self {
            output: Arc::new(Mutex::new(output)),
        }
    }

    /// Read latest spectrum data for UI display
    /// Called from UI thread only
    #[must_use]
    pub fn read(&self) -> SpectrumResult<SpectrumData> {
        self.output
            .try_lock()
            .map(|mut output| output.read().clone())
            .map_err(|_| SpectrumError::LockFailed {
                resource: "spectrum output".to_string(),
            })
    }
}

/// Spectrum analyser speed presets for temporal envelope (attack/release)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, nih_plug::prelude::Enum)]
#[allow(dead_code)]
pub enum SpectrumSpeed {
    #[id = "very_slow"]
    #[name = "Very Slow"]
    VerySlow,
    #[id = "slow"]
    #[name = "Slow"]
    Slow,
    #[id = "medium"]
    #[name = "Medium"]
    Medium,
    #[id = "fast"]
    #[name = "Fast"]
    Fast,
    #[id = "very_fast"]
    #[name = "Very Fast"]
    VeryFast,
}

impl SpectrumSpeed {
    /// Get response time constant in milliseconds for temporal envelope
    fn response_time_ms(&self) -> f32 {
        match self {
            Self::VerySlow => 5000.0,
            Self::Slow => 1500.0,
            Self::Medium => 500.0,
            Self::Fast => 250.0,
            Self::VeryFast => 100.0,
        }
    }

    /// Convert to animation speed multiplier for fluid shader
    /// Medium = 1.0x (baseline), faster speeds = higher multipliers
    pub fn to_animation_speed_multiplier(&self) -> f32 {
        match self {
            Self::VerySlow => 0.25,
            Self::Slow => 0.5,
            Self::Medium => 1.0,
            Self::Fast => 2.0,
            Self::VeryFast => 4.0,
        }
    }
}

/// Continuously computes frequency spectrum and sends to [`SpectrumConsumer`] (audio thread writes to this)
pub struct SpectrumProducer {
    /// FFT processing engine for frequency domain transformation
    fft_processor: Arc<dyn RealToComplex<f32>>,
    /// Pre-computed Hann window for spectrum analysis
    window_coefficients: Vec<f32>,
    /// Ring buffer for accumulating samples across multiple process calls
    ring_buffer: Vec<f32>,
    /// Write position in ring buffer
    ring_buffer_pos: usize,
    /// Sample counter for triggering FFT processing
    samples_since_fft: usize,
    /// Input buffer for windowed samples (time domain)
    time_domain_buffer: Vec<f32>,
    /// Output buffer for FFT results (frequency domain)
    frequency_domain_buffer: Vec<Complex32>,
    /// Current spectrum result - size determined by resolution parameter
    spectrum_result: SpectrumData,
    /// Previous spectrum for temporal envelope calculations - size matches current
    previous_spectrum: SpectrumData,
    /// Current resolution level that determines buffer sizes
    current_resolution: ResolutionLevel,
    /// Triple buffer producer for lock-free communication to UI
    spectrum_producer: triple_buffer::Input<SpectrumData>,
    /// Count of FFT failures (for debugging without impacting performance)
    fft_failure_count: std::sync::atomic::AtomicU32,
}

impl SpectrumProducer {
    /// Create a new SpectrumProducer and consumer pair
    #[must_use = "SpectrumProducer and consumer must be used"]
    pub fn new() -> (SpectrumProducer, SpectrumConsumer) {
        // Create lock-free communication channel initialized with maximum possible size
        let (spectrum_producer, spectrum_consumer) =
            TripleBuffer::new(&vec![SPECTRUM_FLOOR_DB; MAX_SPECTRUM_BINS]).split();

        // Initialize FFT processor with configured size
        let mut fft_planner = RealFftPlanner::<f32>::new();
        let fft_processor = fft_planner.plan_fft_forward(MAX_FFT_SIZE_USIZE);

        // Generate Hann window for maximum size
        let window_coefficients = WindowType::Hann.generate(MAX_FFT_SIZE_USIZE);

        let analyser = SpectrumProducer {
            fft_processor,
            window_coefficients,
            ring_buffer: vec![0.0; MAX_FFT_SIZE_USIZE * RING_BUFFER_SIZE_MULTIPLIER],
            ring_buffer_pos: 0,
            samples_since_fft: 0,
            time_domain_buffer: vec![0.0; MAX_FFT_SIZE_USIZE],
            frequency_domain_buffer: vec![Complex32::new(0.0, 0.0); MAX_SPECTRUM_BINS],
            spectrum_result: vec![SPECTRUM_FLOOR_DB; ResolutionLevel::Medium.to_bin_count()],
            previous_spectrum: vec![SPECTRUM_FLOOR_DB; ResolutionLevel::Medium.to_bin_count()],
            current_resolution: ResolutionLevel::Medium,
            spectrum_producer,
            fft_failure_count: std::sync::atomic::AtomicU32::new(0),
        };

        (analyser, SpectrumConsumer::new(spectrum_consumer))
    }

    /// Write silence to the spectrum buffer (used when plugin is deactivated)
    /// This ensures the UI gets actual silence instead of stale audio data
    pub fn write_silence(&mut self) {
        // Use current spectrum_result size to maintain resolution
        let silence = vec![SPECTRUM_FLOOR_DB; self.spectrum_result.len()];
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
    pub fn process(
        &mut self,
        buffer: &Buffer,
        sample_rate: f32,
        tilt: TiltLevel,
        speed: SpectrumSpeed,
        resolution: ResolutionLevel,
    ) {
        // Add incoming samples to ring buffer
        self.add_samples_to_ring_buffer(buffer);

        // Check if enough samples have been accumulated for next FFT
        if self.samples_since_fft >= (MAX_FFT_SIZE_USIZE as f32 * FFT_OVERLAP_FACTOR) as usize {
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
                self.fft_failure_count
                    .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                return;
            }

            // Check if resolution changed and resize buffers if needed
            if self.current_resolution != resolution {
                self.resize_buffers_for_resolution(resolution);
            }

            // Convert complex FFT output to magnitude spectrum and sample to target resolution
            self.compute_magnitude_spectrum(resolution);

            // Apply temporal envelope (Speed parameter - attack/release dynamics)
            self.apply_temporal_envelope(sample_rate, speed);

            // Apply tilt compensation as visual adjustment
            self.apply_tilt_compensation(sample_rate, tilt);

            // Send result to UI thread (lock-free)
            self.spectrum_producer.write(self.spectrum_result.clone());
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

    /// Copy most recent samples from ring buffer to FFT buffer
    fn copy_from_ring_buffer(&mut self) {
        let ring_len = self.ring_buffer.len();

        // Start position: current pos minus window size
        let start_pos = if self.ring_buffer_pos >= MAX_FFT_SIZE_USIZE {
            self.ring_buffer_pos - MAX_FFT_SIZE_USIZE
        } else {
            ring_len - (MAX_FFT_SIZE_USIZE - self.ring_buffer_pos)
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
        // Apply Hann window to reduce spectral leakage
        for (sample, &coeff) in self
            .time_domain_buffer
            .iter_mut()
            .zip(self.window_coefficients.iter())
        {
            *sample *= coeff;
        }
    }

    /// Resize buffers when resolution changes
    fn resize_buffers_for_resolution(&mut self, new_resolution: ResolutionLevel) {
        let new_bin_count = new_resolution.to_bin_count();

        // Resize spectrum buffers to match new resolution
        self.spectrum_result
            .resize(new_bin_count, SPECTRUM_FLOOR_DB);
        self.previous_spectrum
            .resize(new_bin_count, SPECTRUM_FLOOR_DB);

        // Update current resolution
        self.current_resolution = new_resolution;
    }

    /// Convert complex FFT output to magnitude spectrum and sample to target resolution
    fn compute_magnitude_spectrum(&mut self, resolution: ResolutionLevel) {
        // Get full magnitude spectrum from FFT
        let full_magnitude_spectrum =
            compute_magnitude_spectrum(&self.frequency_domain_buffer, MAX_FFT_SIZE_USIZE);

        // Sample to target resolution using interpolation for better quality
        let target_bin_count = resolution.to_bin_count();
        for i in 0..target_bin_count {
            // Map target bin to source bin with fractional indexing
            let source_pos =
                (i as f32 * (MAX_SPECTRUM_BINS - 1) as f32) / (target_bin_count - 1) as f32;
            let source_idx = source_pos.floor() as usize;
            let fraction = source_pos.fract();

            // Linear interpolation between adjacent bins
            let value = if source_idx + 1 < MAX_SPECTRUM_BINS {
                let current = full_magnitude_spectrum[source_idx];
                let next = full_magnitude_spectrum[source_idx + 1];
                current + (next - current) * fraction
            } else {
                full_magnitude_spectrum[source_idx]
            };

            self.spectrum_result[i] = value;
        }
    }

    /// Apply tilt compensation as final visual adjustment
    /// Tilts the spectrum around 1kHz for perceptually flat response
    fn apply_tilt_compensation(&mut self, sample_rate: f32, tilt: TiltLevel) {
        let tilt_db_per_oct = tilt.to_db_per_octave();

        // Skip if no tilt is needed
        if tilt_db_per_oct == 0.0 {
            return;
        }

        let target_bin_count = self.spectrum_result.len();
        for (bin_idx, db_value) in self.spectrum_result.iter_mut().enumerate() {
            // Only apply tilt to signals above noise floor
            if *db_value > SPECTRUM_FLOOR_DB + 10.0 {
                // Calculate frequency for this bin based on actual resolution
                // Map from decimated bin index back to frequency
                let source_pos = (bin_idx as f32 * (MAX_SPECTRUM_BINS - 1) as f32)
                    / (target_bin_count - 1) as f32;
                let freq_hz = (source_pos * sample_rate) / MAX_FFT_SIZE_USIZE as f32;

                // Apply tilt compensation
                *db_value = apply_tilt_compensation(*db_value, freq_hz, tilt_db_per_oct);
            }
        }
    }

    /// Apply temporal envelope (attack/release) controlled by Speed parameter
    fn apply_temporal_envelope(&mut self, sample_rate: f32, speed: SpectrumSpeed) {
        let (envelope_spectrum, updated_previous) = apply_temporal_envelope_sized(
            &self.spectrum_result,
            &self.previous_spectrum,
            speed,
            sample_rate,
            MAX_FFT_SIZE_USIZE,
        );
        self.spectrum_result.copy_from_slice(&envelope_spectrum);
        self.previous_spectrum.copy_from_slice(&updated_previous);
    }
}

/// Converts complex FFT output to magnitude spectrum in dB
///
/// Transforms raw FFT complex numbers into a magnitude spectrum suitable for display.
/// Applies proper scaling for single-sided spectrum, compensates for window energy loss,
/// and converts to dB scale.
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
///
/// # Scaling Explanation
/// - FFT produces two-sided spectrum, we show single-sided
/// - Factor of 2 accounts for negative frequency energy
/// - DC bin (0 Hz) has no negative counterpart, no factor of 2
/// - Window reduces amplitude by coherent gain factor
///
/// # Implementation Notes
/// - Floor at -140dB prevents log(0) errors
/// - Reference: AES17 standard for digital audio measurement
///
/// # References
/// - "Spectral Audio Signal Processing" by Julius O. Smith III
/// - AES17-2015 "AES standard method for digital audio engineering"
/// - https://ccrma.stanford.edu/~jos/sasp/Spectrum_Analysis_Windows.html
pub fn compute_magnitude_spectrum(frequency_bins: &[Complex32], window_size: usize) -> Vec<f32> {
    let window_coherent_gain = 0.5; // Hann window ACF (amplitude correction factor)
    let spectrum: Vec<f32> = frequency_bins
        .iter()
        .enumerate()
        .map(|(bin_idx, &complex_bin)| {
            // Calculate magnitude (not power)
            let magnitude = complex_bin.norm();

            // Correct scaling for magnitude spectrum with window compensation
            let nyquist_bin = window_size / 2;
            let scaling = if bin_idx == 0 || bin_idx == nyquist_bin {
                // DC and Nyquist: already single-sided, no factor of 2, no RMS conversion
                1.0 / (window_size as f32 * window_coherent_gain)
            } else {
                // AC bins: factor of 2 for single-sided, convert peak to RMS, compensate for window
                (2.0 / (2.0_f32).sqrt()) / (window_size as f32 * window_coherent_gain)
            };

            let normalized_magnitude = magnitude * scaling;

            // Convert to dBFS using 20*log10 for magnitude (not power)
            let db_value = if normalized_magnitude > MIN_AMPLITUDE_THRESHOLD {
                20.0 * normalized_magnitude.log10()
            } else {
                SPECTRUM_FLOOR_DB
            };

            // Apply floor clamping
            db_value.max(SPECTRUM_FLOOR_DB)
        })
        .collect();

    spectrum
}

/// Applies frequency-dependent tilt compensation for visual adjustment
///
/// Tilts the spectrum display around 1kHz to provide perceptually flat response.
/// This is a visual-only adjustment applied as the final step in the processing chain.
/// Common values: 3dB/oct (pink noise flat), 4.5dB/oct (natural perception)
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

/// Apply temporal envelope with attack/release dynamics (Speed parameter)
///
/// Implements fast attack and slow release for musical response:
/// - Fast attack: Immediate response to rising signals
/// - Slow release: Gradual decay controlled by Speed parameter
///
/// # Parameters
/// * `current_spectrum` - New spectrum values from current FFT frame
/// * `previous_spectrum` - Spectrum from previous frame with temporal envelope applied
/// * `speed` - Controls response time for decay characteristics
/// * `sample_rate` - Sample rate for timing calculations
/// * `fft_size` - FFT size for calculating frame rate
///
/// # Returns
/// Tuple of (envelope_applied_spectrum, updated_previous) for next iteration
pub fn apply_temporal_envelope_sized(
    current_spectrum: &[f32],
    previous_spectrum: &[f32],
    speed: SpectrumSpeed,
    sample_rate: f32,
    fft_size: usize,
) -> (Vec<f32>, Vec<f32>) {
    // Calculate envelope factor based on response time
    // The release factor determines how much of the previous value to keep
    let response_time_ms = speed.response_time_ms();

    // Calculate how many FFT frames occur per second
    let fft_hop_size = fft_size as f32 * (1.0 - FFT_OVERLAP_FACTOR);
    let fft_frames_per_second = sample_rate / fft_hop_size;

    // Calculate release factor: higher value = slower decay
    // Using exponential decay: factor = exp(-dt/tau) where tau is the time constant
    let time_constant_seconds = response_time_ms / 1000.0;
    let dt = 1.0 / fft_frames_per_second; // Time between FFT frames
    let release_factor = (-dt / time_constant_seconds).exp();

    let envelope_applied: Vec<f32> = current_spectrum
        .iter()
        .zip(previous_spectrum.iter())
        .map(|(&current_db, &previous_db)| {
            if current_db > previous_db {
                // Rising signal - immediate response (fast attack)
                current_db
            } else {
                // Falling signal - gradual decay (slow release)
                previous_db * release_factor + current_db * (1.0 - release_factor)
            }
        })
        .collect();

    (envelope_applied.clone(), envelope_applied)
}
