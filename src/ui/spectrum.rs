use nih_plug_iced::widget::canvas::{Frame, Geometry, Path, Program, Stroke};
use nih_plug_iced::{mouse, Color, Point, Rectangle, Renderer, Theme};
use std::sync::Arc;

pub struct SpectrumView {
    // Frequency data from FFT (shared between threads)
    pub frequency_bins: Arc<Vec<f32>>,

    // Size of the widget
    pub width: f32,
    pub height: f32,
}

impl Program<(), Theme> for SpectrumView {
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

        // Fill background to verify Canvas is working
        frame.fill_rectangle(
            Point::ORIGIN,
            bounds.size(),
            Color::from_rgb(0.1, 0.1, 0.2), // Dark blue background
        );

        // Draw a thick, bright line across the middle
        let start = Point::new(10.0, bounds.height / 2.0);
        let end = Point::new(bounds.width - 10.0, bounds.height / 2.0);

        let path = Path::line(start, end);
        frame.stroke(
            &path, 
            Stroke::default()
                .with_color(Color::from_rgb(0.0, 1.0, 0.5)) // Bright green
                .with_width(5.0) // Thick line
        );

        vec![frame.into_geometry()]
    }
}
