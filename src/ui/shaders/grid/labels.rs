use crate::audio::constants;
use crate::ui::UITheme;
use nih_plug_iced::widget::canvas::{Frame, Geometry, Path, Program, Text};
use nih_plug_iced::{mouse, Font, Point, Rectangle, Renderer, Size, Theme};

/// Label overlay for the shader grid - draws text labels only
/// This renders on top of the shader grid using canvas text rendering
pub struct GridLabels;

impl GridLabels {
    pub fn new() -> Self {
        Self
    }
}

impl<Message> Program<Message, Theme> for GridLabels {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        _cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        // Draw frequency labels (bottom)
        self.draw_frequency_labels(&mut frame, bounds.size());

        // Draw dB scale labels (right side)
        self.draw_db_labels(&mut frame, bounds.size());

        vec![frame.into_geometry()]
    }
}

impl GridLabels {
    /// Draw frequency labels at the bottom
    fn draw_frequency_labels(&self, frame: &mut Frame, size: Size) {
        let spectrum_width = size.width - UITheme::SPECTRUM_MARGIN_RIGHT;

        self.draw_labels(
            frame,
            constants::FREQUENCY_MARKERS,
            UITheme::TEXT_SECONDARY,
            nih_plug_iced::Pixels(9.0),
            |&(freq, _)| {
                let log_pos = constants::freq_to_log_position(freq);
                let spectrum_height = size.height - UITheme::SPECTRUM_MARGIN_BOTTOM;
                (log_pos * spectrum_width, spectrum_height + 10.0) // Just below the spectrum area
            },
            nih_plug_iced::alignment::Horizontal::Left,
            nih_plug_iced::alignment::Vertical::Top,
        );
    }

    /// Draw dB scale labels on the right side
    fn draw_db_labels(&self, frame: &mut Frame, size: Size) {
        let spectrum_height = size.height - UITheme::SPECTRUM_MARGIN_BOTTOM;

        self.draw_labels(
            frame,
            constants::DB_MARKERS,
            UITheme::TEXT_DB_MARKER,
            nih_plug_iced::Pixels(10.0),
            |&(db_value, _)| {
                let normalized = constants::db_to_normalized(db_value);
                let y = spectrum_height * (1.0 - normalized);
                // Clamp Y position to keep text within visible area
                let clamped_y = y.max(5.0).min(spectrum_height - 5.0);
                (size.width - 5.0, clamped_y)
            },
            nih_plug_iced::alignment::Horizontal::Right,
            nih_plug_iced::alignment::Vertical::Center,
        );
    }

    /// Generic function to draw text labels
    fn draw_labels(
        &self,
        frame: &mut Frame,
        markers: &[(f32, &str)],
        text_color: nih_plug_iced::Color,
        text_size: nih_plug_iced::Pixels,
        text_position: impl Fn(&(f32, &str)) -> (f32, f32),
        h_align: nih_plug_iced::alignment::Horizontal,
        v_align: nih_plug_iced::alignment::Vertical,
    ) {
        // Draw text labels only
        for &marker in markers {
            let (x, y) = text_position(&marker);
            let text = Text {
                content: marker.1.to_string(),
                position: Point::new(x, y),
                color: text_color,
                size: text_size,
                font: Font::default(),
                align_x: h_align.into(),
                align_y: v_align.into(),
                line_height: nih_plug_iced::widget::text::LineHeight::default(),
                shaping: nih_plug_iced::widget::text::Shaping::default(),
                max_width: f32::INFINITY,
            };

            frame.fill_text(text);
        }
    }
}

impl Default for GridLabels {
    fn default() -> Self {
        Self::new()
    }
}
