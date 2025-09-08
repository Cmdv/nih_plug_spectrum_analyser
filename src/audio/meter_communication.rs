use atomic_float::AtomicF32;
use nih_plug::prelude::*;
use std::sync::{atomic::Ordering, Arc};

/// Peak meter range (Pro-Q style: +12 to -60 dB)
pub const METER_MAX_DB: f32 = 12.0;
pub const METER_MIN_DB: f32 = -60.0;
pub const METER_RANGE_DB: f32 = METER_MAX_DB - METER_MIN_DB; // 72dB range

/// Smoothing factors for level meters (Pro-Q style behavior)
/// These values are calibrated to match professional meter behavior
const METER_ATTACK: f32 = 0.3; // Moderate attack (not too jumpy)
const METER_RELEASE: f32 = 0.001; // Ultra slow release for natural decay

/// Peak hold time in update cycles (approximately 1 second at 60fps)
const PEAK_HOLD_CYCLES: u32 = 60;

/// Silence threshold - below this level, trigger faster decay
const SILENCE_THRESHOLD_DB: f32 = -50.0;

/// Meter data sent from audio thread to UI thread
#[derive(Clone)]
pub struct MeterInput {
    /// Atomic peak levels for left and right channels
    /// Audio thread writes to these, UI thread reads from them
    pub peak_left: Arc<AtomicF32>,
    pub peak_right: Arc<AtomicF32>,
}

impl MeterInput {
    /// Update peak levels from audio buffer (called from audio thread)
    /// Must be real-time safe - no allocations or locks
    pub fn update_peaks(&self, buffer: &Buffer) {
        let mut left_peak = util::MINUS_INFINITY_DB;
        let mut right_peak = util::MINUS_INFINITY_DB;

        let num_channels = buffer.channels();
        if num_channels == 0 {
            return;
        }

        // Get immutable access to channel slices
        let channel_slices = buffer.as_slice_immutable();

        // Calculate peak for left channel (or mono)
        if num_channels >= 1 {
            let left_channel = &channel_slices[0];
            for &sample in left_channel.iter() {
                let sample_db = util::gain_to_db(sample.abs());
                if sample_db > left_peak {
                    left_peak = sample_db;
                }
            }
        }

        // Calculate peak for right channel
        if num_channels >= 2 {
            let right_channel = &channel_slices[1];
            for &sample in right_channel.iter() {
                let sample_db = util::gain_to_db(sample.abs());
                if sample_db > right_peak {
                    right_peak = sample_db;
                }
            }
        } else {
            // Mono: use left channel for both
            right_peak = left_peak;
        }

        // Update atomic values (lock-free communication to UI thread)
        self.peak_left.store(left_peak, Ordering::Relaxed);
        self.peak_right.store(right_peak, Ordering::Relaxed);
    }
}

/// Internal state for meter processing that needs to be shared
#[derive(Default)]
struct MeterState {
    /// Smoothed levels for display (internal state)
    smoothed_left: f32,
    smoothed_right: f32,

    /// Peak hold values for visual feedback
    peak_hold_left: f32,
    peak_hold_right: f32,
    peak_hold_value: f32, // Maximum of both channels

    /// Peak hold timer
    peak_hold_counter: u32,

    /// Silence detection counter
    silence_counter: u32,
}

/// Meter processor for UI thread - handles smoothing and peak hold
#[derive(Clone)]
pub struct MeterOutput {
    /// Reference to atomic peak values updated by audio thread
    meter_input: MeterInput,

    /// Shared internal state for smoothing and peak hold
    state: Arc<std::sync::Mutex<MeterState>>,
}

impl MeterOutput {
    /// Create new meter output processor
    fn new(meter_input: MeterInput) -> Self {
        let mut initial_state = MeterState::default();
        initial_state.smoothed_left = util::MINUS_INFINITY_DB;
        initial_state.smoothed_right = util::MINUS_INFINITY_DB;
        initial_state.peak_hold_left = util::MINUS_INFINITY_DB;
        initial_state.peak_hold_right = util::MINUS_INFINITY_DB;
        initial_state.peak_hold_value = util::MINUS_INFINITY_DB;

        Self {
            meter_input,
            state: Arc::new(std::sync::Mutex::new(initial_state)),
        }
    }

    /// Update smoothing and peak hold logic
    /// Call this from UI thread before drawing meters
    pub fn update(&self) {
        // Read current peak levels from audio thread (atomic, lock-free)
        let left_db = self.meter_input.peak_left.load(Ordering::Relaxed);
        let right_db = self.meter_input.peak_right.load(Ordering::Relaxed);

        if let Ok(mut state) = self.state.lock() {
            // Apply smoothing with attack/release characteristics
            self.update_smoothing(&mut state, left_db, right_db);

            // Update peak hold behavior
            self.update_peak_hold(&mut state, left_db, right_db);

            // Silence detection disabled - let natural release handle it
            // self.update_silence_detection(&mut state);
        }
    }

    /// Get smoothed levels for display (left, right)
    pub fn get_smoothed_levels(&self) -> (f32, f32) {
        if let Ok(state) = self.state.lock() {
            (state.smoothed_left, state.smoothed_right)
        } else {
            (util::MINUS_INFINITY_DB, util::MINUS_INFINITY_DB)
        }
    }

    /// Get peak hold value (maximum of both channels)
    pub fn get_peak_hold_db(&self) -> f32 {
        if let Ok(state) = self.state.lock() {
            state.peak_hold_value
        } else {
            util::MINUS_INFINITY_DB
        }
    }

