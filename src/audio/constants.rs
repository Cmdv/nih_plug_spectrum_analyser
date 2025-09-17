/// Shared audio processing constants
/// Only constants used across multiple modules are kept here

// === SHARED FREQUENCY RANGE ===
/// Frequency range for analysis and display (20Hz - 20kHz)
pub const MIN_FREQUENCY: f32 = 20.0;
pub const MAX_FREQUENCY: f32 = 20000.0;

// === SHARED DISPLAY RANGE ===
/// dB range for spectrum display (-100 to 0 dB)
pub const MAX_DB: f32 = 0.0;
pub const MIN_DB: f32 = -100.0;
pub const DB_RANGE: f32 = MAX_DB - MIN_DB; // 100dB total range

// === SHARED DISPLAY FUNCTIONS ===

/// Convert frequency to logarithmic display position (0.0 to 1.0)
/// Used by spectrum display and frequency-based UI components
pub fn freq_to_log_position(freq: f32) -> f32 {
    (freq / MIN_FREQUENCY).log10() / (MAX_FREQUENCY / MIN_FREQUENCY).log10()
}

/// Convert dB to normalized display position (0.0 = MIN_DB, 1.0 = MAX_DB)
/// Used by spectrum and meter displays  
pub fn db_to_normalized(db: f32) -> f32 {
    ((db - MIN_DB) / DB_RANGE).max(0.0).min(1.0)
}

/// Standard frequency markers for grid
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

/// Standard dB markers for grid
pub const DB_MARKERS: &[(f32, &str)] = &[
    (0.0, "0"),
    (-20.0, "-20"),
    (-40.0, "-40"),
    (-60.0, "-60"),
    (-80.0, "-80"),
    (-100.0, "-100"),
];

/// Generate frequency grid lines algorithmically
/// Creates a professional-looking logarithmic frequency grid
/// Returns (frequency, is_major) tuples
pub fn generate_frequency_grid_positions() -> Vec<(f32, bool)> {
    let mut frequencies = Vec::new();

    // Major lines at decade boundaries: 100Hz, 1kHz, 10kHz
    let major_frequencies = &[100.0, 1000.0, 10000.0];

    // Lines at every step within each decade
    // 20-100Hz: every 10Hz (20, 30, 40, 50, 60, 70, 80, 90, 100)
    for i in 2..=10 {
        let freq = i as f32 * 10.0;
        let is_major = major_frequencies.contains(&freq);
        frequencies.push((freq, is_major));
    }

    // 100-1000Hz: every 100Hz (200, 300, 400, 500, 600, 700, 800, 900, 1000)
    for i in 2..=10 {
        let freq = i as f32 * 100.0;
        let is_major = major_frequencies.contains(&freq);
        frequencies.push((freq, is_major));
    }

    // 1000-10000Hz: every 1000Hz (2k, 3k, 4k, 5k, 6k, 7k, 8k, 9k, 10k)
    for i in 2..=10 {
        let freq = i as f32 * 1000.0;
        let is_major = major_frequencies.contains(&freq);
        frequencies.push((freq, is_major));
    }

    // Add 20kHz endpoint
    frequencies.push((20000.0, false));

    frequencies
}
