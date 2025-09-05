use crate::audio::constants;
use crate::ui::AudioTheme;
use atomic_float::AtomicF32;
use nih_plug_iced::widget::canvas::{Frame, Geometry, Path, Program, Stroke};
use nih_plug_iced::{mouse, Color, Point, Rectangle, Renderer, Size, Theme};
use std::sync::{atomic::Ordering, Arc};

pub struct LevelMeter {
    // Post-gain output levels (dB) from audio thread via AtomicF32
    pub peak_level_left: Arc<AtomicF32>,
    pub peak_level_right: Arc<AtomicF32>,
    // Internal smoothed levels for display
    smoothed_levels: std::sync::Mutex<(f32, f32)>,
}

impl LevelMeter {
    pub fn new(peak_level_left: Arc<AtomicF32>, peak_level_right: Arc<AtomicF32>) -> Self {
        Self {
            peak_level_left,
            peak_level_right,
            smoothed_levels: std::sync::Mutex::new((-100.0, -100.0)),
        }
    }
}

impl<Message> Program<Message, Theme> for LevelMeter {
    type State = ();
    
    fn draw(
        &self, 
        _state: &Self::State, 
        renderer: &Renderer, 
        _theme: &Theme, 
        bounds: Rectangle, 
        _cursor: mouse::Cursor
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());
        
        // Draw meter background
        self.draw_meter_background(&mut frame, bounds.size());
        
        // Draw level bars with gradient (Pro-Q style)
        self.draw_level_bars(&mut frame, bounds.size());
        
        // Draw dB scale labels
        self.draw_db_scale(&mut frame, bounds.size());
        
        vec![frame.into_geometry()]
    }
}

impl LevelMeter {
    fn draw_meter_background(&self, frame: &mut Frame, size: Size) {
        // Dark background similar to Pro-Q
        let background = Path::rectangle(Point::ORIGIN, size);
        frame.fill(&background, Color::from_rgb(0.06, 0.06, 0.08));
        
        // Subtle border
        let border_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(Color::from_rgba(0.3, 0.3, 0.3, 0.5));
        frame.stroke(&background, border_stroke);
    }
    
    fn draw_level_bars(&self, frame: &mut Frame, size: Size) {
        // Get current levels from atomic values (no locking needed)
        let left_db = self.peak_level_left.load(Ordering::Relaxed);
        let right_db = self.peak_level_right.load(Ordering::Relaxed);
        
        // Apply smoothing (attack/release like Pro-Q meters)
        let mut smoothed = self.smoothed_levels.lock().unwrap();
        let attack = constants::METER_ATTACK;  // Fast attack
        let release = constants::METER_RELEASE; // Slow release
        
        if left_db > smoothed.0 {
            smoothed.0 = left_db * attack + smoothed.0 * (1.0 - attack);
        } else {
            smoothed.0 = left_db * release + smoothed.0 * (1.0 - release);
        }
        
        if right_db > smoothed.1 {
            smoothed.1 = right_db * attack + smoothed.1 * (1.0 - attack);
        } else {
            smoothed.1 = right_db * release + smoothed.1 * (1.0 - release);
        }
        
        let (smooth_left, smooth_right) = *smoothed;
        drop(smoothed);
        
        // Draw level bars (like Pro-Q's yellow meter)
        let bar_width = size.width * 0.6;
        let bar_spacing = size.width * 0.1;
        let bar_height = size.height - 40.0; // Leave space for labels
        
        // Left channel bar
        self.draw_single_level_bar(
            frame, 
            Point::new(bar_spacing, 20.0),
            Size::new(bar_width * 0.4, bar_height),
            smooth_left
        );
        
        // Right channel bar  
        self.draw_single_level_bar(
            frame,
            Point::new(bar_spacing + bar_width * 0.6, 20.0),
            Size::new(bar_width * 0.4, bar_height),
            smooth_right
        );
    }
    
    fn draw_single_level_bar(&self, frame: &mut Frame, position: Point, size: Size, level_db: f32) {
        // Convert dB to 0-1 range using constants
        let normalized_level = ((level_db - constants::METER_MIN_DB) / constants::METER_RANGE_DB).max(0.0).min(1.0);
        
        // Draw background bar (dark)
        let bg_path = Path::rectangle(position, size);
        frame.fill(&bg_path, AudioTheme::METER_BACKGROUND);
        
        if normalized_level > 0.0 {
            // Draw filled level with Pro-Q style gradient
            let fill_height = size.height * normalized_level;
            let fill_y = position.y + size.height - fill_height;
            
            let fill_path = Path::rectangle(
                Point::new(position.x, fill_y),
                Size::new(size.width, fill_height)
            );
            
            // Use theme color gradient
            let color = AudioTheme::get_meter_color(normalized_level);
            
            frame.fill(&fill_path, color);
        }
        
        // Draw subtle border around bar
        let border_stroke = Stroke::default()
            .with_width(0.5)
            .with_color(Color::from_rgba(0.4, 0.4, 0.4, 0.6));
        frame.stroke(&bg_path, border_stroke);
    }
    
    fn draw_db_scale(&self, frame: &mut Frame, size: Size) {
        // Pro-Q style dB markings on the right side
        // Font size for future text rendering
        // let _font_size = 8.0;
        let text_color = Color::from_rgba(0.7, 0.7, 0.7, 0.8);
        
        // dB markers: +12, 0, -12, -24, -48, -60
        let db_marks = [12.0, 0.0, -12.0, -24.0, -48.0, -60.0];
        let max_db = 12.0;
        let min_db = -60.0;
        let bar_height = size.height - 40.0;
        
        for &db in &db_marks {
            let normalized = (db - min_db) / (max_db - min_db);
            let y = 20.0 + bar_height * (1.0 - normalized);
            
            // Draw tick mark
            let tick_start = Point::new(size.width - 15.0, y);
            let tick_end = Point::new(size.width - 5.0, y);
            let tick_path = Path::line(tick_start, tick_end);
            
            let tick_stroke = Stroke::default()
                .with_width(1.0)
                .with_color(text_color);
            frame.stroke(&tick_path, tick_stroke);
            
            // TODO: Add text labels (requires text rendering in canvas)
            // For now, just the tick marks provide visual reference
        }
        
        // Draw "dB" label at bottom
        // TODO: Add "OUT" or "dB" text label when text rendering is available
    }
}