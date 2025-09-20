use crate::audio::meter::MeterConsumer;
use crate::ui::UITheme;
use nih_plug_iced::widget::canvas::{
    fill::Rule, gradient::Linear, Fill, Frame, Geometry, Gradient, Path, Program, Style,
};
use nih_plug_iced::{border::Radius, mouse, Color, Point, Rectangle, Renderer, Size, Theme};

// Local constants for meter display
const METER_MAX_DB: f32 = 0.0;
const METER_MIN_DB: f32 = -60.0;
const METER_RANGE_DB: f32 = METER_MAX_DB - METER_MIN_DB; // 60dB range

#[derive(Clone, Copy, PartialEq)]
pub enum Channel {
    Left,
    Right,
}

/// Pure meter display component - no processing logic
/// Reads meter data from MeterConsumer communication channel
pub struct MeterDisplay {
    /// Communication channel from audio thread
    meter_output: MeterConsumer,
}

impl MeterDisplay {
    pub fn new(meter_output: MeterConsumer) -> Self {
        Self { meter_output }
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

        // Draw level bars with gradient
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
        // UPDATE - Process latest meter data from audio thread
        // The MeterConsumer handles smoothing and peak hold in the UI thread
        self.meter_output.update();

        // Get smoothed levels for LED display
        let (smooth_left, smooth_right) = self.meter_output.get_smoothed_levels_or_silence();

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
        let led_count = 110;
        let led_gap = 1.0;

        // Debug logging
        if channel == Channel::Left {
            nih_plug::nih_log!("Meter drawing - Left channel: {:.2} dB", level_db);
        }

        let leds = generate_meter_leds(position, size, level_db, channel, led_count, led_gap);

        let gradient = create_meter_gradient(
            Point::new(position.x, position.y + size.height), // Bottom
            Point::new(position.x, position.y),               // Top
        );

        let gradient_fill = Fill {
            style: Style::Gradient(Gradient::Linear(gradient)),
            rule: Rule::NonZero,
        };

        for led in leds {
            if led.is_active {
                frame.fill(&led.path, gradient_fill.clone());
            } else {
                frame.fill(&led.path, UITheme::METER_BACKGROUND);
            }
        }
    }
}

/// Convert dB level to normalized 0-1 range for meter display
///
/// Maps the meter's dB range to a 0.0-1.0 scale for visual representation.
/// Values below the minimum are clamped to 0, above maximum to 1.
pub fn normalize_db_level(level_db: f32) -> f32 {
    ((level_db - METER_MIN_DB) / METER_RANGE_DB)
        .max(0.0)
        .min(1.0)
}

/// Calculate LED dimensions and spacing for a given container size
///
/// Determines the optimal LED height and positioning to fill the available
/// space while maintaining consistent gaps between LEDs.
/// Returns (led_height, led_gap, total_leds).
pub fn calculate_led_layout(
    container_height: f32,
    led_count: usize,
    led_gap: f32,
) -> (f32, f32, usize) {
    let total_gap_space = (led_count - 1) as f32 * led_gap;
    let led_height = (container_height - total_gap_space) / led_count as f32;
    (led_height, led_gap, led_count)
}

/// Calculate number of active LEDs based on normalized level
///
/// Converts a 0.0-1.0 level to the corresponding number of LEDs that should
/// be illuminated, with proper rounding for smooth visual transitions.
pub fn calculate_active_leds(normalized_level: f32, total_leds: usize) -> usize {
    (normalized_level * total_leds as f32).round() as usize
}

/// Calculate LED position for a specific LED index
///
/// Returns the Y position of an LED given its index, with LEDs numbered
/// from bottom (0) to top. Accounts for LED height and gap spacing.
pub fn calculate_led_position(
    led_index: usize,
    container_position: Point,
    container_height: f32,
    led_height: f32,
    led_gap: f32,
) -> Point {
    let led_y = container_position.y + container_height
        - (led_index as f32 * (led_height + led_gap) + led_height);
    Point::new(container_position.x, led_y)
}

/// Create gradient for meter LED visualization
///
/// Generates a linear gradient from green (bottom) through yellow to red (top),
/// matching professional audio meter color schemes.
pub fn create_meter_gradient(start_point: Point, end_point: Point) -> Linear {
    Linear::new(start_point, end_point)
        .add_stop(
            0.0,
            Color::from_rgb(44.0 / 255.0, 67.0 / 255.0, 27.0 / 255.0),
        ) // Green
        .add_stop(
            0.95,
            Color::from_rgb(214.0 / 255.0, 198.0 / 255.0, 82.0 / 255.0),
        ) // Yellow at 95%
        .add_stop(
            0.97,
            Color::from_rgb(255.0 / 255.0, 140.0 / 255.0, 0.0),
        ) // Orange transition
        .add_stop(
            1.0,
            Color::from_rgb(255.0 / 255.0, 77.0 / 255.0, 26.0 / 255.0),
        ) // Red for top 3%
}

/// Create rounded rectangle path for channel-specific LED shape
///
/// Generates the appropriate rounded rectangle path for left or right channel LEDs.
/// Left channel has rounded left corners, right channel has rounded right corners.
pub fn create_channel_led_path(position: Point, size: Size, radius: f32, channel: Channel) -> Path {
    match channel {
        Channel::Left => Path::rounded_rectangle(
            position,
            size,
            Radius {
                top_left: radius,
                top_right: 0.0,
                bottom_right: 0.0,
                bottom_left: radius,
            },
        ),
        Channel::Right => Path::rounded_rectangle(
            position,
            size,
            Radius {
                top_left: 0.0,
                top_right: radius,
                bottom_right: radius,
                bottom_left: 0.0,
            },
        ),
    }
}

/// Generate LED rendering data for a complete meter bar
///
/// Creates all the data needed to render a meter bar including positions,
/// sizes, and active/inactive states for each LED. Returns a vector of
/// LED rendering information.
pub struct LedInfo {
    pub is_active: bool,
    pub path: Path,
}

pub fn generate_meter_leds(
    container_position: Point,
    container_size: Size,
    level_db: f32,
    channel: Channel,
    led_count: usize,
    led_gap: f32,
) -> Vec<LedInfo> {
    let normalized_level = normalize_db_level(level_db);
    let (led_height, _, _) = calculate_led_layout(container_size.height, led_count, led_gap);
    let active_leds = calculate_active_leds(normalized_level, led_count);
    let radius = led_height / 2.0;

    (0..led_count)
        .map(|i| {
            let position = calculate_led_position(
                i,
                container_position,
                container_size.height,
                led_height,
                led_gap,
            );
            let size = Size::new(container_size.width, led_height);
            let is_active = i < active_leds;
            let path = create_channel_led_path(position, size, radius, channel);

            LedInfo { is_active, path }
        })
        .collect()
}
