use crate::audio::constants;
use crate::ui::UITheme;
use nih_plug_iced::widget::canvas::{Frame, Geometry, Path, Program, Stroke, Text};
use nih_plug_iced::{mouse, Font, Point, Rectangle, Renderer, Size, Theme};

/// Grid overlay component - draws static grid lines and labels
/// No data processing, just visual grid elements
pub struct GridOverlay;

impl GridOverlay {
    pub fn new() -> Self {
        Self
    }
}

impl<Message> Program<Message, Theme> for GridOverlay {
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

        // Draw grid
        self.draw_grid(&mut frame, bounds.size());

        // Draw frequency labels (bottom)
        self.draw_frequency_labels(&mut frame, bounds.size());

        // Draw dB scale labels (right side)
        self.draw_db_labels(&mut frame, bounds.size());

        vec![frame.into_geometry()]
    }
}

impl GridOverlay {
    fn draw_grid(&self, frame: &mut Frame, size: Size) {
        let stroke = Stroke::default()
            .with_width(UITheme::GRID_LINE_WIDTH)
            .with_color(UITheme::GRID_LINE);

        // Calculate the spectrum area (same as used for spectrum drawing)
        let spectrum_width = size.width - UITheme::SPECTRUM_MARGIN_RIGHT;
        let spectrum_height = size.height - UITheme::SPECTRUM_MARGIN_BOTTOM;

        // Draw horizontal grid lines using pure function
        let db_grid_lines = generate_db_grid_lines(spectrum_width, spectrum_height);
        for grid_line in db_grid_lines {
            let path = Path::line(grid_line.start, grid_line.end);
            frame.stroke(&path, stroke.clone());
        }

        // Draw vertical grid lines using pure function with different weights
        let frequency_grid_lines =
            generate_frequency_grid_lines_with_weights(spectrum_width, spectrum_height);
        for (grid_line, is_major) in frequency_grid_lines {
            let path = Path::line(grid_line.start, grid_line.end);
            if is_major {
                // Major lines (100Hz, 1kHz, 10kHz) - normal color
                frame.stroke(&path, stroke.clone());
            } else {
                // Minor lines - lighter color
                let light_stroke = Stroke::default()
                    .with_width(UITheme::GRID_LINE_WIDTH)
                    .with_color(UITheme::GRID_LINE_LIGHT);
                frame.stroke(&path, light_stroke);
            }
        }
    }

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
            nih_plug_iced::alignment::Horizontal::Left, // Align to right of position
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

/// Grid line data for spectrum display
pub struct GridLine {
    pub start: Point,
    pub end: Point,
}

/// Generate horizontal grid lines for dB levels
pub fn generate_db_grid_lines(spectrum_width: f32, spectrum_height: f32) -> Vec<GridLine> {
    constants::DB_MARKERS
        .iter()
        .map(|&(db, _)| {
            let normalized = constants::db_to_normalized(db);
            let y = spectrum_height * (1.0 - normalized);
            GridLine {
                start: Point::new(0.0, y),
                end: Point::new(spectrum_width, y),
            }
        })
        .collect()
}

pub fn generate_frequency_grid_lines_with_weights(
    spectrum_width: f32,
    spectrum_height: f32,
) -> Vec<(GridLine, bool)> {
    let frequency_positions = constants::generate_frequency_grid_positions();
    frequency_positions
        .iter()
        .map(|&(freq, is_major)| {
            let log_pos = constants::freq_to_log_position(freq);
            let x = log_pos * spectrum_width;
            let grid_line = GridLine {
                start: Point::new(x, 0.0),
                end: Point::new(x, spectrum_height),
            };
            (grid_line, is_major)
        })
        .collect()
}
