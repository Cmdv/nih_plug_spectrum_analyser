use nih_plug_iced::widget::canvas::{self, Frame, Geometry, Path, Program, Stroke};
use nih_plug_iced::{mouse, Color, Point, Rectangle, Renderer, Size, Theme};
use std::sync::{Arc, Mutex, RwLock};

pub struct SpectrumView {
    // Frequency data from FFT (shared between threads)
    pub frequency_bins: Arc<RwLock<Vec<f32>>>,
    // Smoothed bins for visual smoothing (thread-safe for draw method)
    smoothed_bins: Mutex<Vec<f32>>,
}

impl SpectrumView {
    pub fn new(frequency_bins: Arc<RwLock<Vec<f32>>>) -> Self {
        Self {
            frequency_bins,
            smoothed_bins: Mutex::new(Vec::new()),
        }
    }
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
    /// Apply A-weighting to frequency response for perceptual accuracy
    /// Based on IEC 61672-1:2013 standard
    fn apply_a_weighting(freq_hz: f32, db_value: f32) -> f32 {
        if freq_hz <= 0.0 {
            return db_value - 50.0; // Heavily attenuate invalid frequencies
        }
        
        let f = freq_hz as f64;
        let f2 = f * f;
        let f4 = f2 * f2;
        
        // A-weighting formula (IEC 61672-1 standard)
        let numerator = 12194.0_f64.powi(2) * f4;
        let denominator = (f2 + 20.6_f64.powi(2)) * 
                         (f2 + 12194.0_f64.powi(2)) *
                         (f2 + 107.7_f64.powi(2)).sqrt() *
                         (f2 + 737.9_f64.powi(2)).sqrt();
        
        if denominator == 0.0 {
            return db_value - 50.0;
        }
        
        let ra = numerator / denominator;
        let a_weighting_db = 20.0 * ra.log10() + 2.00; // +2dB normalization
        
        db_value + a_weighting_db as f32
    }

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

    /// Calculate a spectrum point with logarithmic frequency scaling
    fn calculate_spectrum_point(
        &self,
        i: usize,
        num_points: usize,
        bins: &[f32],
        size: Size,
    ) -> Point {
        // Logarithmic frequency mapping (like Pro-Q 3)
        // Map 20Hz to 20kHz logarithmically across the display
        let min_freq = 20.0;
        let max_freq = 20000.0;
        let nyquist = 22050.0; // Half of 44.1kHz sample rate

        // Calculate the frequency for this display point (logarithmic)
        let norm_pos = i as f32 / num_points as f32;
        let freq = min_freq * (max_freq / min_freq as f32).powf(norm_pos);

        // Convert frequency to bin index with interpolation
        let bin_position = (freq / nyquist) * bins.len() as f32;
        let bin_index = bin_position.floor() as usize;
        let bin_fraction = bin_position.fract(); // For interpolation

        // Get interpolated dB value
        let raw_db_value = if bin_index + 1 < bins.len() {
            // Linear interpolation between two bins
            let current_bin = bins[bin_index];
            let next_bin = bins[bin_index + 1];
            current_bin + (next_bin - current_bin) * bin_fraction
        } else if bin_index < bins.len() {
            bins[bin_index]
        } else {
            -100.0
        };

        // Apply A-weighting for perceptual accuracy (like Pro-Q 3)
        let db_value = Self::apply_a_weighting(freq, raw_db_value);

        // Map dB range to screen coordinates
        let normalised = ((db_value + 90.0) / 110.0).max(0.0).min(1.0);

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

        // Apply smoothing with attack/release
        let mut smoothed = self.smoothed_bins.lock().unwrap();

        // Initialize smoothed_bins if needed
        if smoothed.len() != bins.len() {
            *smoothed = vec![-100.0; bins.len()];
        }

        // Apply smoothing with different attack/release times
        let attack = 0.9; // Fast attack (higher = faster)
        let release = 0.02; // Slow release (lower = slower)
        for i in 0..bins.len() {
            if bins[i] > smoothed[i] {
                // Attack: follow quickly when louder
                smoothed[i] = bins[i] * attack + smoothed[i] * (1.0 - attack);
            } else {
                // Release: decay slowly when quieter
                smoothed[i] = bins[i] * release + smoothed[i] * (1.0 - release);
            }
        }

        // Create a copy to use for drawing (to avoid holding the borrow)
        let smoothed_copy = smoothed.clone();

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

        // Create spectrum path with smooth curves
        let mut path_builder = canvas::path::Builder::new();

        // Calculate points with logarithmic frequency scaling
        let num_points = 768; // Optimal for smooth curves - fewer points = smoother
        let mut points = Vec::with_capacity(num_points);

        // Collect all points first
        for i in 0..num_points {
            let point = self.calculate_spectrum_point(i, num_points, &smoothed_copy, size);
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
            .with_width(0.5)
            .with_color(Color::from_rgb(0.3, 1.0, 0.8));
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
        frame.fill(&fill_path, Color::from_rgba(0.3, 1.0, 0.8, 0.15));
    }
}
