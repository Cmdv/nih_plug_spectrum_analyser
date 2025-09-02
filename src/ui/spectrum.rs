use iced::widget::canvas::{self, Canvas, Frame, Geometry, Path, Program};
use iced::{mouse, Color, Point, Rectangle, Renderer, Theme};
use std::sync::Arc;

pub struct SpectrumView {
    // Frequency data from FFT (shared between threads)
    pub frequency_bins: Arc<Vec<f32>>,

    // Size of the widget
    pub width: f32,
    pub height: f32,
}

impl Program<(), iced::Theme> for SpectrumView {
    type State = ();

    fn draw(
        &self,
        state: &Self::State,
        renderer: &Renderer,
        theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        // Create frame
        // Draw spectrum
        // Return geometry
        let mut frame = Frame::new(renderer, bounds.size());

        // For now, just draw a simple test line to verify it works
        let start = Point::new(0.0, bounds.height / 2.0);
        let end = Point::new(bounds.width, bounds.height / 2.0);

        let path = Path::line(start, end);
        frame.stroke(
            &path,
            iced::widget::canvas::Stroke::default().with_color(Color::WHITE),
        );

        // Return the geometry
        vec![frame.into_geometry()]
    }
}
