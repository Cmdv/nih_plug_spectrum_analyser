use nih_plug::prelude::*;

/// Apply gain adjustment to audio buffer using smoothed parameter values
///
/// The gain parameter provides smoothed values to prevent audio artifacts
/// during parameter changes. Each sample frame gets a potentially different
/// gain value as the smoother interpolates between old and new parameter values.
pub fn apply_gain(buffer: &mut Buffer, gain_param: &FloatParam) {
    for mut channel_samples in buffer.iter_samples() {
        let gain = gain_param.smoothed.next();
        // Apply uniform gain to all samples in the frame
        // Multiplies each sample by the gain factor. Gain values:
        // - 1.0 = unity gain (no change)
        // - > 1.0 = amplification
        // - < 1.0 = attenuation
        // - 0.0 = silence
        for sample in channel_samples.iter_mut() {
            *sample *= gain;
        }
    }
}
