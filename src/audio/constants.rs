/// Audio processing constants and helper functions
/// Separated from visual theme for better organization

/// Frequency range constants
pub const MIN_FREQUENCY: f32 = 20.0;
pub const MAX_FREQUENCY: f32 = 20000.0;
pub const NYQUIST_FREQUENCY: f32 = 22050.0;

/// dB range for spectrum display
pub const MAX_DB: f32 = 20.0;
pub const MIN_DB: f32 = -100.0;
pub const DB_RANGE: f32 = MAX_DB - MIN_DB; // 120dB total range

/// Gain parameter range
pub const GAIN_MIN_DB: f32 = -30.0;
pub const GAIN_MAX_DB: f32 = 30.0;
pub const GAIN_RANGE_DB: f32 = GAIN_MAX_DB - GAIN_MIN_DB; // 60dB range

/// Level meter range (Pro-Q style)
pub const METER_MAX_DB: f32 = 12.0;
pub const METER_MIN_DB: f32 = -60.0;
pub const METER_RANGE_DB: f32 = METER_MAX_DB - METER_MIN_DB; // 72dB range

/// Smoothing factors for level meters (Pro-Q style)
pub const METER_ATTACK: f32 = 0.8; // Fast attack
pub const METER_RELEASE: f32 = 0.02; // Slow release

/// Smoothing factors for spectrum display
pub const SPECTRUM_ATTACK: f32 = 0.9; // Very fast attack
pub const SPECTRUM_RELEASE: f32 = 0.02; // Very slow release

/// Knob rotation range (Pro-Q style: 300 degrees total rotation)
pub const KNOB_MIN_ANGLE_DEG: f32 = -150.0;
pub const KNOB_MAX_ANGLE_DEG: f32 = 150.0;
pub const KNOB_TOTAL_ROTATION_DEG: f32 = 300.0;

pub const WAVEFORM_BUFFER_SIZE: usize = 4096;
pub const MAX_BLOCK_SIZE: usize = 8192;

// === HELPER FUNCTIONS ===

/// Convert frequency to logarithmic position (0.0 to 1.0)
pub fn freq_to_log_position(freq: f32) -> f32 {
    (freq / MIN_FREQUENCY).log10() / (MAX_FREQUENCY / MIN_FREQUENCY).log10()
}

/// Convert dB to normalized position (0.0 = MIN_DB, 1.0 = MAX_DB)  
pub fn db_to_normalized(db: f32) -> f32 {
    ((db - MIN_DB) / DB_RANGE).max(0.0).min(1.0)
}

/// Convert normalized position back to dB
pub fn normalized_to_db(normalized: f32) -> f32 {
    MIN_DB + (normalized * DB_RANGE)
}

/// Convert gain dB to normalized knob position (0.0 to 1.0)
pub fn gain_db_to_normalized(gain_db: f32) -> f32 {
    (gain_db - GAIN_MIN_DB) / GAIN_RANGE_DB
}

/// Convert normalized knob position to gain dB
pub fn normalized_to_gain_db(normalized: f32) -> f32 {
    GAIN_MIN_DB + (normalized * GAIN_RANGE_DB)
}

/// Convert knob angle to gain dB
pub fn knob_angle_to_gain_db(angle_deg: f32) -> f32 {
    // Map angle from -150° to +150° → 0.0 to 1.0 → gain dB range
    let normalized = (angle_deg - KNOB_MIN_ANGLE_DEG) / KNOB_TOTAL_ROTATION_DEG;
    normalized_to_gain_db(normalized)
}

/// Convert gain dB to knob angle
pub fn gain_db_to_knob_angle(gain_db: f32) -> f32 {
    let normalized = gain_db_to_normalized(gain_db);
    KNOB_MIN_ANGLE_DEG + (normalized * KNOB_TOTAL_ROTATION_DEG)
}

/// Standard frequency markers for grid (Pro-Q style)
pub const FREQUENCY_MARKERS: &[(f32, &str)] = &[
    (20.0, "20"),
    (50.0, "50"),
    (100.0, "100"),
    (200.0, "200"),
    (500.0, "500"),
    (1000.0, "1K"),
    (2000.0, "2K"),
    (5000.0, "5K"),
    (10000.0, "10K"),
    (20000.0, "20K"),
];

/// Standard dB markers for grid (Pro-Q style)
pub const DB_MARKERS: &[(f32, &str)] = &[
    (20.0, "+20"),
    (0.0, "0"),
    (-20.0, "-20"),
    (-40.0, "-40"),
    (-60.0, "-60"),
    (-80.0, "-80"),
    (-100.0, "-100"),
];
