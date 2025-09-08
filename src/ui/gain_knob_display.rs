// No audio constants needed in this display component
use nih_plug::prelude::*;
use nih_plug_iced::widget::canvas::{Frame, Geometry, Path, Program, Stroke};
use nih_plug_iced::{mouse, Color, Point, Rectangle, Renderer, Size, Theme};
use std::sync::Arc;

pub struct GainKnobDisplay {
    pub params: Arc<crate::PluginLearnParams>,
    pub is_dragging: bool,
    pub drag_start_y: f32,
    pub drag_start_value: f32,
}

#[derive(Debug, Clone)]
pub enum KnobMessage {
    StartDrag { position: Point },
    Drag { position: Point },
    EndDrag,
}

impl GainKnobDisplay {
    pub fn new(params: Arc<crate::PluginLearnParams>) -> Self {
        Self {
            params,
            is_dragging: false,
            drag_start_y: 0.0,
            drag_start_value: 0.0,
        }
    }

    /// Get current gain value in dB
    fn get_gain_db(&self) -> f32 {
        util::gain_to_db(self.params.gain.value())
    }

    /// Set gain value from dB (placeholder for future drag implementation)
    #[allow(dead_code)]
    fn set_gain_db(&self, _db: f32, _context: &dyn GuiContext) {
        // TODO: Implement proper parameter setting when we add mouse interaction
        // This will be implemented when we create custom mouse event handling
    }
}

impl<Message> Program<Message, Theme> for GainKnobDisplay {
    type State = ();

    fn draw(
        &self,
        _state: &Self::State,
        renderer: &Renderer,
        _theme: &Theme,
        bounds: Rectangle,
        cursor: mouse::Cursor,
    ) -> Vec<Geometry> {
        let mut frame = Frame::new(renderer, bounds.size());

        // Draw knob background circle
        self.draw_knob_background(&mut frame, bounds.size());

        // Draw value indicator (pointer line)
        self.draw_value_indicator(&mut frame, bounds.size());

        // Draw center dot
        self.draw_center_dot(&mut frame, bounds.size());

        // Highlight on hover
        if cursor.is_over(bounds) {
            self.draw_hover_highlight(&mut frame, bounds.size());
        }

        vec![frame.into_geometry()]
    }
}

impl GainKnobDisplay {
    fn draw_knob_background(&self, frame: &mut Frame, size: Size) {
        let center = Point::new(size.width / 2.0, size.height / 2.0);
        let radius = (size.width.min(size.height) / 2.0) * 0.8;

        // Outer ring (Pro-Q style)
        let outer_circle = Path::circle(center, radius);
        frame.fill(&outer_circle, Color::from_rgb(0.12, 0.12, 0.15));

        // Inner knob body
        let inner_circle = Path::circle(center, radius * 0.85);
        frame.fill(&inner_circle, Color::from_rgb(0.18, 0.18, 0.22));

        // Subtle border
        let border_stroke = Stroke::default()
            .with_width(1.0)
            .with_color(Color::from_rgba(0.4, 0.4, 0.4, 0.6));
        frame.stroke(&outer_circle, border_stroke);
    }

    fn draw_value_indicator(&self, frame: &mut Frame, size: Size) {
        let center = Point::new(size.width / 2.0, size.height / 2.0);
        let radius = (size.width.min(size.height) / 2.0) * 0.6;

        // Convert gain dB to rotation angle (Pro-Q style: -150° to +150°, 300° total range)
        let gain_db = self.get_gain_db();
        let normalized = (gain_db + 30.0) / 60.0; // 0.0 to 1.0
        let angle = (normalized * 300.0 - 150.0).to_radians(); // -150° to +150°

        // Calculate pointer end position
        let pointer_end = Point::new(
            center.x + angle.sin() * radius,
            center.y - angle.cos() * radius,
        );

        // Draw pointer line (bright like Pro-Q indicators)
        let pointer_path = Path::line(center, pointer_end);
        let pointer_stroke = Stroke::default()
            .with_width(2.5)
            .with_color(Color::from_rgb(0.9, 0.7, 0.2)); // Golden yellow
        frame.stroke(&pointer_path, pointer_stroke);

        // Draw value text below knob
        self.draw_value_text(frame, size, center, gain_db);
    }

    fn draw_value_text(&self, frame: &mut Frame, size: Size, center: Point, gain_db: f32) {
        // TODO: Implement text rendering when available in canvas
        // For now, we'll add a visual indicator for the value

        // Draw a small indicator for the current value range
        let indicator_y = center.y + (size.height / 2.0) * 0.9;
        let indicator_width = 30.0;
        let indicator_height = 4.0;

        // Background for value indicator
        let bg_rect = Path::rectangle(
            Point::new(
                center.x - indicator_width / 2.0,
                indicator_y - indicator_height / 2.0,
            ),
            Size::new(indicator_width, indicator_height),
        );
        frame.fill(&bg_rect, Color::from_rgba(0.1, 0.1, 0.1, 0.8));

        // Value position within the range
        let normalized = (gain_db + 30.0) / 60.0;
        let value_x = center.x - indicator_width / 2.0 + normalized * indicator_width;

        // Value indicator dot
        let value_dot = Path::circle(Point::new(value_x, indicator_y), 2.0);
        frame.fill(&value_dot, Color::from_rgb(0.9, 0.7, 0.2));
    }

    fn draw_center_dot(&self, frame: &mut Frame, size: Size) {
        let center = Point::new(size.width / 2.0, size.height / 2.0);
        let dot = Path::circle(center, 2.0);
        frame.fill(&dot, Color::from_rgba(0.6, 0.6, 0.6, 0.8));
    }

    fn draw_hover_highlight(&self, frame: &mut Frame, size: Size) {
        let center = Point::new(size.width / 2.0, size.height / 2.0);
        let radius = (size.width.min(size.height) / 2.0) * 0.85;

        let highlight_circle = Path::circle(center, radius);
        let highlight_stroke = Stroke::default()
            .with_width(1.5)
            .with_color(Color::from_rgba(0.9, 0.7, 0.2, 0.4));
        frame.stroke(&highlight_circle, highlight_stroke);
    }
}
