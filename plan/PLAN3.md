# PLAN3: Spectrum Analyzer Rebuild with Professional DSP Implementation

## Current Architecture Analysis

### Data Flow Structure
```
PluginLearn (lib.rs)
├── spectrum_analyzer: SpectrumAnalyzer     // Audio thread processing
├── spectrum_output: SpectrumOutput         // UI thread communication  
├── sample_rate: Arc<AtomicF32>            // Shared sample rate
└── audio_engine: AudioEngine              // Separate gain processing

SpectrumDisplay (spectrum_display.rs)
├── spectrum_output: SpectrumOutput        // Reads from analyzer
├── sample_rate: Arc<AtomicF32>           // For frequency calculations
└── A-weighting + logarithmic display      // Current display processing
```

### Communication Pattern
- **Triple buffer**: Lock-free audio → UI data transfer
- **Sample rate**: Shared via Arc<AtomicF32> 
- **Separation**: Audio processing (SpectrumAnalyzer) vs Display (SpectrumDisplay)

## Current Library Capabilities (Based on Documentation Research)

### 1. `apodize = "1.0"` - Window Functions
**PROVIDES:**
- `blackman_iter()`, `hanning_iter()`, `hamming_iter()`, `nuttall_iter()` - Window coefficient iterators
- `triangular_iter()` - Triangular window
- `cosine_iter()` - Generalized cosine windows
- Returns f64 values (need `.map(|x| x as f32)` for f32)

**DOES NOT PROVIDE:**
- Window compensation factors (coherent gain values)
- Equivalent noise bandwidth (ENBW) values
- Any scaling or normalization helpers

### 2. `realfft = "3.3"` - FFT Processing  
**PROVIDES:**
- `RealFftPlanner` - FFT planner for real-valued input
- Real-to-complex FFT (N → N/2+1 complex bins)
- Raw FFT output (no normalization applied)
- Efficient processing (2x faster than complex FFT for longer lengths)

**DOES NOT PROVIDE:**
- Any amplitude scaling (2/N, 1/N, etc.)
- dB conversion utilities
- Window compensation integration
- Power spectrum calculations

### 3. `nih_plug` - Plugin Framework
**PROVIDES:**
- Buffer management and sample access
- Parameter smoothing
- dB conversion utilities: `util::db_to_gain()`, formatters
- Logging: `nih_plug::nih_log!()`

**DOES NOT PROVIDE:**
- Spectrum analysis utilities
- FFT scaling helpers
- Professional audio calibration standards

### 4. `dasp = "0.11.0"` - Digital Audio Signal Processing ⭐ NEW
**PROVIDES (Highly Relevant):**
- `dasp_rms::Rms` - RMS calculation over signal windows (for RMS spectrum mode)
- `dasp_peak::FullWave` - Peak envelope detection (for current peak spectrum mode)  
- `dasp_envelope::Detector` - Envelope detection with attack/release (could improve smoothing)
- `dasp_window::Window` trait - Window function abstraction (more organized than apodize)
- `dasp::Sample` trait - Generic sample type handling (f32/f64/i16/etc.)

**PROVIDES (Less Relevant for Spectrum):**
- `dasp_interpolate` - Sample rate conversion
- `dasp_signal` - Signal generation and manipulation
- `dasp_slice` - Slice operations for audio data

**DOES NOT PROVIDE:**
- FFT processing or scaling
- dB calibration standards
- Pink noise tilt compensation
- Window compensation factors
- Professional spectrum analysis mathematics

## Problems with Current Implementation

### 1. **Incorrect FFT Scaling** (Critical Issue)
```rust
// WRONG: Current implementation (line 191)
let normalized_magnitude = magnitude / (SPECTRUM_WINDOW_SIZE as f32).sqrt();

// CORRECT: Should be 2/N scaling for single-sided spectrum
let amplitude = magnitude * 2.0 / (SPECTRUM_WINDOW_SIZE as f32 * coherent_gain);
```

### 2. **Arbitrary dB Calibration** (Critical Issue)
```rust
// WRONG: Current +36dB offset is arbitrary
20.0 * normalized_magnitude.log10() + 36.0

// CORRECT: Should reference full-scale sine wave (0 dBFS)
20.0 * (amplitude / reference_amplitude).log10()
```

