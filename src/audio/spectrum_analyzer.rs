use core::f32::consts::PI;
use libm::cosf;
use nih_plug::prelude::*;
use realfft::{num_complex::Complex32, RealFftPlanner, RealToComplex};
use std::sync::Arc;
use triple_buffer::TripleBuffer;

/// The size of our FFT analysis window
/// 2048 gives us 23.4Hz resolution at 48kHz (good for 20Hz-20kHz range)
pub const SPECTRUM_WINDOW_SIZE: usize = 2048;

/// Number of frequency bins produced by the FFT (N/2 + 1 for real FFT)
pub const SPECTRUM_BINS: usize = SPECTRUM_WINDOW_SIZE / 2 + 1;

/// Spectrum analyzer floor prevents log(0) in FFT calculations
const SPECTRUM_FLOOR_DB: f32 = -120.0;

/// Time constant for spectrum attack (fast response to increases)
const SPECTRUM_ATTACK: f32 = 0.3;  // Faster attack for testing

/// Time constant for spectrum release (slow decay)  
const SPECTRUM_RELEASE: f32 = 0.05;  // Faster release to reduce "rocking"

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
            ring_buffer: vec![0.0; SPECTRUM_WINDOW_SIZE * 2], // 2x size for overlap
            ring_buffer_pos: 0,
            samples_since_fft: 0,
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
        // Add incoming samples to ring buffer
        self.add_samples_to_ring_buffer(buffer);
        
        // Check if we should process FFT (50% overlap = every WINDOW_SIZE/2 samples)
        if self.samples_since_fft >= SPECTRUM_WINDOW_SIZE / 2 {
            self.samples_since_fft = 0;
            
            // Copy from ring buffer to FFT buffer
            self.copy_from_ring_buffer();
            
            // Debug: Comprehensive spectral leakage analysis
            static mut DEBUG_COUNTER: u32 = 0;
            unsafe {
                DEBUG_COUNTER += 1;
                if DEBUG_COUNTER % 120 == 0 {
                    let max_sample = self.time_domain_buffer.iter()
                        .map(|s| s.abs())
                        .fold(0.0f32, f32::max);
                    let rms = (self.time_domain_buffer.iter()
                        .map(|s| s * s)
                        .sum::<f32>() / SPECTRUM_WINDOW_SIZE as f32)
                        .sqrt();
                    nih_log!("Time domain: max={:.3}, RMS={:.3}", max_sample, rms);
                    
                    // Check for DC offset and phase discontinuities
                    let dc_offset = self.time_domain_buffer.iter().sum::<f32>() / SPECTRUM_WINDOW_SIZE as f32;
                    nih_log!("DC offset: {:.6}", dc_offset);
                    
                    // Check frequency bin alignment for 1kHz
                    let exact_bin_1k = (1000.0 * SPECTRUM_WINDOW_SIZE as f32) / sample_rate;
                    let bin_error = exact_bin_1k - exact_bin_1k.round();
                    nih_log!("1kHz bin alignment: exact={:.3}, error={:.3}", exact_bin_1k, bin_error);
                    
                    // Check for signal periodicity issues
                    let samples_per_1k_cycle = sample_rate / 1000.0;
                    let cycles_in_window = SPECTRUM_WINDOW_SIZE as f32 / samples_per_1k_cycle;
                    let integer_cycles = cycles_in_window.round();
                    let cycle_error = cycles_in_window - integer_cycles;
                    nih_log!("1kHz cycles: {:.3} cycles, {:.3} integer, error={:.3}", 
                            cycles_in_window, integer_cycles, cycle_error);
                }
            }
            
            // Apply windowing to reduce spectral leakage
            self.apply_window();
            
            // Debug: Check after windowing
            unsafe {
                if DEBUG_COUNTER % 120 == 0 {
                    let max_windowed = self.time_domain_buffer.iter()
                        .map(|s| s.abs())
                        .fold(0.0f32, f32::max);
                    nih_log!("After window: max={:.3}, gain={:.3}", max_windowed, self.window_coherent_gain);
                }
            }
            
            // Perform FFT: time domain -> frequency domain
            if let Err(_) = self.fft_processor.process(
                &mut self.time_domain_buffer,
                &mut self.frequency_domain_buffer,
            ) {
                // FFT failed - skip this frame to maintain real-time safety
                return;
            }
            
            // Debug: Analyze spectral distribution across wide frequency range
            unsafe {
                if DEBUG_COUNTER % 120 == 0 {
                    nih_log!("FFT spectral analysis - examining leakage pattern:");
                    
                    // Look at frequency range from 200Hz to 2kHz to see leakage pattern
                    let start_freq = 200.0;
                    let end_freq = 2000.0;
                    let start_bin = ((start_freq * SPECTRUM_WINDOW_SIZE as f32) / sample_rate) as usize;
                    let end_bin = ((end_freq * SPECTRUM_WINDOW_SIZE as f32) / sample_rate) as usize;
                    
                    nih_log!("  Scanning bins {} to {} ({:.0}Hz to {:.0}Hz)", start_bin, end_bin, start_freq, end_freq);
                    
                    // Sample every 5th bin to avoid spam but get good coverage
                    for bin in (start_bin..=end_bin.min(self.frequency_domain_buffer.len()-1)).step_by(5) {
                        let magnitude = self.frequency_domain_buffer[bin].norm();
                        let freq = (bin as f32 * sample_rate) / SPECTRUM_WINDOW_SIZE as f32;
                        let raw_db = if magnitude > 0.0 {
                            20.0 * magnitude.log10()
                        } else {
                            -120.0
                        };
                        
                        // Only log bins with significant energy (above -80dB)
                        if raw_db > -80.0 {
                            nih_log!("  Bin {}: {:.0}Hz, mag={:.6}, raw_dB={:.1}", 
                                    bin, freq, magnitude, raw_db);
                        }
                    }
                    
                    // Also check the exact 1kHz region for reference
                    let expected_1k_bin = ((1000.0 * SPECTRUM_WINDOW_SIZE as f32) / sample_rate) as usize;
                    let mag_1k = self.frequency_domain_buffer[expected_1k_bin].norm();
                    let db_1k = if mag_1k > 0.0 { 20.0 * mag_1k.log10() } else { -120.0 };
                    nih_log!("  1kHz reference: bin {}, mag={:.6}, raw_dB={:.1}", expected_1k_bin, mag_1k, db_1k);
                }
            }

            // Convert complex FFT output to magnitude spectrum in dB
            self.compute_magnitude_spectrum(sample_rate);
            
            // Debug: Check final dB values
            unsafe {
                if DEBUG_COUNTER % 120 == 0 {
                    for i in 0..5 {
                        let freq = (i as f32 * sample_rate) / SPECTRUM_WINDOW_SIZE as f32;
                        nih_log!("Final bin {} @ {:.0}Hz: {:.1}dB", i, freq, self.spectrum_result[i]);
                    }
                    let expected_bin = (1000.0 * SPECTRUM_WINDOW_SIZE as f32 / sample_rate) as usize;
                    for i in (expected_bin.saturating_sub(2))..=(expected_bin + 2) {
                        if i < self.spectrum_result.len() {
                            let freq = (i as f32 * sample_rate) / SPECTRUM_WINDOW_SIZE as f32;
                            nih_log!("Final bin {} @ {:.0}Hz: {:.1}dB", i, freq, self.spectrum_result[i]);
                        }
                    }
                }
            }
            
            // Apply perceptual smoothing (attack/release envelope)
            self.apply_spectrum_smoothing();
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

    /// Apply windowing and store result in internal buffer
    fn apply_window(&mut self) {
        let windowed = apply_window(&self.time_domain_buffer, &self.window_function);
        self.time_domain_buffer.copy_from_slice(&windowed);
    }

    /// Convert complex FFT output to magnitude spectrum and store in internal buffer
    fn compute_magnitude_spectrum(&mut self, sample_rate: f32) {
        let magnitude_spectrum = compute_magnitude_spectrum(
            &self.frequency_domain_buffer,
            SPECTRUM_WINDOW_SIZE,
            self.window_coherent_gain,
            sample_rate,
        );
        
        // Debug: Find peak bin and its value
        let (peak_bin, peak_value) = magnitude_spectrum
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, &v)| (i, v))
            .unwrap_or((0, -120.0));
        
        let peak_freq = (peak_bin as f32 * sample_rate) / (SPECTRUM_WINDOW_SIZE as f32);
        
        // Only log every ~60 frames to avoid spam
        static mut FRAME_COUNT: u32 = 0;
        unsafe {
            FRAME_COUNT += 1;
            if FRAME_COUNT % 60 == 0 {
                // Count bins above -60dB around the peak
                let significant_bins: Vec<(usize, f32)> = magnitude_spectrum
                    .iter()
                    .enumerate()
                    .filter(|(_, &v)| v > -60.0)
                    .map(|(i, &v)| (i, v))
                    .collect();
                
                nih_log!("Peak: bin {} @ {:.0}Hz = {:.1}dB | {} bins > -60dB", 
                    peak_bin, peak_freq, peak_value, significant_bins.len());
                
                // Show the first few significant bins
                if significant_bins.len() < 20 {
                    for (bin, val) in significant_bins.iter().take(5) {
                        let freq = (*bin as f32 * sample_rate) / (SPECTRUM_WINDOW_SIZE as f32);
                        nih_log!("  Bin {} @ {:.0}Hz = {:.1}dB", bin, freq, val);
                    }
                }
            }
        }
        
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
pub fn compute_magnitude_spectrum(
    frequency_bins: &[Complex32],
    window_size: usize,
    window_coherent_gain: f32,
    sample_rate: f32,
) -> Vec<f32> {
    frequency_bins
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
            
            // Debug first few bins
            static mut LOG_ONCE: bool = false;
            unsafe {
                if !LOG_ONCE && bin_idx < 5 {
                    let freq = (bin_idx as f32 * sample_rate) / window_size as f32;
                    nih_log!("Bin {} @ {:.0}Hz: mag={:.6}, scaling={:.6}, coherent_gain={:.3}, amplitude={:.6}", 
                        bin_idx, freq, magnitude, scaling, window_coherent_gain, amplitude);
                    if bin_idx == 4 { LOG_ONCE = true; }
                }
            }
            
            // Convert to dBFS according to AES17 standard (spectrum.md)
            let db_value = if amplitude > 1e-8 {
                20.0 * amplitude.log10()
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

/// Apply pink noise tilt compensation for perceptually flat response
/// Modern analyzers use 4.5 dB/octave tilt to make pink noise appear flat
pub fn apply_pink_noise_tilt(magnitude_db: f32, frequency_hz: f32) -> f32 {
    if frequency_hz <= 0.0 {
        return magnitude_db;
    }
    // Calculate octaves from 1kHz reference
    let octaves_from_1khz = (frequency_hz / 1000.0).log2();
    // Apply 4.5 dB/octave compensation
    let tilt_compensation = 4.5 * octaves_from_1khz;
    magnitude_db + tilt_compensation
}
