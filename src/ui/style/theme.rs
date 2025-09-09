use nih_plug_iced::{border, color, widget::container::Style, Color, Theme};

/// Pro-Q inspired visual theme - colors and UI dimensions only
/// Audio-related constants are in audio::constants
pub struct UITheme;

impl UITheme {
    // === COLORS ===

    /// Background colors (Pro-Q dark theme)
    pub const BACKGROUND_MAIN: Color = Color::from_rgb(0.08, 0.08, 0.12);

    /// Grid and border colors
    pub const GRID_LINE: Color = Color::from_rgba(0.3, 0.3, 0.4, 0.3);

    /// Spectrum analyzer colors
    pub const SPECTRUM_LINE: Color = Color::from_rgb(0.3, 1.0, 0.8); // Cyan curve
    pub const SPECTRUM_FILL: Color = Color::from_rgba(0.3, 1.0, 0.8, 0.15); // Semi-transparent fill

    /// Level meter colors (Pro-Q yellow gradient)
    pub const METER_BACKGROUND: Color = Color::from_rgba(0.1, 0.1, 0.12, 0.8);

    /// Text and label colors
    pub const TEXT_SECONDARY: Color = Color::from_rgba(0.6, 0.6, 0.6, 0.8);

    // === DIMENSIONS ===
    pub const METER_WIDTH: f32 = 60.0;

    /// Margins and padding
    pub const PADDING_SMALL: u16 = 5;

    pub const SPECTRUM_MARGIN_BOTTOM: f32 = 30.0; // Space for frequency labels
    pub const SPECTRUM_MARGIN_RIGHT: f32 = 0.0; // No right margin - use full canvas width

    /// Grid and labels
    pub const GRID_LINE_WIDTH: f32 = 0.5;
    pub const TICK_MARK_WIDTH: f32 = 1.0;
    pub const TICK_MARK_LENGTH: f32 = 5.0;

    pub const FREQUENCY_LABEL_HEIGHT: f32 = 15.0;

    // === VISUAL HELPER FUNCTIONS ===
    pub fn background_dark(_theme: &Theme) -> Style {
        Style {
            background: Some(color!(0x14141F).into()),
            border: border::rounded(2),
            ..Style::default()
        }
    }
}
