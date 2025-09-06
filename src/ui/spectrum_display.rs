use crate::audio::constants;
use crate::audio::spectrum_engine::SpectrumEngine;
use crate::ui::UITheme;
use nih_plug_iced::widget::canvas::{self, Frame, Geometry, Path, Program, Stroke};
use nih_plug_iced::{mouse, Point, Rectangle, Renderer, Size, Theme};
use std::sync::{Arc, RwLock};

pub struct SpectrumDisplay {
    // Spectrum processing engine (handles all audio logic)
    spectrum_engine: Arc<std::sync::Mutex<SpectrumEngine>>,
}

impl SpectrumDisplay {
    pub fn new(frequency_bins: Arc<RwLock<Vec<f32>>>) -> Self {
        let spectrum_engine = Arc::new(std::sync::Mutex::new(SpectrumEngine::new(frequency_bins)));
        Self { spectrum_engine }
    }
}

impl<Message> Program<Message, Theme> for SpectrumDisplay {
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
        frame.fill(&background, UITheme::BACKGROUND_MAIN);

        // Draw grid
        self.draw_grid(&mut frame, bounds.size());

        // Draw spectrum curve (with margins for labels)
        let spectrum_area = Size::new(
            bounds.width - UITheme::SPECTRUM_MARGIN_RIGHT,
            bounds.height - UITheme::SPECTRUM_MARGIN_BOTTOM,
        );
        self.draw_spectrum(&mut frame, spectrum_area);

        // Draw frequency labels (bottom)
        self.draw_frequency_labels(&mut frame, bounds.size());

        // Draw dB scale labels overlaid on spectrum (right edge)
        self.draw_db_labels(&mut frame, bounds.size());

        vec![frame.into_geometry()]
    }
}

impl SpectrumDisplay {
    /// Create smooth Bézier curves from a set of points with adaptive smoothing
    fn add_smooth_curves_to_path(
        path_builder: &mut canvas::path::Builder,
        points: &[Point],
        base_smoothing: f32,
        start_with_move: bool,
    ) {
        if points.len() < 2 {
            return;
        }

        if start_with_move {
            path_builder.move_to(points[0]);
        }

        for i in 1..points.len() - 1 {
            let current = points[i];
            let next = points[i + 1];

            // Adaptive smoothing: more in low frequencies (left side), less in high frequencies (right side)
            let progress = i as f32 / points.len() as f32;
            let adaptive_smoothing = if progress < 0.3 {
                // Low frequencies (20Hz - ~2kHz): maximum smoothing
                base_smoothing * 1.5
            } else if progress < 0.7 {
                // Mid frequencies (~2kHz - ~8kHz): normal smoothing
                base_smoothing
            } else {
                // High frequencies (>8kHz): minimal smoothing for detail
                base_smoothing * 0.4
            };

            // Calculate control points for smooth curve
            let control_point1 = Point::new(
                current.x + (next.x - current.x) * adaptive_smoothing,
                current.y,
            );
            let control_point2 =
                Point::new(next.x - (next.x - current.x) * adaptive_smoothing, next.y);

            path_builder.bezier_curve_to(control_point1, control_point2, next);
        }
    }

    /// Calculate display point using SpectrumEngine processing
    fn calculate_spectrum_point_for_display(
        &self,
        i: usize,
        num_points: usize,
        bins: &[f32],
        size: Size,
    ) -> Point {
        // Use engine's calculation method for consistency
        let (_freq, db_value) = SpectrumEngine::calculate_spectrum_point(bins, i, num_points);

        // Map dB range to screen coordinates using audio constants
        let normalised = constants::db_to_normalized(db_value);

        let x = (i as f32 / num_points as f32) * size.width;
        let y = size.height * (1.0 - normalised);

        Point::new(x, y)
    }

    fn draw_grid(&self, frame: &mut Frame, size: Size) {
        let stroke = Stroke::default()
            .with_width(UITheme::GRID_LINE_WIDTH)
            .with_color(UITheme::GRID_LINE);

        // Calculate the spectrum area (same as used for spectrum drawing)
        let spectrum_width = size.width - UITheme::SPECTRUM_MARGIN_RIGHT;
        let spectrum_height = size.height - UITheme::SPECTRUM_MARGIN_BOTTOM;

        // Horizontal grid lines for dB levels (every 10dB from 0 to -100)
        for i in 0..=10 {
            let db = -(i as f32 * 10.0);
            let normalized = constants::db_to_normalized(db);
            let y = spectrum_height * (1.0 - normalized);
            // Grid lines span only the spectrum area
            let path = Path::line(Point::new(0.0, y), Point::new(spectrum_width, y));
            frame.stroke(&path, stroke.clone());
        }

        // Vertical grid lines for frequency markers (logarithmic)
        for &(freq, _label) in constants::FREQUENCY_MARKERS {
            // Use the same logarithmic positioning as labels
            let log_pos = constants::freq_to_log_position(freq);
            let x = log_pos * spectrum_width;

            // Grid lines span only the spectrum area
            let path = Path::line(Point::new(x, 0.0), Point::new(x, spectrum_height));
            frame.stroke(&path, stroke.clone());
        }
    }

