use crate::audio::constants;
use crate::audio::spectrum_analyzer::SpectrumOutput;
use crate::ui::UITheme;
use atomic_float::AtomicF32;
use nih_plug_iced::widget::canvas::{self, Frame, Geometry, Path, Program, Stroke};
use nih_plug_iced::{mouse, Point, Rectangle, Renderer, Size, Theme};
use std::sync::{atomic::Ordering, Arc};

/// Spectrum display component - no processing logic
/// Reads spectrum data from SpectrumOutput communication channel
pub struct SpectrumDisplay {
    /// Communication channel from audio thread
    spectrum_output: SpectrumOutput,

    /// Sample rate for frequency calculation
    sample_rate: Arc<AtomicF32>,
}

impl SpectrumDisplay {
    pub fn new(spectrum_output: SpectrumOutput, sample_rate: Arc<AtomicF32>) -> Self {
        Self {
            spectrum_output,
            sample_rate,
        }
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

        let bezier_segments = generate_bezier_segments(points, base_smoothing);
        for (control1, control2, end_point) in bezier_segments {
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
        calculate_single_display_point(bins, sample_rate, size, i, num_points)
    }

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

        // Draw vertical grid lines using pure function
        let frequency_grid_lines = generate_frequency_grid_lines(spectrum_width, spectrum_height);
        for grid_line in frequency_grid_lines {
            let path = Path::line(grid_line.start, grid_line.end);
            frame.stroke(&path, stroke.clone());
        }
    }

    fn draw_spectrum(&self, frame: &mut Frame, size: Size) {
        // READ ONLY - Get latest spectrum data from audio thread
        let spectrum_data = self.spectrum_output.read();

        // Convert to Vec for compatibility with existing display code
        let smoothed_copy: Vec<f32> = spectrum_data.to_vec();
        
        // Debug: Find peak in the data we're displaying
        let (peak_idx, peak_val) = smoothed_copy
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap())
            .map(|(i, &v)| (i, v))
            .unwrap_or((0, -120.0));
        
        static mut UI_FRAME_COUNT: u32 = 0;
        unsafe {
            UI_FRAME_COUNT += 1;
            if UI_FRAME_COUNT % 60 == 0 {
                let sample_rate = self.sample_rate.load(Ordering::Relaxed);
                // Fix: Should be sample_rate / window_size, not divided by 2 again
                let peak_freq = (peak_idx as f32 * sample_rate) / 2048.0;
                nih_plug::nih_log!("UI received peak: bin {} @ {:.0}Hz = {:.1}dB", 
                    peak_idx, peak_freq, peak_val);
            }
        }

        if smoothed_copy.len() < 2 {
            return;
        }

        // Create spectrum path with smooth curves
        let mut path_builder = canvas::path::Builder::new();

        // Calculate points with logarithmic frequency scaling
        let num_points = 768; // Optimal for smooth curves - fewer points = smoother
        let mut points = Vec::with_capacity(num_points);

        // Debug: Log what we're drawing
        static mut DRAW_COUNTER: u32 = 0;
        unsafe {
            DRAW_COUNTER += 1;
            if DRAW_COUNTER % 120 == 0 {
                nih_plug::nih_log!("Display: Drawing {} points from {} bins", num_points, smoothed_copy.len());
            }
        }

        // Collect all points first
        for i in 0..num_points {
            let point =
                self.calculate_spectrum_point_for_display(i, num_points, &smoothed_copy, size);
            points.push(point);
            
            // Log display points including around 1kHz
            unsafe {
                if DRAW_COUNTER % 120 == 0 {
                    let freq = calculate_log_frequency(i, num_points);
                    if i < 10 || (freq > 900.0 && freq < 1100.0) {
                        let db_val = interpolate_bin_value(&smoothed_copy, freq, 48000.0);
                        nih_plug::nih_log!("  Point {}: freq={:.0}Hz, db={:.1}dB, y={:.1}", i, freq, db_val, point.y);
                    }
                }
            }
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

        let tick_stroke = Stroke::default()
            .with_width(UITheme::TICK_MARK_WIDTH)
            .with_color(UITheme::TEXT_SECONDARY);

        // Generate and draw tick marks using pure function
        let tick_marks = generate_frequency_tick_marks(spectrum_width, label_y);
        for tick_mark in tick_marks {
            let tick_path = Path::line(tick_mark.start, tick_mark.end);
            frame.stroke(&tick_path, tick_stroke.clone());
        }

        // TODO: Add text labels when text rendering becomes available
        // For now, just the tick marks provide visual reference
    }

