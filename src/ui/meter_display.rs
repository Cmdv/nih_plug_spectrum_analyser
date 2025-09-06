use crate::audio::constants;
use crate::audio::meter_engine::MeterEngine;
use crate::ui::UITheme;
use nih_plug_iced::widget::canvas::{
    fill::Rule, gradient::Linear, Fill, Frame, Geometry, Gradient, Path, Program, Style,
};
use nih_plug_iced::{border::Radius, mouse, Color, Point, Rectangle, Renderer, Size, Theme};
use std::sync::Arc;

#[derive(Clone, Copy)]
enum Channel {
    Left,
    Right,
}

pub struct MeterDisplay {
    // Meter processor handles all audio processing logic
    pub meter_processor: Arc<MeterEngine>,
}

impl MeterDisplay {
    pub fn new(meter_processor: Arc<MeterEngine>) -> Self {
        Self { meter_processor }
    }
}

impl<Message> Program<Message, Theme> for MeterDisplay {
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

        // Draw meter background
        self.draw_meter_background(&mut frame, bounds.size());

        // Draw level bars with gradient (Pro-Q style)
        self.draw_level_bars(&mut frame, bounds.size());

        vec![frame.into_geometry()]
    }
}

impl MeterDisplay {
    fn draw_meter_background(&self, frame: &mut Frame, size: Size) {
        // Dark background
        let background = Path::rectangle(Point::ORIGIN, size);
        frame.fill(&background, Color::from_rgb(0.06, 0.06, 0.08));
    }

    fn draw_level_bars(&self, frame: &mut Frame, size: Size) {
        // Update processor (processes smoothing, peak hold, etc.)
        self.meter_processor.update();

        // Get smoothed levels for LED display
        let (smooth_left, smooth_right) = self.meter_processor.get_smoothed_levels();

        // Draw level bars with consistent gap
        let channel_gap = 1.0; // Same as LED gap
        let total_width = size.width;
        let bar_width = (total_width - channel_gap) / 2.0;

        // Left channel bar
        self.draw_single_level_bar(
            frame,
            Point::new(0.0, 0.0),
            Size::new(bar_width, size.height),
            smooth_left,
            Channel::Left,
        );

        // Right channel bar
        self.draw_single_level_bar(
            frame,
            Point::new(bar_width + channel_gap, 0.0),
            Size::new(bar_width, size.height),
            smooth_right,
            Channel::Right,
        );
    }

    fn draw_single_level_bar(
        &self,
        frame: &mut Frame,
        position: Point,
        size: Size,
        level_db: f32,
        channel: Channel,
    ) {
        // Convert dB to 0-1 range using constants
        let normalized_level = ((level_db - constants::METER_MIN_DB) / constants::METER_RANGE_DB)
            .max(0.0)
            .min(1.0);

        // LED configuration
        let led_count = 110;
        let led_gap = 1.0;

        // Calculate LED height to fill container
        let available_height = size.height;
        let total_gap_space = (led_count - 1) as f32 * led_gap;
        let led_height = (available_height - total_gap_space) / led_count as f32;

        let active_leds = (normalized_level * led_count as f32).round() as usize;

        // Create gradient for the entire meter
        let gradient = Linear::new(
            Point::new(position.x, position.y + size.height), // Bottom
            Point::new(position.x, position.y),               // Top
        )
        .add_stop(
            0.0,
            Color::from_rgb(44.0 / 255.0, 67.0 / 255.0, 27.0 / 255.0),
        ) // Green #2c431b
        .add_stop(
            0.98,
            Color::from_rgb(214.0 / 255.0, 198.0 / 255.0, 82.0 / 255.0),
        ) // Yellow #d6c652
        .add_stop(
            1.0,
            Color::from_rgb(255.0 / 255.0, 77.0 / 255.0, 26.0 / 255.0),
        ); // Red for top 2 LEDs

        let gradient_fill = Fill {
            style: Style::Gradient(Gradient::Linear(gradient)),
            rule: Rule::NonZero,
        };

        // Draw each LED from bottom up
        for i in 0..led_count {
            if i >= active_leds {
                // Draw inactive LEDs with dark background
                let led_y =
                    position.y + size.height - (i as f32 * (led_height + led_gap) + led_height);
                let led_position = Point::new(position.x, led_y);
                let led_size = Size::new(size.width, led_height);

                let radius = led_height / 2.0;
                let led_path = match channel {
                    Channel::Left => Path::rounded_rectangle(
                        led_position,
                        led_size,
                        Radius {
                            top_left: radius,
                            top_right: 0.0,
                            bottom_right: 0.0,
                            bottom_left: radius,
                        },
                    ),
                    Channel::Right => Path::rounded_rectangle(
                        led_position,
                        led_size,
                        Radius {
                            top_left: 0.0,
                            top_right: radius,
                            bottom_right: radius,
                            bottom_left: 0.0,
                        },
                    ),
                };

                frame.fill(&led_path, UITheme::METER_BACKGROUND);
            } else {
                // Draw active LEDs with gradient
                let led_y =
                    position.y + size.height - (i as f32 * (led_height + led_gap) + led_height);
                let led_position = Point::new(position.x, led_y);
                let led_size = Size::new(size.width, led_height);

                let radius = led_height / 2.0;
                let led_path = match channel {
                    Channel::Left => Path::rounded_rectangle(
                        led_position,
                        led_size,
                        Radius {
                            top_left: radius,
                            top_right: 0.0,
                            bottom_right: 0.0,
                            bottom_left: radius,
                        },
                    ),
                    Channel::Right => Path::rounded_rectangle(
                        led_position,
                        led_size,
                        Radius {
                            top_left: 0.0,
                            top_right: radius,
                            bottom_right: radius,
                            bottom_left: 0.0,
                        },
                    ),
                };

                frame.fill(&led_path, gradient_fill.clone());
            }
        }
    }
}
