use nih_plug_iced::{border, color, widget::container::Style, Color, Theme};

/// Pro-Q inspired visual theme - colors and UI dimensions only
/// Audio-related constants are in audio::constants
pub struct UITheme;

impl UITheme {
    // === COLORS ===

    /// Background colors (Pro-Q dark theme)
    pub const BACKGROUND_MAIN: Color = Color::from_rgb(0.08, 0.08, 0.12);
    pub const BACKGROUND_PANEL: Color = Color::from_rgb(0.06, 0.06, 0.08);
    pub const BACKGROUND_COMPONENT: Color = Color::from_rgb(0.12, 0.12, 0.15);
    pub const BACKGROUND_COMPONENT_INNER: Color = Color::from_rgb(0.18, 0.18, 0.22);

    /// Grid and border colors
    pub const GRID_LINE: Color = Color::from_rgba(0.3, 0.3, 0.4, 0.3);
    pub const BORDER_SUBTLE: Color = Color::from_rgba(0.4, 0.4, 0.4, 0.6);
    pub const BORDER_HOVER: Color = Color::from_rgba(0.9, 0.7, 0.2, 0.4);

    /// Spectrum analyzer colors
    pub const SPECTRUM_LINE: Color = Color::from_rgb(0.3, 1.0, 0.8); // Cyan curve
    pub const SPECTRUM_FILL: Color = Color::from_rgba(0.3, 1.0, 0.8, 0.15); // Semi-transparent fill

    /// Level meter colors (Pro-Q yellow gradient)
    pub const METER_GREEN: Color = Color::from_rgb(0.2, 0.9, 0.1); // Low levels
    pub const METER_YELLOW: Color = Color::from_rgb(0.9, 0.9, 0.1); // Mid levels
    pub const METER_ORANGE: Color = Color::from_rgb(0.9, 0.6, 0.1); // High levels
    pub const METER_RED: Color = Color::from_rgb(1.0, 0.2, 0.1); // Peak levels
    pub const METER_BACKGROUND: Color = Color::from_rgba(0.1, 0.1, 0.12, 0.8);

    /// Knob colors
    pub const KNOB_POINTER: Color = Color::from_rgb(0.9, 0.7, 0.2); // Golden yellow pointer
    pub const KNOB_VALUE_INDICATOR: Color = Color::from_rgb(0.9, 0.7, 0.2);
    pub const KNOB_CENTER_DOT: Color = Color::from_rgba(0.6, 0.6, 0.6, 0.8);

    /// Text and label colors
    pub const TEXT_PRIMARY: Color = Color::from_rgba(0.9, 0.9, 0.9, 0.9);
    pub const TEXT_SECONDARY: Color = Color::from_rgba(0.6, 0.6, 0.6, 0.8);
    pub const TEXT_MUTED: Color = Color::from_rgba(0.4, 0.4, 0.4, 0.7);

    // === DIMENSIONS ===

    /// Layout proportions (Pro-Q style)
    pub const SPECTRUM_WIDTH_RATIO: u16 = 4; // 80% of width
    pub const CONTROLS_WIDTH_RATIO: u16 = 1; // 20% of width

    /// Component sizes
    pub const KNOB_SIZE: f32 = 80.0;
    pub const KNOB_POINTER_WIDTH: f32 = 2.5;
    pub const KNOB_CENTER_DOT_RADIUS: f32 = 2.0;

    pub const METER_WIDTH: f32 = 60.0;
    pub const METER_BAR_WIDTH_RATIO: f32 = 0.4; // Each bar is 40% of meter width
    pub const METER_BAR_SPACING: f32 = 0.1; // 10% spacing between bars

    /// Margins and padding
    pub const PADDING_SMALL: u16 = 5;
    pub const PADDING_MEDIUM: u16 = 10;
    pub const PADDING_LARGE: u16 = 15;

    pub const SPECTRUM_MARGIN_BOTTOM: f32 = 30.0; // Space for frequency labels
    pub const SPECTRUM_MARGIN_RIGHT: f32 = 0.0; // No right margin - use full canvas width

    /// Grid and labels
    pub const GRID_LINE_WIDTH: f32 = 0.5;
    pub const TICK_MARK_WIDTH: f32 = 1.0;
    pub const TICK_MARK_LENGTH: f32 = 5.0;

    pub const FREQUENCY_LABEL_HEIGHT: f32 = 15.0;
    pub const DB_LABEL_WIDTH: f32 = 30.0;

    // === VISUAL HELPER FUNCTIONS ===
    pub fn background_dark(_theme: &Theme) -> Style {
        Style {
            background: Some(color!(0x14141F).into()),
            border: border::rounded(2),
            ..Style::default()
        }
    }

    /// Get level meter color based on level (Pro-Q style gradient)
    pub fn get_meter_color(normalized_level: f32) -> Color {
        if normalized_level < 0.7 {
            // Green to yellow (0-70%)
            let factor = normalized_level / 0.7;
            Color::from_rgb(
                factor * Self::METER_YELLOW.r + (1.0 - factor) * Self::METER_GREEN.r,
                Self::METER_GREEN.g, // Keep green high
                Self::METER_GREEN.b,
            )
        } else if normalized_level < 0.9 {
            // Yellow to orange (70-90%)
            let factor = (normalized_level - 0.7) / 0.2;
            Color::from_rgb(
                Self::METER_YELLOW.r,
                Self::METER_YELLOW.g * (1.0 - factor) + Self::METER_ORANGE.g * factor,
                Self::METER_YELLOW.b,
            )
        } else {
            // Red (90-100%)
            Self::METER_RED
        }
    }
}
