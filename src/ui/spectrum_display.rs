use crate::audio::spectrum::{SpectrumConsumer, SpectrumData};
use crate::ui::UITheme;
use crate::{ResolutionLevel, SAPluginParams};
use atomic_float::AtomicF32;
use nih_plug_iced::widget::canvas::{self, Frame, Geometry, Path, Program, Stroke};
use nih_plug_iced::{mouse, Point, Rectangle, Renderer, Size, Theme};
use std::sync::{atomic::Ordering, Arc};

/// Spectrum display component
pub struct SpectrumDisplay {
    /// Communication channel from audio thread
    spectrum_output: SpectrumConsumer,
    /// Sample rate for frequency calculation
    sample_rate: Arc<AtomicF32>,
    /// Plugin parameters for accessing amplitude range and resolution
    plugin_params: Arc<SAPluginParams>,
}

impl SpectrumDisplay {
    pub fn new(
        spectrum_output: SpectrumConsumer,
        sample_rate: Arc<AtomicF32>,
        plugin_params: Arc<SAPluginParams>,
    ) -> Self {
        Self {
            spectrum_output,
            sample_rate,
            plugin_params,
        }
    }

    /// Get spectrum data for display - just read final processed data from audio thread
    fn get_display_spectrum(&self) -> SpectrumData {
        self.spectrum_output.read_or_silence()
    }

    /// Convert dB to normalized position based on current amplitude range
    fn db_to_normalized(&self, db: f32) -> f32 {
        let (min_db, max_db) = self.plugin_params.range.value().to_db_range();
        let db_range = max_db - min_db;
        ((db - min_db) / db_range).max(0.0).min(1.0)
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

        // Get final processed spectrum data from audio thread
        let spectrum_data = self.get_display_spectrum();

        // Draw spectrum curve using processed data
        self.draw_spectrum(&mut frame, bounds.size(), &spectrum_data);

        vec![frame.into_geometry()]
    }
}

impl SpectrumDisplay {
    /// Create smooth curves from a set of points using Catmull-Rom splines
    ///
    /// Catmull-Rom splines provide better interpolation for noisy spectrum data
    /// as they pass through all control points without the overshooting artifacts
    /// that can occur with Bézier curves at high smoothing factors.
    fn add_smooth_curves_to_path(
        path_builder: &mut canvas::path::Builder,
        points: &[Point],
        resolution: ResolutionLevel,
        start_with_move: bool,
    ) {
        if points.len() < 2 {
            return;
        }

        if start_with_move {
            path_builder.move_to(points[0]);
        }

        let catmull_rom_segments = generate_catmull_rom_segments(points, resolution);
        for (control1, control2, end_point) in catmull_rom_segments {
            path_builder.bezier_curve_to(control1, control2, end_point);
        }
    }

    /// Calculate display point with logarithmic frequency scaling and A-weighting
    fn calculate_spectrum_point_for_display(
        &self,
        i: usize,
        num_points: usize,
        bins: &[f32],
        size: Size,
    ) -> Point {
        let sample_rate = self.sample_rate.load(Ordering::Relaxed);
        let frequency = calculate_log_frequency(i, num_points);
        let db_value = interpolate_bin_value(bins, frequency, sample_rate);

        // Use our instance method that respects the amplitude range
        self.map_to_screen_coordinates(db_value, frequency, size, i, num_points)
    }

    /// Maps dB value and frequency to screen coordinates with proper scaling.
    fn map_to_screen_coordinates(
        &self,
        db_value: f32,
        _frequency: f32,
        size: Size,
        point_index: usize,
        total_points: usize,
    ) -> Point {
        // Map dB range to screen coordinates using current amplitude range
        let normalized = self.db_to_normalized(db_value);

        // Use same width calculation as grid overlay for alignment
        let spectrum_width = size.width - UITheme::SPECTRUM_MARGIN_RIGHT;

        let x = (point_index as f32 / total_points as f32) * spectrum_width;
        let y = size.height * (1.0 - normalized);

        Point::new(x, y)
    }

    fn draw_spectrum(&self, frame: &mut Frame, size: Size, spectrum_data: &SpectrumData) {
        // Use the actual spectrum data - already sized correctly based on resolution
        if spectrum_data.len() < 2 {
            return;
        }

        // Use actual bin count from the spectrum data
        let num_points = spectrum_data.len();

        // Collect all points and shift them down by 5 pixels
        let mut points = Vec::with_capacity(num_points);
        for i in 0..num_points {
            let mut point =
                self.calculate_spectrum_point_for_display(i, num_points, spectrum_data, size);
            // Shift all points down by 1 pixels - this pushes the floor line below the visible area
            point.y += 1.0;
            points.push(point);
        }

        if points.len() < 3 {
            return;
        }

        // Create smooth curves using resolution-based smoothing
        let mut path_builder = canvas::path::Builder::new();
        let resolution = self.plugin_params.resolution.value();
        Self::add_smooth_curves_to_path(&mut path_builder, &points, resolution, true);

        let spectrum_path = path_builder.build();

        // Draw the line
        let line_stroke = Stroke::default()
            .with_width(UITheme::GRID_LINE_WIDTH)
            .with_color(UITheme::SPECTRUM_LINE);
        frame.stroke(&spectrum_path, line_stroke);

        // Create fill path (closed polygon) with same smooth curves
        let mut fill_builder = canvas::path::Builder::new();

        // Use same width calculation as spectrum points for X-axis alignment
        let spectrum_width = size.width - UITheme::SPECTRUM_MARGIN_RIGHT;

        // Start at bottom left (shifted down to hide floor line)
        fill_builder.move_to(Point::new(0.0, size.height + 5.0));

        // Add first point
        fill_builder.line_to(points[0]);

        // Add smooth spectrum curve using resolution-based smoothing
        Self::add_smooth_curves_to_path(&mut fill_builder, &points, resolution, false);

        // Close at bottom right (shifted down to hide floor line)
        fill_builder.line_to(Point::new(spectrum_width, size.height + 5.0));
        fill_builder.close();

        let fill_path = fill_builder.build();

        // Fill with semi-transparent color
        frame.fill(&fill_path, UITheme::SPECTRUM_FILL);
    }
}

