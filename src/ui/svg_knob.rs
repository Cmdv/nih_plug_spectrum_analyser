use crate::ui::UITheme;
use nih_plug::prelude::*;
use nih_plug_iced::widget::svg::{self, Svg};
use nih_plug_iced::widget::{container, text, Column};
use nih_plug_iced::widgets::ParamMessage;
use nih_plug_iced::{Element, Length, Point, Rectangle, Size, Theme};
use std::sync::Arc;

/// A Pro-Q style SVG knob widget for gain control
pub struct SvgKnob<'a> {
    param: &'a FloatParam,
}

impl<'a> SvgKnob<'a> {
    pub fn new(param: &'a FloatParam) -> Self {
        Self { param }
    }

    /// Generate SVG content for the knob at current parameter value
    fn generate_knob_svg(&self) -> String {
        let gain_db = util::gain_to_db(self.param.value());
        let normalized = UITheme::gain_db_to_normalized(gain_db);

        // Convert to rotation angle (Pro-Q style: -150° to +150°)
        let angle_deg =
            UITheme::KNOB_MIN_ANGLE_DEG + (normalized * UITheme::KNOB_TOTAL_ROTATION_DEG);

        // SVG knob with Pro-Q styling
        format!(
            r#"
            <svg viewBox="0 0 80 80" xmlns="http://www.w3.org/2000/svg">
                <!-- Knob background -->
                <circle cx="40" cy="40" r="32" fill="rgb(31, 31, 37)" stroke="rgb(102, 102, 102)" stroke-width="1"/>
                <circle cx="40" cy="40" r="27" fill="rgb(46, 46, 56)"/>
                
                <!-- Knob pointer -->
                <g transform="rotate({} 40 40)">
                    <line x1="40" y1="40" x2="40" y2="18" stroke="rgb(230, 179, 51)" stroke-width="2.5" stroke-linecap="round"/>
                </g>
                
                <!-- Center dot -->
                <circle cx="40" cy="40" r="2" fill="rgb(153, 153, 153)"/>
                
                <!-- Value indicator at bottom -->
                <rect x="25" y="65" width="30" height="4" fill="rgb(26, 26, 26)" rx="2"/>
                <circle cx="{}" cy="67" r="2" fill="rgb(230, 179, 51)"/>
            </svg>
            "#,
            angle_deg,
            25.0 + normalized * 30.0 // Position dot along indicator bar
        )
    }

    /// Create a text label showing the current dB value
    fn create_value_text(&self) -> Element<'_, ParamMessage, Theme> {
        let gain_db = util::gain_to_db(self.param.value());
        let text_content = if gain_db >= 0.0 {
            format!("+{:.1} dB", gain_db)
        } else {
            format("{:.1} dB", gain_db)
        };

        text(text_content)
            .size(10)
            .color(UITheme::TEXT_SECONDARY)
            .into()
    }
}

impl<'a> From<SvgKnob<'a>> for Element<'a, ParamMessage, Theme> {
    fn from(knob: SvgKnob<'a>) -> Self {
        // Create column with SVG knob and value text
        Column::new()
            .push(
                // SVG knob with mouse interaction
                container(
                    Svg::new(svg::Handle::from_memory(
                        knob.generate_knob_svg().into_bytes(),
                    ))
                    .width(Length::Fixed(UITheme::KNOB_SIZE))
                    .height(Length::Fixed(UITheme::KNOB_SIZE)),
                )
                .center_x(Length::Fill), // TODO: Add mouse interaction handlers here
                                         // This is where we'd add .on_press(), .on_drag(), etc.
            )
            .push(
                // Value text below knob
                container(knob.create_value_text())
                    .center_x(Length::Fill)
                    .padding([5, 0, 0, 0]),
            )
            .align_items(nih_plug_iced::Alignment::Center)
            .spacing(2)
            .into()
    }
}

// TODO: Implement proper mouse interaction
// We need to implement the Widget trait to handle mouse events properly
// This would follow the same pattern as ParamSlider but for rotary controls
