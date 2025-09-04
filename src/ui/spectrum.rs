use nih_plug::util;
use nih_plug_iced::widget::canvas::{self, Frame, Geometry, Path, Program, Stroke};
use nih_plug_iced::{mouse, Color, Point, Rectangle, Renderer, Size, Theme};
use std::sync::{Arc, RwLock};

pub struct SpectrumView {
    // Frequency data from FFT (shared between threads)
    pub frequency_bins: Arc<RwLock<Vec<f32>>>,
}

impl<Message> Program<Message, Theme> for SpectrumView {
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

        // Draw background
        let background = Path::rectangle(Point::ORIGIN, bounds.size());
        frame.fill(&background, Color::from_rgb(0.08, 0.08, 0.12));

        // Draw grid
        self.draw_grid(&mut frame, bounds.size());

        // Draw spectrum curve
        self.draw_spectrum(&mut frame, bounds.size());

        vec![frame.into_geometry()]
    }
}

impl SpectrumView {
    /// Calculate a spectrum point with logarithmic frequency scaling
    fn calculate_spectrum_point(
        &self,
        i: usize,
        num_points: usize,
        bins: &[f32],
        size: Size,
    ) -> Point {
        let freq_ratio = ((i as f32 / num_points as f32).powf(2.0) * bins.len() as f32) as usize;
        let magnitude = if freq_ratio < bins.len() {
            bins[freq_ratio]
        } else {
            0.0
        };

        let db = util::gain_to_db(magnitude);
        let normalised = ((db + 100.0) / 100.0).max(0.0).min(1.0); // -100dB to 0dB range

        let x = (i as f32 / num_points as f32) * size.width;
        let y = size.height * (1.0 - normalised);

        Point::new(x, y)
    }

    fn draw_grid(&self, frame: &mut Frame, size: Size) {
        let grid_color = Color::from_rgba(0.3, 0.3, 0.4, 0.3);
        let stroke = Stroke::default().with_width(0.5).with_color(grid_color);

        // Horizontal grid lines for dB levels (every 10dB from 0 to -100)
        for i in 0..=10 {
            let db = -(i as f32 * 10.0);
            let y = size.height * (1.0 - (db + 100.0) / 100.0);
            let path = Path::line(Point::new(0.0, y), Point::new(size.width, y));
            frame.stroke(&path, stroke.clone());
        }

        // Vertical grid lines for frequency markers (logarithmic)
        let frequencies = [
            50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0, 5000.0, 10000.0, 20000.0,
        ];
        let nyquist = 22050.0; // Half of 44.1kHz sample rate

        for freq in frequencies {
            let normalised_freq: f32 = freq / nyquist;
            let display_position = normalised_freq.sqrt(); // Inverse of powf(2.0)
            let x = display_position * size.width;

            let path = Path::line(Point::new(x, 0.0), Point::new(x, size.height));
            frame.stroke(&path, stroke.clone());
        }
    }

    fn draw_spectrum(&self, frame: &mut Frame, size: Size) {
        let bins = match self.frequency_bins.read() {
            Ok(data) => data,
            Err(_) => return, // Lock poisoned, skip this frame
        };

        if bins.len() < 2 {
            return;
        }

        // Log spectrum data occasionally
        static mut DRAW_LOG_COUNTER: u32 = 0;
        unsafe {
            DRAW_LOG_COUNTER += 1;
            if DRAW_LOG_COUNTER >= 600 {
                // Log every ~10 seconds at 60fps
                DRAW_LOG_COUNTER = 0;
                let max_val = bins.iter().take(100).fold(0.0f32, |a, &b| a.max(b));
                nih_plug::nih_log!("Drawing spectrum, max value in first 100 bins: {}", max_val);
            }
        }

        // Create spectrum path
        let mut path_builder = canvas::path::Builder::new();

        // Calculate points with logarithmic frequency scaling
        let num_points = bins.len().min(512);
        let mut first_point = true;

        for i in 0..num_points {
            let point = self.calculate_spectrum_point(i, num_points, &bins, size);

            if first_point {
                path_builder.move_to(point);
                first_point = false;
            } else {
                path_builder.line_to(point);
            }
        }

        let spectrum_path = path_builder.build();

        // Draw the line
        let line_stroke = Stroke::default()
            .with_width(2.0)
            .with_color(Color::from_rgb(0.3, 1.0, 0.8));
        frame.stroke(&spectrum_path, line_stroke);

        // Create fill path (closed polygon)
        let mut fill_builder = canvas::path::Builder::new();

        // Start at bottom left
        fill_builder.move_to(Point::new(0.0, size.height));

        // Add spectrum points
        for i in 0..num_points {
            let point = self.calculate_spectrum_point(i, num_points, &bins, size);
            fill_builder.line_to(point);
        }

        // Close at bottom right
        fill_builder.line_to(Point::new(size.width, size.height));
        fill_builder.close();

        let fill_path = fill_builder.build();

        // Fill with semi-transparent color
        frame.fill(&fill_path, Color::from_rgba(0.3, 1.0, 0.8, 0.15));
    }
}