/// Calculate logarithmic frequency for a display point index
///
/// Maps point indices to frequencies using logarithmic scaling for musical perception.
/// Lower indices represent lower frequencies, following the standard 20Hz-20kHz range.
pub fn calculate_log_frequency(point_index: usize, total_points: usize) -> f32 {
    use crate::audio::constants;
    let min_freq = constants::MIN_FREQUENCY;
    let max_freq = constants::MAX_FREQUENCY;

    let norm_pos = point_index as f32 / total_points as f32;
    min_freq * (max_freq / min_freq).powf(norm_pos)
}

/// Interpolate magnitude value from FFT bins at a specific frequency
///
/// Uses linear interpolation between adjacent bins to provide smooth frequency response.
/// Handles edge cases where the frequency maps outside the available bin range.
pub fn interpolate_bin_value(bins: &[f32], frequency: f32, sample_rate: f32) -> f32 {
    let nyquist_frequency = sample_rate / 2.0;
    // Fix: bins.len() - 1 because indices go from 0 to len-1
    let bin_position = (frequency / nyquist_frequency) * (bins.len() - 1) as f32;
    let bin_index = bin_position.floor() as usize;
    let bin_fraction = bin_position.fract();

    let result = if bin_index + 1 < bins.len() {
        // Linear interpolation between two bins
        let current_bin = bins[bin_index];
        let next_bin = bins[bin_index + 1];
        current_bin + (next_bin - current_bin) * bin_fraction
    } else if bin_index < bins.len() {
        bins[bin_index]
    } else {
        -100.0 // Out of range
    };

    result
}

/// Generate Catmull-Rom spline segments for natural curve interpolation
///
/// Catmull-Rom splines pass through all control points, providing smoother
/// interpolation for noisy data like high-frequency spectrum without overshooting.
/// Each segment is represented as a cubic curve with computed control points.
/// Adaptive smoothing provides resolution-specific smoothing patterns.
pub fn generate_catmull_rom_segments(
    points: &[Point],
    resolution: ResolutionLevel,
) -> Vec<(Point, Point, Point)> {
    if points.len() < 4 {
        // Fall back to simple lines for short point sequences
        return points
            .windows(2)
            .map(|window| {
                let start = window[0];
                let end = window[1];
                // Simple linear control points
                let control1 = Point::new(
                    start.x + (end.x - start.x) * 0.33,
                    start.y + (end.y - start.y) * 0.33,
                );
                let control2 = Point::new(
                    start.x + (end.x - start.x) * 0.67,
                    start.y + (end.y - start.y) * 0.67,
                );
                (control1, control2, end)
            })
            .collect();
    }

    let mut segments = Vec::new();

    // Generate Catmull-Rom segments for interior points
    for i in 1..points.len() - 2 {
        let p0 = points[i - 1];
        let p1 = points[i];
        let p2 = points[i + 1];
        let p3 = points[i + 2];

        // Calculate tension based on resolution level and frequency position
        let progress = i as f32 / points.len() as f32;
        let base_tension = match resolution {
            ResolutionLevel::Low => 0.4,      // Large radius curves - very smooth
            ResolutionLevel::Medium => 0.25,  // Medium radius curves
            ResolutionLevel::High => 0.18,    // Smaller radius curves - more detailed
            ResolutionLevel::Maximum => 0.12, // Tight radius curves - most precise
        };

        // Apply frequency-aware scaling: larger curves for low frequencies, tighter for high frequencies
        let frequency_scale = if progress < 0.3 {
            4.0 // Low frequencies: much larger radius curves
        } else if progress < 0.7 {
            1.0 // Mid frequencies: normal radius
        } else {
            0.6 // High frequencies: tighter curves for detail
        };

        let raw_tension: f32 = base_tension * frequency_scale;
        let tension = raw_tension.min(0.5); // Clamp maximum tension

        // Catmull-Rom control point calculation with adaptive tension
        let control1 = Point::new(
            p1.x + (p2.x - p0.x) * tension,
            p1.y + (p2.y - p0.y) * tension,
        );
        let control2 = Point::new(
            p2.x - (p3.x - p1.x) * tension,
            p2.y - (p3.y - p1.y) * tension,
        );

        segments.push((control1, control2, p2));
    }

    segments
}