    /// Get peak hold values for individual channels
    pub fn get_peak_hold_channels(&self) -> (f32, f32) {
        if let Ok(state) = self.state.lock() {
            (state.peak_hold_left, state.peak_hold_right)
        } else {
            (util::MINUS_INFINITY_DB, util::MINUS_INFINITY_DB)
        }
    }

    /// Apply attack/release smoothing to meter levels
    fn update_smoothing(&self, state: &mut MeterState, left_db: f32, right_db: f32) {
        // Left channel smoothing with attack/release envelope
        if left_db > state.smoothed_left {
            // Attack: fast response to increases (like a peak meter)
            state.smoothed_left =
                left_db * METER_ATTACK + state.smoothed_left * (1.0 - METER_ATTACK);
        } else {
            // Release: slow decay (prevents meter flickering)
            state.smoothed_left =
                left_db * METER_RELEASE + state.smoothed_left * (1.0 - METER_RELEASE);
        }

        // Right channel smoothing (same algorithm)
        if right_db > state.smoothed_right {
            state.smoothed_right =
                right_db * METER_ATTACK + state.smoothed_right * (1.0 - METER_ATTACK);
        } else {
            state.smoothed_right =
                right_db * METER_RELEASE + state.smoothed_right * (1.0 - METER_RELEASE);
        }
    }

    /// Update peak hold indicators (sticky peaks like hardware meters)
    fn update_peak_hold(&self, state: &mut MeterState, left_db: f32, right_db: f32) {
        // Check if we have new peak values
        let mut new_peak = false;

        if left_db > state.peak_hold_left {
            state.peak_hold_left = left_db;
            new_peak = true;
        }

        if right_db > state.peak_hold_right {
            state.peak_hold_right = right_db;
            new_peak = true;
        }

        // Update overall peak hold value (max of both channels)
        let current_peak = state.peak_hold_left.max(state.peak_hold_right);
        if current_peak > state.peak_hold_value {
            state.peak_hold_value = current_peak;
            new_peak = true;
        }

        // Reset or increment peak hold timer
        if new_peak {
            state.peak_hold_counter = 0;
        } else {
            state.peak_hold_counter += 1;

            // Release peak hold after timeout
            if state.peak_hold_counter >= PEAK_HOLD_CYCLES {
                state.peak_hold_left = util::MINUS_INFINITY_DB;
                state.peak_hold_right = util::MINUS_INFINITY_DB;
                state.peak_hold_value = util::MINUS_INFINITY_DB;
                state.peak_hold_counter = 0;
            }
        }
    }

    /// Detect silence and apply faster decay when appropriate
    fn update_silence_detection(&self, state: &mut MeterState) {
        let max_level = state.smoothed_left.max(state.smoothed_right);

        if max_level < SILENCE_THRESHOLD_DB {
            state.silence_counter += 1;

            // After a delay, apply faster linear decay to silence
            if state.silence_counter > 30 {
                // About 0.5 seconds at 60fps
                // Use linear decay in dB space for smooth, predictable decay
                let decay_rate = 0.5; // dB per frame - adjust for desired speed

                // Apply linear decay in dB space
                if state.smoothed_left > util::MINUS_INFINITY_DB {
                    state.smoothed_left -= decay_rate;
                    if state.smoothed_left < -80.0 {
                        state.smoothed_left = util::MINUS_INFINITY_DB;
                    }
                }

                if state.smoothed_right > util::MINUS_INFINITY_DB {
                    state.smoothed_right -= decay_rate;
                    if state.smoothed_right < -80.0 {
                        state.smoothed_right = util::MINUS_INFINITY_DB;
                    }
                }
            }
        } else {
            state.silence_counter = 0;
        }
    }
}

/// Factory function to create meter communication pair
/// Returns (input for audio thread, output for UI thread)
pub fn create_meter_channels() -> (MeterInput, MeterOutput) {
    let meter_input = MeterInput {
        peak_left: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
        peak_right: Arc::new(AtomicF32::new(util::MINUS_INFINITY_DB)),
    };

    let meter_output = MeterOutput::new(MeterInput {
        peak_left: meter_input.peak_left.clone(),
        peak_right: meter_input.peak_right.clone(),
    });

    (meter_input, meter_output)
}

/// Utility functions for level meter display
pub mod display_utils {
    use super::*;

    /// Convert dB level to normalized meter position (0.0 to 1.0)
    /// Used by meter display components
    pub fn level_to_normalized(level_db: f32) -> f32 {
        ((level_db - METER_MIN_DB) / METER_RANGE_DB)
            .max(0.0)
            .min(1.0)
    }

    /// Get color for meter level (Pro-Q style: green → yellow → red)
    /// Returns (r, g, b) values in 0.0-1.0 range
    pub fn get_meter_color(level_db: f32) -> (f32, f32, f32) {
        if level_db < -20.0 {
            // Green zone: -60 to -20 dB
            (0.0, 0.8, 0.0) // Green
        } else if level_db < -6.0 {
            // Yellow zone: -20 to -6 dB
            let blend = (level_db + 20.0) / 14.0; // 0.0 to 1.0
            (0.8 * blend, 0.8, 0.0) // Green to Yellow
        } else if level_db < 0.0 {
            // Orange zone: -6 to 0 dB
            let blend = (level_db + 6.0) / 6.0; // 0.0 to 1.0
            (0.8, 0.8 * (1.0 - blend), 0.0) // Yellow to Orange
        } else {
            // Red zone: 0+ dB (clipping warning)
            (1.0, 0.0, 0.0) // Red
        }
    }
}
