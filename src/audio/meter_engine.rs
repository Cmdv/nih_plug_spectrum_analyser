use crate::audio::constants;
use atomic_float::AtomicF32;
use std::sync::{atomic::Ordering, Arc, Mutex};

/// Processes meter levels for display - handles smoothing, peak hold, etc.
pub struct MeterEngine {
    // Raw input levels from audio thread
    pub input_left: Arc<AtomicF32>,
    pub input_right: Arc<AtomicF32>,

    // Processed values for UI
    state: Mutex<MeterState>,
}

struct MeterState {
    // Smoothed levels for LED display
    smoothed_left: f32,
    smoothed_right: f32,

    // Peak hold for dB text display
    peak_left: f32,
    peak_right: f32,
    peak_hold_time: f32,
}

impl MeterEngine {
    pub fn new(input_left: Arc<AtomicF32>, input_right: Arc<AtomicF32>) -> Self {
        Self {
            input_left,
            input_right,
            state: Mutex::new(MeterState {
                smoothed_left: -100.0,
                smoothed_right: -100.0,
                peak_left: -100.0,
                peak_right: -100.0,
                peak_hold_time: 0.0,
            }),
        }
    }

    /// Update all meter processing (should be called per frame)
    pub fn update(&self) {
        let left_db = self.input_left.load(Ordering::Relaxed);
        let right_db = self.input_right.load(Ordering::Relaxed);

        let mut state = self.state.lock().unwrap();

        // Update smoothed levels for LEDs
        self.update_smoothing(&mut state, left_db, right_db);

        // Update peak hold for dB display
        self.update_peak_hold(&mut state, left_db, right_db);
    }

    fn update_smoothing(&self, state: &mut MeterState, left_db: f32, right_db: f32) {
        let attack = constants::METER_ATTACK;
        let release = constants::METER_RELEASE;

        // Left channel smoothing
        if left_db > state.smoothed_left {
            state.smoothed_left = left_db * attack + state.smoothed_left * (1.0 - attack);
        } else {
            state.smoothed_left = left_db * release + state.smoothed_left * (1.0 - release);
        }

        // Right channel smoothing
        if right_db > state.smoothed_right {
            state.smoothed_right = right_db * attack + state.smoothed_right * (1.0 - attack);
        } else {
            state.smoothed_right = right_db * release + state.smoothed_right * (1.0 - release);
        }
    }

    fn update_peak_hold(&self, state: &mut MeterState, left_db: f32, right_db: f32) {
        let current_peak = left_db.max(right_db);

        // Update peaks if current is higher
        if left_db > state.peak_left {
            state.peak_left = left_db;
            state.peak_hold_time = 0.5;
        }
        if right_db > state.peak_right {
            state.peak_right = right_db;
            state.peak_hold_time = 0.5;
        }

        // Handle peak decay
        if current_peak > -80.0 {
            // Audio is present, reset hold timer
            state.peak_hold_time = 0.5;
        } else if state.peak_hold_time > 0.0 {
            // Audio stopped, count down
            state.peak_hold_time -= 1.0 / 60.0;
        } else {
            // Silent - follow smoothed levels
            state.peak_left = state.smoothed_left;
            state.peak_right = state.smoothed_right;
        }
    }

    /// Get smoothed levels for LED display
    pub fn get_smoothed_levels(&self) -> (f32, f32) {
        let state = self.state.lock().unwrap();
        (state.smoothed_left, state.smoothed_right)
    }

    /// Get peak hold value for dB text display
    pub fn get_peak_hold_db(&self) -> f32 {
        let state = self.state.lock().unwrap();
        state.peak_left.max(state.peak_right)
    }
}
