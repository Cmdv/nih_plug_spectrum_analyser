use nih_plug_iced::{border, color, widget::container::Style, Color, Theme};

/// colors and UI dimensions only
/// Audio-related constants are in audio::constants
pub struct UITheme;

impl UITheme {
    // === COLORS ===

    /// Background colors
    pub const BACKGROUND_MAIN: Color = Color::from_rgb(0.114, 0.114, 0.114);

    /// Grid and border colors
    pub const GRID_LINE: Color = Color::from_rgba(0.3, 0.3, 0.4, 0.3);
    pub const GRID_LINE_LIGHT: Color = Color::from_rgba(0.25, 0.25, 0.3, 0.15);

    /// Spectrum analyser colors
    pub const SPECTRUM_LINE: Color = Color::from_rgb(0.3, 1.0, 0.8); // Cyan curve
    pub const SPECTRUM_FILL: Color = Color::from_rgba(0.3, 1.0, 0.8, 0.15); // Semi-transparent fill

    /// Level meter colors
    pub const METER_BACKGROUND: Color = Color::from_rgba(0.1, 0.1, 0.12, 0.8);

    /// Text and label colors
    pub const TEXT_SECONDARY: Color = Color::from_rgba(0.6, 0.6, 0.6, 0.8);
    pub const TEXT_DB_MARKER: Color = Color::from_rgb(1.0, 1.0, 0.6); // Yellow for dB labels

    // === DIMENSIONS ===
    pub const METER_WIDTH: f32 = 40.0;

    /// Margins and padding
    pub const PADDING_SMALL: f32 = 5.0;

    pub const SPECTRUM_MARGIN_BOTTOM: f32 = 30.0; // Space for frequency labels
    pub const SPECTRUM_MARGIN_RIGHT: f32 = 30.0; // Space for dB labels on right side

    /// Grid and labels
    pub const GRID_LINE_WIDTH: f32 = 0.5;

    // === VISUAL HELPER FUNCTIONS ===
    pub fn background_dark(_theme: &Theme) -> Style {
        Style {
            background: Some(color!(0x14141F).into()),
            border: border::rounded(2),
            ..Style::default()
        }
    }
}
