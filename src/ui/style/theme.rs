use nih_plug_iced::{border, color, widget::container::Style, Color, Theme};

/// colors and UI dimensions only
/// Audio-related constants are in audio::constants
pub struct UITheme;

impl UITheme {
    // === COLORS ===

    /// Background colors
    pub const BACKGROUND_MAIN: Color = Color::from_rgb(0.114, 0.114, 0.114);

    /// Text and label colors
    pub const TEXT_SECONDARY: Color = Color::from_rgba(0.6, 0.6, 0.6, 0.8);

    // === DIMENSIONS ===
    pub const METER_WIDTH: f32 = 40.0;

    /// Margins and padding
    pub const PADDING_SMALL: f32 = 5.0;

    // === VISUAL HELPER FUNCTIONS ===
    pub fn background_dark(_theme: &Theme) -> Style {
        Style {
            background: Some(color!(0x1D1D1D).into()),
            border: border::rounded(2),
            ..Style::default()
        }
    }
}