    fn draw_spectrum(&self, frame: &mut Frame, size: Size) {
        // Update spectrum processing engine
        if let Ok(mut engine) = self.spectrum_engine.lock() {
            engine.update();
        }

        // Get processed spectrum data
        let smoothed_copy = if let Ok(engine) = self.spectrum_engine.lock() {
            engine.get_spectrum_data()
        } else {
            return; // Lock failed, skip this frame
        };

        if smoothed_copy.len() < 2 {
            return;
        }

        // Create spectrum path with smooth curves
        let mut path_builder = canvas::path::Builder::new();

        // Calculate points with logarithmic frequency scaling
        let num_points = 768; // Optimal for smooth curves - fewer points = smoother
        let mut points = Vec::with_capacity(num_points);

        // Collect all points first
        for i in 0..num_points {
            let point =
                self.calculate_spectrum_point_for_display(i, num_points, &smoothed_copy, size);
            points.push(point);
        }

        if points.len() < 3 {
            return;
        }

        // Create smooth curves using the helper method
        let smoothing = 0.3; // Adjust this for curve smoothness (0.1-0.3)
        Self::add_smooth_curves_to_path(&mut path_builder, &points, smoothing, true);

        let spectrum_path = path_builder.build();

        // Draw the line
        let line_stroke = Stroke::default()
            .with_width(UITheme::GRID_LINE_WIDTH)
            .with_color(UITheme::SPECTRUM_LINE);
        frame.stroke(&spectrum_path, line_stroke);

        // Create fill path (closed polygon) with same smooth curves
        let mut fill_builder = canvas::path::Builder::new();

        // Start at bottom left
        fill_builder.move_to(Point::new(0.0, size.height));

        // Add first point
        fill_builder.line_to(points[0]);

        // Add smooth spectrum curve using the helper method (skip move_to since we already positioned)
        Self::add_smooth_curves_to_path(&mut fill_builder, &points, smoothing, false);

        // Close at bottom right
        fill_builder.line_to(Point::new(size.width, size.height));
        fill_builder.close();

        let fill_path = fill_builder.build();

        // Fill with semi-transparent color
        frame.fill(&fill_path, UITheme::SPECTRUM_FILL);
    }

    /// Draw frequency labels at the bottom
    fn draw_frequency_labels(&self, frame: &mut Frame, size: Size) {
        let label_y = size.height - UITheme::FREQUENCY_LABEL_HEIGHT;
        let spectrum_width = size.width - UITheme::SPECTRUM_MARGIN_RIGHT;

        for &(freq, _label) in constants::FREQUENCY_MARKERS {
            // Convert frequency to logarithmic position using audio constants
            let log_pos = constants::freq_to_log_position(freq);
            let x = log_pos * spectrum_width;

            // Draw tick mark
            let tick_start = Point::new(x, label_y - UITheme::TICK_MARK_LENGTH);
            let tick_end = Point::new(x, label_y - 3.0);
            let tick_path = Path::line(tick_start, tick_end);

            let tick_stroke = Stroke::default()
                .with_width(UITheme::TICK_MARK_WIDTH)
                .with_color(UITheme::TEXT_SECONDARY);
            frame.stroke(&tick_path, tick_stroke);

            // TODO: Add text labels when text rendering becomes available
            // For now, just the tick marks provide visual reference
        }
    }

    /// Draw dB scale labels on the right side (Pro-Q style)
    fn draw_db_labels(&self, frame: &mut Frame, size: Size) {
        let spectrum_height = size.height - UITheme::SPECTRUM_MARGIN_BOTTOM;

        // Draw tick marks at the very right edge (no margin)
        let tick_start_x = size.width - UITheme::TICK_MARK_LENGTH;
        let tick_end_x = size.width;

        for &(db, _label) in constants::DB_MARKERS {
            // Convert dB to y position using audio constants
            let normalized = constants::db_to_normalized(db);
            let y = spectrum_height * (1.0 - normalized);

            // Draw tick mark at right edge
            let tick_start = Point::new(tick_start_x, y);
            let tick_end = Point::new(tick_end_x, y);
            let tick_path = Path::line(tick_start, tick_end);

            let tick_stroke = Stroke::default()
                .with_width(UITheme::TICK_MARK_WIDTH)
                .with_color(UITheme::TEXT_SECONDARY);
            frame.stroke(&tick_path, tick_stroke);

            // TODO: Add text labels when text rendering becomes available
            // For now, just the tick marks provide visual reference
        }

        // Draw "dB" indicator dot at bottom right corner
        let db_label_pos = Point::new(size.width - 8.0, spectrum_height + 15.0);
        let db_indicator = Path::circle(db_label_pos, 2.0);
        frame.fill(&db_indicator, UITheme::TEXT_SECONDARY);
    }
}
