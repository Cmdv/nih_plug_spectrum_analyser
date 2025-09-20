use crate::audio::constants;
use crate::audio::spectrum::{SpectrumConsumer, SpectrumData};
use crate::ui::UITheme;
use atomic_float::AtomicF32;
use nih_plug_iced::widget::canvas::{self, Frame, Geometry, Path, Program, Stroke};
use nih_plug_iced::{mouse, Point, Rectangle, Renderer, Size, Theme};
use std::sync::{atomic::Ordering, Arc, Mutex};
use std::time::{Duration, Instant};

/// Spectrum display component with natural decay behavior
pub struct SpectrumDisplay {
    /// Communication channel from audio thread
    spectrum_output: SpectrumConsumer,
    /// Sample rate for frequency calculation
    sample_rate: Arc<AtomicF32>,
    /// Thread-safe mutable decay state
    decay_state: Arc<Mutex<DecayState>>,
}

/// Internal state for managing spectrum decay behavior
struct DecayState {
    /// Last known spectrum data
    last_spectrum: Option<SpectrumData>,
    /// When spectrum data was last updated
    last_update_time: Option<Instant>,
}

impl SpectrumDisplay {
    pub fn new(spectrum_output: SpectrumConsumer, sample_rate: Arc<AtomicF32>) -> Self {
        Self {
            spectrum_output,
            sample_rate,
            decay_state: Arc::new(Mutex::new(DecayState {
                last_spectrum: None,
                last_update_time: None,
            })),
        }
    }

    /// Apply exponential decay toward the spectrum floor over time
    fn apply_decay(&self, spectrum: &SpectrumData, time_delta: Duration) -> SpectrumData {
        // Faster, more natural decay - 1.5 seconds to reach floor
        const RELEASE_TIME_SECONDS: f32 = 1.0;
        const SPECTRUM_FLOOR_DB: f32 = -120.0; // Lower floor, similar to meter behavior

        let decay_factor = (-time_delta.as_secs_f32() / RELEASE_TIME_SECONDS).exp();

        spectrum
            .iter()
            .map(|&value| {
                let decayed = value * decay_factor + SPECTRUM_FLOOR_DB * (1.0 - decay_factor);
                decayed.max(SPECTRUM_FLOOR_DB)
            })
            .collect::<Vec<_>>()
            .try_into()
            .unwrap_or(*spectrum)
    }

    /// Get spectrum data for display, applying natural decay when values decrease
    fn get_display_spectrum(&self) -> SpectrumData {
        let current_spectrum = self.spectrum_output.read_or_silence();
        let now = Instant::now();

        // Try to get the lock, fall back to current spectrum if we can't
        let mut state = match self.decay_state.try_lock() {
            Ok(state) => state,
            Err(_) => return current_spectrum,
        };

        // If this is the first spectrum or values are increasing, update immediately
        if let Some(last) = state.last_spectrum {
            // Check if spectrum is increasing (new audio) or decreasing (needs decay)
            let is_increasing = current_spectrum
                .iter()
                .zip(last.iter())
                .any(|(current, last)| current > last);

            if is_increasing {
                // New audio data - update immediately
                state.last_spectrum = Some(current_spectrum);
                state.last_update_time = Some(now);
                current_spectrum
            } else {
                // Values decreasing - apply smooth decay
                let time_delta = state.last_update_time
                    .map(|t| now.duration_since(t))
                    .unwrap_or(Duration::ZERO);

                let decayed = self.apply_decay(&last, time_delta);

                // Take the maximum of decayed and current (in case current is higher)
                let result: SpectrumData = decayed
                    .iter()
                    .zip(current_spectrum.iter())
                    .map(|(&d, &c)| d.max(c))
                    .collect::<Vec<_>>()
                    .try_into()
                    .unwrap_or(current_spectrum);

                state.last_spectrum = Some(result);
                result
            }
        } else {
            // First spectrum data
            state.last_spectrum = Some(current_spectrum);
            state.last_update_time = Some(now);
            current_spectrum
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

        // Get spectrum data with decay applied when plugin is disabled
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
        base_smoothing: f32,
        start_with_move: bool,
    ) {
        if points.len() < 2 {
            return;
        }

        if start_with_move {
            path_builder.move_to(points[0]);
        }

        let catmull_rom_segments = generate_catmull_rom_segments(points, base_smoothing);
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
        calculate_single_display_point(bins, sample_rate, size, i, num_points)
    }

    fn draw_spectrum(&self, frame: &mut Frame, size: Size, spectrum_data: &SpectrumData) {
        // Convert to Vec for compatibility with existing display code
        let smoothed_copy: Vec<f32> = spectrum_data.to_vec();

        if smoothed_copy.len() < 2 {
            return;
        }

        // Calculate points with logarithmic frequency scaling
        let num_points = 256; // Optimal for smooth curves

        // Collect all points and shift them down by 5 pixels
        let mut points = Vec::with_capacity(num_points);
        for i in 0..num_points {
            let mut point =
                self.calculate_spectrum_point_for_display(i, num_points, &smoothed_copy, size);
            // Shift all points down by 1 pixels - this pushes the floor line below the visible area
            point.y += 1.0;
            points.push(point);
        }

        if points.len() < 3 {
            return;
        }

        // Create smooth curves using the helper method
        let mut path_builder = canvas::path::Builder::new();
        let smoothing = 0.5;
        Self::add_smooth_curves_to_path(&mut path_builder, &points, smoothing, true);

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

        // Add smooth spectrum curve using the helper method
        Self::add_smooth_curves_to_path(&mut fill_builder, &points, smoothing, false);

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

    // Use same width calculation as grid overlay for alignment
    let spectrum_width = size.width - UITheme::SPECTRUM_MARGIN_RIGHT;

    let x = (point_index as f32 / total_points as f32) * spectrum_width;
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
/// Applies stronger smoothing to high frequencies for cleaner visual appearance,
/// matching professional spectrum analysers, that prioritize
/// visual clarity over raw detail in the high-frequency display.
pub fn calculate_adaptive_smoothing(index: usize, total_points: usize, base_smoothing: f32) -> f32 {
    let progress = index as f32 / total_points as f32;
    if progress < 0.3 {
        // Low frequencies (20Hz - ~2kHz): normal smoothing
        base_smoothing
    } else if progress < 0.7 {
        // Mid frequencies (~2kHz - ~8kHz): increased smoothing
        base_smoothing * 1.3
    } else {
        // High frequencies (>8kHz): moderate smoothing to avoid overshooting
        base_smoothing * 2.0
    }
}

/// Generate Catmull-Rom spline segments for natural curve interpolation
///
/// Catmull-Rom splines pass through all control points, providing smoother
/// interpolation for noisy data like high-frequency spectrum without overshooting.
/// Each segment is represented as a cubic curve with computed control points.
/// Adaptive smoothing provides more aggressive smoothing for high frequencies.
pub fn generate_catmull_rom_segments(
    points: &[Point],
    base_smoothing: f32,
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

        // Apply adaptive smoothing - more smoothing for high frequencies
        let adaptive_smoothing = calculate_adaptive_smoothing(i, points.len(), base_smoothing);
        // Clamp tension to prevent overshooting (0.16 = standard Catmull-Rom)
        let tension = (1.0_f32 / 6.0).min(1.0 / (6.0 * adaptive_smoothing.max(0.5)));

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
