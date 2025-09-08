# Professional spectrum analyzer mathematics for audio plugins

The mathematics behind professional spectrum analyzers requires precise implementation of FFT scaling, window compensation, dB calibration, perceptual corrections, and visual smoothing to match industry standards. This comprehensive guide provides the specific formulas, scaling factors, and numerical values needed for implementation in audio VST/CLAP plugins targeting music producers.

## Proper FFT conversion and scaling mathematics

Professional spectrum analyzers convert time-domain audio to frequency domain using specific scaling conventions that ensure accurate amplitude readings. The industry standard approach scales FFT results to maintain amplitude consistency regardless of FFT size.

For converting a real-valued audio signal to a single-sided spectrum, the fundamental formula applies a factor of 2 to account for negative frequencies (except DC and Nyquist bins):

```
Single-sided amplitude = 2 × |FFT(x)[k]| / N  (for k = 1 to N/2-1)
Single-sided amplitude = |FFT(x)[k]| / N      (for k = 0, DC component)
```

Where N represents the FFT length. This **1/N scaling** maintains amplitude consistency and represents the most common approach in professional audio analysis tools. The factor of 2 compensates for the energy in negative frequencies that gets folded into the positive frequency display.

For power spectrum calculations, square the amplitude spectrum:
```
Power[k] = |FFT(x)[k]|² / N²
```

The RMS amplitude spectrum, used for comparing with time-domain RMS values, requires an additional √2 factor for sinusoidal components:
```
RMS Amplitude[k] = Amplitude[k] / √2
```

Energy conservation between time and frequency domains follows **Parseval's theorem**, ensuring that ∑|x[n]|² = (1/N) × ∑|X[k]|². This relationship validates proper FFT scaling implementation.

## Window compensation factors for accurate measurements

Windowing functions reduce spectral leakage but attenuate signal amplitude, requiring compensation to restore accurate measurements. Two critical compensation factors determine the correction needed: coherent gain for amplitude accuracy and equivalent noise bandwidth (ENBW) for noise measurements.

The **coherent gain** represents the normalized sum of window coefficients:
```
Coherent Gain = (1/N) × Σ(w[n])
```

Critical window compensation values for professional implementations:

**Hann window**: Coherent gain = **0.50**, requiring 2.0× amplitude correction. ENBW = **1.50** bins for noise measurements. This window provides excellent general-purpose performance with -32 dB sidelobe suppression.

**Blackman window**: Coherent gain = **0.42**, requiring 2.38× amplitude correction. ENBW = **1.73** bins. Offers superior -58 dB sidelobe suppression for high-dynamic-range measurements.

**Hamming window**: Coherent gain = **0.54**, requiring 1.85× amplitude correction. ENBW = **1.36** bins. Optimized for close frequency separation with -43 dB sidelobes.

**Flat-top window**: Coherent gain = **0.22**, requiring 4.18× amplitude correction. ENBW = **3.77** bins. Provides maximum amplitude accuracy at the expense of frequency resolution.

To apply window compensation:
```
Corrected Amplitude = Raw FFT Amplitude / Coherent Gain
```

For noise measurements, convert to Power Spectral Density using ENBW:
```
PSD[k] = |FFT(x)[k]|² / (N² × Δf × ENBW)
```
Where Δf = fs/N represents frequency resolution.

## Industry-standard dB calibration with 0 dB = full scale sine

The **AES17-2020 standard** defines 0 dBFS as the RMS value of a full-scale sine wave, creating a critical **+3.01 dB offset** from mathematical RMS calculations. This calibration standard ensures consistency across professional audio tools.

The calibration formula incorporates this offset:
```
dBFS = 20 × log₁₀(RMS_signal × √2)
     = 20 × log₁₀(RMS_signal) + 3.0103 dB
```

For a full-scale sine wave with peak amplitude of 1.0, the RMS value equals 1/√2 = 0.7071. Applying the formula yields exactly 0 dBFS, confirming proper calibration. A full-scale square wave measures **+3.01 dBFS** under this standard, precisely 3.010299956 dB higher than the sine reference.

FFT spectrum analyzer calibration requires scaling the reference level appropriately:
```
dB_spectrum = 20 × log₁₀(|FFT_output| / Reference)
where Reference = N × A_fullscale / 2
```

This reference accounts for both FFT scaling and the expected magnitude of a full-scale sine wave in the frequency domain. Professional implementations follow alignment standards like **EBU R68** (-18 dBFS = 0 VU) or **SMPTE** (-20 dBFS alignment) for broadcast compatibility.

