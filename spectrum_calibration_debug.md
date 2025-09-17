# Spectrum Analyzer Calibration Debug Log

## Problem Description
The spectrum analyzer reads approximately 10-12dB too low compared to professional analyzers (Bitwig's spectrum, Pro-Q 4). A 0dBFS sine wave at 1kHz shows as around -10 to -12dB in our analyzer, while other analyzers correctly show 0dB.

## What We've Tried and Results

### 1. ✅ Fixed Display Range (0dB Reference)
**Issue**: Display was using +20dB as maximum instead of 0dB
**Fix**: Updated constants to use 0dB max, fixed grid generation
**Result**: Grid and display scaling now correct, but spectrum values still low

### 2. ❌ Suspected Spectral Leakage
**Theory**: 1000Hz doesn't align with FFT bins causing energy spread
**Test**: Tried 990Hz (close to bin frequency 990.5Hz)
**Result**: Still shows -10dB, so spectral leakage not the main issue

### 3. ❌ Tried RMS to Peak Conversion
**Theory**: FFT might be giving RMS values, spectrum analyzers show peak
**Fix**: Added `* 1.414` (√2) factor to amplitude calculation
**Result**: Helped ~3dB but still ~10dB too low

### 4. ⚠️ Suspected Double Windowing (UNCLEAR)
**Theory**: Window applied twice - once in `apply_window()`, once in adaptive windowing
**Attempted Fix**: Commented out `self.apply_window()` call
**Result**: User reports it didn't change (need to verify this was actually applied)

### 5. ✅ Added Comprehensive Logging
**Added**: FFT processing logs showing magnitude, scaling, coherent gain, amplitude, dB
**Found**:
- Coherent gain correct (0.5 for Hann, 0.42 for Blackman)
- Scaling factor: 0.000977 (= 2.0/2048)
- For 1kHz: magnitude ~357, amplitude ~0.70, result ~-3dB per bin

## Current Logging Output (990Hz test)
```
FFT bin 46: freq=990.5Hz, magnitude=356.765503, scaling=0.000977, coherent_gain=0.500000, amplitude=0.696808, db=-3.1dB
```

## What We've Ruled Out
- ❌ Display mapping issues (grid shows correct 0dB reference)
- ❌ Spectral leakage (tested with aligned frequency)
- ❌ Input signal level (Pro-Q and Bitwig show 0dB for same signal)
- ❌ Coherent gain values (0.5 is correct for Hann window)

## Remaining Suspects

### 1. FFT Scaling Factor
- Current: `2.0 / window_size` = `2.0 / 2048` = 0.000977
- **Question**: Is this the correct scaling for single-sided spectrum?
- **Theory**: Might need different scaling (e.g., `1.0 / window_size` or other factor)

### 2. Double Windowing (Unconfirmed)
- **Status**: User couldn't see the commented line take effect
- **Need**: Verify the `apply_window()` call is actually disabled
- **Check**: Line 182 in spectrum.rs should be commented out

### 3. Amplitude Calculation Order
- Current: `magnitude * scaling / window_coherent_gain * 1.414`
- **Question**: Is the order of operations correct?
- **Alternative**: Try `magnitude * scaling * 1.414 / window_coherent_gain`

### 4. FFT Implementation Details
- **Question**: Does the realfft crate require different normalization?
- **Check**: realfft documentation for proper scaling factors
- **Alternative**: Compare with other FFT implementations

## Next Debugging Steps

### 1. Verify Double Windowing Fix
```rust
// In process() method around line 182, ensure this line is commented:
// self.apply_window(); // Disabled - adaptive windowing handles this
```

### 2. Test Different Scaling Factors
Try these alternatives in `compute_magnitude_spectrum()`:
```rust
// Option A: Remove factor of 2
let scaling = 1.0 / window_size as f32;

// Option B: Different normalization
let amplitude = magnitude / window_size as f32 / window_coherent_gain * 2.0;

// Option C: Check realfft docs for proper scaling
```

### 3. Add More Detailed Logging
Add logging for:
- Raw input samples before windowing
- Samples after windowing but before FFT
- Raw FFT output before any scaling

### 4. Compare with Reference Implementation
- Find a working Rust FFT spectrum analyzer
- Compare scaling factors and normalization steps
- Verify against known test signals

## Key Files to Check
- `src/audio/spectrum.rs` - Main FFT processing
- `src/audio/constants.rs` - Display range constants (already fixed)
- `src/ui/spectrum_display.rs` - Display mapping (seems correct)

## Test Signal Details
- **Frequency**: 990Hz (close to FFT bin 990.5Hz)
- **Amplitude**: 0dBFS
- **Expected Result**: 0dB on spectrum analyzer
- **Actual Result**: ~-10dB
- **Reference**: Pro-Q 4 and Bitwig show 0dB correctly