### 3. **Missing Window Compensation** (Critical Issue)
- Using Blackman window without compensating for amplitude loss
- Coherent gain for Blackman = 0.42 (requires 2.38x correction)

### 4. **No Pink Noise Tilt** (Major Issue) 
- Missing 4.5 dB/octave compensation for modern spectrum display
- Causes high frequencies to appear too quiet

### 5. **Spectrum Shows All Bins** (Debug Issue)
- 1kHz sine showing energy across all frequencies
- Indicates fundamental processing error

## Implementation Plan

### Phase 1: Fix Core DSP Mathematics

#### 1.1 Window Compensation Constants
**NEED TO IMPLEMENT** (not provided by apodize):
```rust
const BLACKMAN_COHERENT_GAIN: f32 = 0.42;
const HANNING_COHERENT_GAIN: f32 = 0.50;
const HAMMING_COHERENT_GAIN: f32 = 0.54;
const FLAT_TOP_COHERENT_GAIN: f32 = 0.22;
```

#### 1.2 Correct FFT Amplitude Scaling
**IMPLEMENT** (realfft provides raw output only):
```rust
// From spectrum.md: Single-sided amplitude scaling
fn calculate_amplitude(fft_magnitude: f32, fft_size: usize, coherent_gain: f32) -> f32 {
    // Factor of 2 for single-sided spectrum (except DC and Nyquist)
    let scaling = if bin_index == 0 || (fft_size % 2 == 0 && bin_index == fft_size/2) {
        1.0 / (fft_size as f32)  // DC and Nyquist bins
    } else {
        2.0 / (fft_size as f32)  // Other bins
    };
    
    // Apply window compensation
    fft_magnitude * scaling / coherent_gain
}
```

#### 1.3 Professional dB Calibration
**IMPLEMENT** (following AES17 standard from spectrum.md):
```rust
// 0 dBFS = full-scale sine wave (AES17 standard)
fn amplitude_to_dbfs(amplitude: f32) -> f32 {
    // Reference: full-scale sine has amplitude 1.0
    20.0 * amplitude.log10()
}
```

#### 1.4 Pink Noise Tilt Compensation
**IMPLEMENT** (from spectrum.md):
```rust
fn apply_pink_noise_tilt(magnitude_db: f32, frequency: f32) -> f32 {
    let octaves_from_1khz = (frequency / 1000.0).log2();
    let tilt_compensation = 4.5 * octaves_from_1khz;  // Modern standard
    magnitude_db + tilt_compensation
}
```

### Phase 2: Rebuild SpectrumAnalyzer

#### 2.1 New compute_magnitude_spectrum() Method
```rust
fn compute_magnitude_spectrum(&mut self, sample_rate: f32) {
    for (bin_idx, &complex_bin) in self.frequency_domain_buffer.iter().enumerate() {
        // 1. Raw magnitude from FFT
        let raw_magnitude = complex_bin.norm();
        
        // 2. Apply proper FFT scaling with window compensation
        let amplitude = calculate_amplitude(raw_magnitude, SPECTRUM_WINDOW_SIZE, BLACKMAN_COHERENT_GAIN);
        
        // 3. Convert to dB with professional calibration
        let db_value = if amplitude > 0.0 {
            amplitude_to_dbfs(amplitude)
        } else {
            SPECTRUM_FLOOR_DB
        };
        
        // 4. Apply pink noise tilt for modern spectrum display
        let frequency = bin_to_frequency(bin_idx, sample_rate);
        let compensated_db = apply_pink_noise_tilt(db_value, frequency);
        
        self.spectrum_result[bin_idx] = compensated_db.max(SPECTRUM_FLOOR_DB);
    }
}
```

