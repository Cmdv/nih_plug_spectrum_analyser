use nih_plug::prelude::*;

/// Simplified audio engine for gain plugin
/// Handles only core gain processing - spectrum and meter analysis
/// are now handled by separate communication channels
pub struct AudioEngine {
    // No complex state needed for a simple gain plugin
}

impl AudioEngine {
    pub fn new() -> Self {
        Self {}
    }

    /// Apply gain to audio buffer with parameter smoothing
    /// This is the core audio processing for a gain plugin
    pub fn process(&mut self, buffer: &mut Buffer, gain_param: &FloatParam) {
        // Apply gain with parameter smoothing
        for mut channel_samples in buffer.iter_samples() {
            let gain = gain_param.smoothed.next();
            for sample in channel_samples.iter_mut() {
                *sample *= gain;
            }
        }
    }
}