## Pink noise tilt compensation for perceptual flatness

Pink noise exhibits a **-3 dB/octave slope** because it contains equal energy per octave rather than per Hz. This characteristic aligns with human auditory processing, making pink noise sound perceptually flat despite its technical slope.

The compensation formula adjusts spectrum display for perceptual accuracy:
```
compensation_dB = -10 × log₁₀(frequency/reference_frequency)
```

For implementation, apply frequency-dependent scaling:
```
scaling_factor = √(reference_frequency/frequency)
```

The square root accounts for the relationship between power and amplitude. Reference frequency typically equals 1000 Hz by convention.

Professional tools implement various slope settings for different applications. **3.0 dB/octave** compensation makes pink noise appear flat, suitable for traditional mixing and system calibration. **4.5 dB/octave** has become the modern standard, better matching contemporary music production's frequency balance. FabFilter Pro-Q and Voxengo SPAN default to this 4.5 dB/octave setting, described as creating a "natural looking spectrum" for modern productions.

Per-bin compensation applies the correction to each FFT bin individually:
```
compensated_magnitude = original_magnitude × 10^(slope × log₁₀(f/f_ref) / 20)
```

Band-averaged compensation, more computationally efficient for real-time display, applies corrections to octave or fractional-octave bands, better matching the critical band processing of human hearing.

## Visual smoothing and animation techniques

Professional spectrum analyzers employ sophisticated smoothing algorithms to create responsive yet stable displays. The fundamental approach uses single-pole IIR filtering with different time constants for attack and release phases.

The core smoothing formula:
```
smoothed_value = α × new_value + (1-α) × previous_value
```

Converting time constants to filter coefficients:
```
α = 1 - exp(-1 / (time_constant × sample_rate))
```

**Attack time ranges** span **0-100ms** for typical implementations. Peak Program Meters (PPM) use **1.7ms** time constants for broadcast standards, while VU meters employ **300ms** for both rise and fall times. Fast response modes typically use 35ms attack times for transient detection.

**Release time ranges** extend from **100-5000ms**. PPM meters specify **650ms** time constants (1.5 seconds to -20dB decay), while extended release times up to 5 seconds provide very smooth displays for mastering applications.

Different coefficients for attack and release create asymmetric response:
```
if (new_value > current_value) {
    alpha = attack_coefficient  // Fast rise
} else {
    alpha = release_coefficient  // Slow decay
}
current_value += alpha × (new_value - current_value)
```

**Peak hold algorithms** enhance readability by maintaining maximum values briefly:
```
if (new_value > peak_value) {
    peak_value = new_value
    hold_timer = hold_time_samples  // Typically 500-2000ms
} else if (hold_timer > 0) {
    hold_timer--
} else {
    peak_value *= decay_factor  // 0.999-0.9999 for smooth decay
}
```

Frame rate considerations affect coefficient scaling. At **60fps** (professional standard), scale alpha values:
```
scaled_alpha = 1 - pow(1-alpha, target_fps/actual_fps)
```

Typical alpha coefficient ranges provide different response characteristics. **Fast response** uses α = 0.3-0.7 for immediate tracking. **Medium response** employs α = 0.1-0.3 for balanced smoothing. **Slow response** applies α = 0.01-0.1 for maximum stability.

## Implementation recommendations and best practices

Combining all components requires careful attention to processing order. First apply windowing with appropriate compensation factors. Then perform FFT with proper 2/N scaling for single-sided spectra. Apply the AES17 dBFS calibration with +3.01 dB offset for sine wave reference. Implement pink noise compensation at 3.0 or 4.5 dB/octave based on target application. Finally, apply IIR smoothing with asymmetric attack/release coefficients.

Professional tools like Waves PAZ Analyzer implement 52 or 68 band analysis with selectable peak/RMS modes and 0.1Hz resolution below 250Hz. FabFilter Pro-Q offers multiple FFT sizes (1024-8192) with configurable speed settings controlling release rates. Voxengo SPAN provides adjustable visual slopes with multiple simultaneous spectrum displays including max hold and averaging modes.

For Rust implementation, focus on computational efficiency through single-pole IIR filters, cached frequency-dependent calculations, and appropriate numeric precision. Consider SIMD optimizations for parallel bin processing and GPU acceleration for complex visual rendering.

The mathematics presented here form the foundation of professional spectrum analysis, ensuring accurate measurements, perceptually relevant displays, and smooth visual performance matching industry expectations. These specific formulas and values enable implementation of spectrum analyzers comparable to leading commercial tools.