#### 2.2 Pre-compute Window Coefficients
```rust
impl SpectrumAnalyzer {
    pub fn new() -> (Self, SpectrumOutput) {
        // Use apodize for window generation
        let window_function: Vec<f32> = blackman_iter(SPECTRUM_WINDOW_SIZE)
            .map(|w| w as f32)  // Convert f64 to f32
            .collect();
            
        // Pre-calculate coherent gain for efficiency
        let coherent_gain = window_function.iter().sum::<f32>() / SPECTRUM_WINDOW_SIZE as f32;
        // Should equal 0.42 for Blackman window
        
        // ... rest of initialization
    }
}
```

### Phase 3: Testing and Validation

#### 3.1 Test with 1kHz Sine Wave
- **Expected**: Single peak at bin ~85 (1kHz @ 48kHz sample rate)
- **Expected dB**: Should read -6 dB for 0.5 amplitude sine wave
- **Other bins**: Should be at noise floor (-120 dB)

#### 3.2 Test with Full-Scale Sine
- **Expected**: Peak should read 0 dBFS exactly
- **Validation**: Confirms proper AES17 calibration

#### 3.3 Compare with Bitwig Spectrum
- Apply 4.5 dB/octave tilt → pink noise should appear flat
- Frequency response should match professional analyzers

### Phase 4: Advanced Features (Future) - Leveraging dasp

#### 4.1 Multiple Window Options
- Add support for Hanning, Hamming, Flat-top windows using `dasp_window::Window` trait
- Each with proper coherent gain compensation (still need to implement coefficients)
- More organized architecture than current apodize approach

#### 4.2 RMS vs Peak Mode using dasp
- Current implementation shows **peak spectrum** (instantaneous magnitude)
- Add **RMS spectrum mode** using `dasp_rms::Rms` for perceived loudness measurement
- Professional analyzers often offer both modes for different use cases

```rust
// Future RMS spectrum implementation
use dasp_rms::Rms;

// RMS over frequency bins instead of time domain
let mut rms_detector = Rms::new(rms_window_size);
for bin_magnitude in fft_magnitudes {
    let rms_value = rms_detector.next([bin_magnitude]); // Returns RMS over window
}
```

#### 4.3 Improved Smoothing using dasp Envelope Detection
- Current attack/release is basic single-pole IIR
- Use `dasp_envelope::Detector` for more sophisticated envelope following:

```rust
use dasp_envelope::{Detector, Detect};
use dasp_peak::FullWave;

// More professional envelope detection per spectrum bin
let detector = Detector::new(FullWave, attack_samples, release_samples);
let smoothed_magnitude = detector.detect(raw_magnitude);
```

#### 4.4 Multi-Mode Spectrum Analyzer
- **Peak Mode**: Current implementation (instant magnitude)
- **RMS Mode**: Using `dasp_rms::Rms` for perceptual accuracy
- **Average Mode**: Long-term averaging for mastering applications
- **Max Hold**: Peak hold with slow decay (current basic implementation)

## Implementation Guidelines

### What We Must Implement
1. **Window compensation factors** - apodize only provides coefficients
2. **FFT amplitude scaling** - realfft provides raw output
3. **dB calibration with AES17 standard** - no library provides this
4. **Pink noise tilt compensation** - audio-specific requirement
5. **Frequency-dependent corrections** - spectrum analyzer specific

### What We Can Use Directly
1. **Window coefficients** - apodize handles mathematical generation
2. **FFT computation** - realfft handles efficient real→complex transform
3. **dB formatting** - nih_plug utilities for parameter display
4. **Triple buffer** - already working for communication
5. **Atomic sample rate** - already working for shared state

### Architecture Preservation
- Keep current data flow: SpectrumAnalyzer → SpectrumOutput → SpectrumDisplay
- Maintain triple buffer communication (it's working correctly)
- Preserve separation of audio processing vs UI display
- Keep current parameter and gain processing separate

## Success Criteria

1. **1kHz sine test**: Shows single narrow peak at correct frequency
2. **dB calibration**: Full-scale sine reads exactly 0 dBFS
3. **Visual match**: Spectrum appearance matches Bitwig with 4.5 dB/oct tilt
4. **No performance regression**: Maintains real-time audio thread safety
5. **Professional accuracy**: Matches industry standard spectrum analyzers

This plan addresses all mathematical issues identified in spectrum.md while leveraging existing working architecture and available library capabilities.