    /// Draw dB scale labels on the right side (Pro-Q style)
    fn draw_db_labels(&self, frame: &mut Frame, size: Size) {
        let spectrum_height = size.height - UITheme::SPECTRUM_MARGIN_BOTTOM;
        let tick_start_x = size.width - UITheme::TICK_MARK_LENGTH;
        let tick_end_x = size.width;

        let tick_stroke = Stroke::default()
            .with_width(UITheme::TICK_MARK_WIDTH)
            .with_color(UITheme::TEXT_SECONDARY);

        // Generate and draw tick marks using pure function
        let tick_marks = generate_db_tick_marks(spectrum_height, tick_start_x, tick_end_x);
        for tick_mark in tick_marks {
            let tick_path = Path::line(tick_mark.start, tick_mark.end);
            frame.stroke(&tick_path, tick_stroke.clone());
        }

        // TODO: Add text labels when text rendering becomes available
        // For now, just the tick marks provide visual reference

        // Draw "dB" indicator dot at bottom right corner
        let db_label_pos = Point::new(size.width - 8.0, spectrum_height + 15.0);
        let db_indicator = Path::circle(db_label_pos, 2.0);
        frame.fill(&db_indicator, UITheme::TEXT_SECONDARY);
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

    if bin_index + 1 < bins.len() {
        // Linear interpolation between two bins
        let current_bin = bins[bin_index];
        let next_bin = bins[bin_index + 1];
        current_bin + (next_bin - current_bin) * bin_fraction
    } else if bin_index < bins.len() {
        bins[bin_index]
    } else {
        -100.0 // Out of range
    }
}

/// Map dB value to screen coordinates
///
/// Converts dB magnitude to pixel coordinates using the standard spectrum display mapping.
/// Applies A-weighting for perceptually accurate frequency response visualization.
pub fn map_to_screen_coordinates(
    db_value: f32,
    _frequency: f32,
    size: Size,
    point_index: usize,
    total_points: usize,
) -> Point {
    // Apply A-weighting for perceptual accuracy
    // let weighted_db = apply_a_weighting(frequency, db_value);

    // Map dB range to screen coordinates
    let normalized = constants::db_to_normalized(db_value);

    let x = (point_index as f32 / total_points as f32) * size.width;
    let y = size.height * (1.0 - normalized);

    Point::new(x, y)
}

/// Calculate a single display point with complete frequency mapping pipeline
///
/// Combines frequency calculation, bin interpolation, and screen mapping into a single
/// composable function. This is the main pure function that replaces the original method.
pub fn calculate_single_display_point(
    bins: &[f32],
    sample_rate: f32,
    size: Size,
    point_index: usize,
    total_points: usize,
) -> Point {
    let frequency = calculate_log_frequency(point_index, total_points);
    let db_value = interpolate_bin_value(bins, frequency, sample_rate);
    map_to_screen_coordinates(db_value, frequency, size, point_index, total_points)
}

/// Calculate adaptive smoothing factor based on frequency position
///
/// Applies stronger smoothing to low frequencies (left side of spectrum) and
/// lighter smoothing to high frequencies for better detail preservation.
/// Matches the frequency characteristics of professional spectrum analyzers.
pub fn calculate_adaptive_smoothing(index: usize, total_points: usize, base_smoothing: f32) -> f32 {
    let progress = index as f32 / total_points as f32;
    if progress < 0.3 {
        // Low frequencies (20Hz - ~2kHz): maximum smoothing
        base_smoothing * 1.5
    } else if progress < 0.7 {
        // Mid frequencies (~2kHz - ~8kHz): normal smoothing
        base_smoothing
    } else {
        // High frequencies (>8kHz): minimal smoothing for detail
        base_smoothing * 0.4
    }
}

/// Create Bézier control points for smooth curve segment
///
/// Generates control points for a cubic Bézier curve between two points.
/// Control points are positioned to create smooth transitions while preserving
/// the overall curve shape and frequency response characteristics.
pub fn create_bezier_control_points(
    current_point: Point,
    next_point: Point,
    smoothing_factor: f32,
) -> (Point, Point) {
    let control_point1 = Point::new(
        current_point.x + (next_point.x - current_point.x) * smoothing_factor,
        current_point.y,
    );
    let control_point2 = Point::new(
        next_point.x - (next_point.x - current_point.x) * smoothing_factor,
        next_point.y,
    );
    (control_point1, control_point2)
}

/// Generate Bézier curve segments from a series of points
///
/// Creates smooth curve segments with adaptive smoothing based on frequency position.
/// Returns control point pairs that can be used to draw smooth Bézier curves.
/// Each segment connects adjacent points with appropriate smoothing.
pub fn generate_bezier_segments(
    points: &[Point],
    base_smoothing: f32,
) -> Vec<(Point, Point, Point)> {
    if points.len() < 2 {
        return Vec::new();
    }

    points
        .windows(2)
        .enumerate()
        .map(|(i, window)| {
            let current = window[0];
            let next = window[1];
            let adaptive_smoothing =
                calculate_adaptive_smoothing(i + 1, points.len(), base_smoothing);
            let (control1, control2) =
                create_bezier_control_points(current, next, adaptive_smoothing);
            (control1, control2, next)
        })
        .collect()
}

/// Grid line data for spectrum display
pub struct GridLine {
    pub start: Point,
    pub end: Point,
}

/// Generate horizontal grid lines for dB levels
pub fn generate_db_grid_lines(spectrum_width: f32, spectrum_height: f32) -> Vec<GridLine> {
    (0..=10)
        .map(|i| {
            let db = -(i as f32 * 10.0);
            let normalized = constants::db_to_normalized(db);
            let y = spectrum_height * (1.0 - normalized);
            GridLine {
                start: Point::new(0.0, y),
                end: Point::new(spectrum_width, y),
            }
        })
        .collect()
}

/// Generate vertical grid lines for frequency markers
pub fn generate_frequency_grid_lines(spectrum_width: f32, spectrum_height: f32) -> Vec<GridLine> {
    constants::FREQUENCY_MARKERS
        .iter()
        .map(|&(freq, _label)| {
            let log_pos = constants::freq_to_log_position(freq);
            let x = log_pos * spectrum_width;
            GridLine {
                start: Point::new(x, 0.0),
                end: Point::new(x, spectrum_height),
            }
        })
        .collect()
}

/// Tick mark data for labels
pub struct TickMark {
    pub start: Point,
    pub end: Point,
}

/// Generate frequency tick marks
pub fn generate_frequency_tick_marks(spectrum_width: f32, label_y: f32) -> Vec<TickMark> {
    constants::FREQUENCY_MARKERS
        .iter()
        .map(|&(freq, _label)| {
            let log_pos = constants::freq_to_log_position(freq);
            let x = log_pos * spectrum_width;
            TickMark {
                start: Point::new(x, label_y - UITheme::TICK_MARK_LENGTH),
                end: Point::new(x, label_y - 3.0),
            }
        })
        .collect()
}

/// Generate dB tick marks
pub fn generate_db_tick_marks(
    spectrum_height: f32,
    tick_start_x: f32,
    tick_end_x: f32,
) -> Vec<TickMark> {
    constants::DB_MARKERS
        .iter()
        .map(|&(db, _label)| {
            let normalized = constants::db_to_normalized(db);
            let y = spectrum_height * (1.0 - normalized);
            TickMark {
                start: Point::new(tick_start_x, y),
                end: Point::new(tick_end_x, y),
            }
        })
        .collect()
}
