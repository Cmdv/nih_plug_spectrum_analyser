use nih_plug::buffer::Buffer;

use crate::audio::buffer::WaveformBuffer;
use std::sync::{Arc, Mutex};

pub struct AudioProcessor {
    waveform_buffer: Arc<Mutex<WaveformBuffer>>,
    mono_scratch: Vec<f32>, // Pre-allocated, reused each call
}

impl AudioProcessor {
    pub fn new(waveform_buffer: Arc<Mutex<WaveformBuffer>>, max_buffer_size: usize) -> Self {
        Self {
            waveform_buffer,
            mono_scratch: Vec::with_capacity(max_buffer_size), // Pre-allocate reasonable size
        }
    }

    pub fn process_buffer(&mut self, buffer: &mut Buffer, gain: f32) {
        self.mono_scratch.clear();

        for mut channel_samples in buffer.iter_samples() {
            // Collect into small fixed array (no heap allocation!)
            let mut channel_values = [0.0f32; 2];
            let mut channel_count = 0;

            for (i, sample) in channel_samples.iter_mut().enumerate() {
                if i < 2 {
                    channel_values[i] = *sample;
                    channel_count += 1;
                }
                *sample *= gain; // Apply gain
            }

            // Calculate mono from our fixed array
            let mono = if channel_count >= 2 {
                (channel_values[0] + channel_values[1]) * 0.5
            } else if channel_count >= 1 {
                channel_values[0]
            } else {
                0.0
            };

            self.mono_scratch.push(mono);
        }

        if let Ok(mut buffer) = self.waveform_buffer.lock() {
            buffer.write_samples(&self.mono_scratch);
        }
    }
}
