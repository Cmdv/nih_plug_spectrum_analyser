use crate::audio::meter_communication::MeterOutput;
use crate::audio::spectrum_analyzer::SpectrumOutput;
use crate::ui::{GainKnobDisplay, MeterDisplay, SpectrumDisplay, UITheme};
use crate::PluginLearnParams;

use atomic_float::AtomicF32;
use nih_plug::context::gui::GuiContext;
use nih_plug_iced::executor::Default;
use nih_plug_iced::futures::Subscription;
use nih_plug_iced::widget::canvas::Canvas;
use nih_plug_iced::widget::{column, container, row, text};
use nih_plug_iced::widgets::ParamSlider;
use nih_plug_iced::{alignment::Horizontal, Element, IcedEditor, Length, Renderer, Task, Theme};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Message {
    /// Update a parameter's value.
    ParamUpdate(nih_plug_iced::widgets::ParamMessage),
    /// Timer tick for regular redraws
    Tick,
}

/// Grouped UI data following Diopser pattern
/// Contains all data needed for the editor UI thread
#[derive(Clone)]
pub struct EditorData {
    /// PARAMETER ACCESS
    pub params: Arc<PluginLearnParams>,

    /// AUDIO STATE - Read-only from UI
    pub sample_rate: Arc<AtomicF32>,

    /// DISPLAY DATA - Separated communication channels
    pub spectrum_output: SpectrumOutput,
    pub meter_output: MeterOutput,
}

#[derive(Clone)]
pub struct EditorInitFlags {
    pub params: Arc<PluginLearnParams>,
    pub sample_rate: Arc<AtomicF32>,
    pub spectrum_output: SpectrumOutput,
    pub meter_output: MeterOutput,
}

pub struct PluginEditor {
    /// EDITOR DATA - Grouped UI dependencies
    editor_data: EditorData,

    /// DISPLAY COMPONENTS - Pure rendering
    spectrum_display: SpectrumDisplay,
    meter_display: MeterDisplay,
    knob_display: GainKnobDisplay,

    /// GUI CONTEXT
    context: Arc<dyn GuiContext>,
}

impl IcedEditor for PluginEditor {
    type Executor = Default;
    type Message = Message;
    type InitializationFlags = EditorInitFlags; // Data needed to create editor
    type Theme = Theme;

    fn new(
        initialization_flags: Self::InitializationFlags,
        context: Arc<dyn GuiContext>,
    ) -> (Self, Task<Self::Message>) {
        // Create grouped editor data following Diopser pattern
        let editor_data = EditorData {
            params: initialization_flags.params.clone(),
            sample_rate: initialization_flags.sample_rate,
            spectrum_output: initialization_flags.spectrum_output,
            meter_output: initialization_flags.meter_output,
        };

        let editor = Self {
            // DISPLAY COMPONENTS - Pure rendering with new communication channels
            spectrum_display: SpectrumDisplay::new(
                editor_data.spectrum_output.clone(),
                editor_data.sample_rate.clone(),
            ),
            meter_display: MeterDisplay::new(editor_data.meter_output.clone()),
            knob_display: GainKnobDisplay::new(editor_data.params.clone()),

            // GROUPED DATA
            editor_data,
            context,
        };

        (editor, Task::none()) // Return editor and no initial task
    }

    fn context(&self) -> &dyn GuiContext {
        self.context.as_ref()
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::ParamUpdate(message) => {
                self.handle_param_message(message);
                Task::none()
            }
            Message::Tick => {
                // Request a redraw by returning none
                // The canvas will automatically redraw with latest spectrum data
                Task::none()
            }
        }
    }

    fn subscription(
        &self,
        window_subs: &mut nih_plug_iced::window::WindowSubs<Self::Message>,
    ) -> Subscription<Self::Message> {
        // Set up a callback that runs before each frame render
        window_subs.on_frame = Some(Arc::new(|| Some(Message::Tick)));

        // Return no additional subscriptions
        Subscription::none()
    }

    fn view(&self) -> Element<'_, Self::Message, Self::Theme, Renderer> {
        // Main spectrum analyzer - maximize space to eliminate dead area
        let spectrum = Canvas::new(&self.spectrum_display)
            .width(Length::FillPortion(6)) // More space for spectrum (85.7%)
            .height(Length::Fill);

        // Right side panel with knob and meter
        let right_panel = column![
            // Gain knob at the top right - same width as meter
            container(
                ParamSlider::new(&self.editor_data.params.gain)
                    .width(Length::Fixed(UITheme::METER_WIDTH))
                    .height(Length::Fixed(UITheme::METER_WIDTH))
                    .map(Message::ParamUpdate)
            )
            .width(Length::Fill)
            .padding(UITheme::PADDING_SMALL),
            // dB value display above meter
            container({
                // Update meter processing before reading peak hold
                self.editor_data.meter_output.update();
                text(format!(
                    "{:.1} dB",
                    self.editor_data.meter_output.get_peak_hold_db()
                ))
                .size(10.0)
                .color(UITheme::TEXT_SECONDARY)
            })
            .width(Length::Fill)
            .align_x(Horizontal::Center)
            .padding(UITheme::PADDING_SMALL),
            // Level meter below the dB display
            container(
                Canvas::new(&self.meter_display)
                    .width(Length::Fixed(UITheme::METER_WIDTH))
                    .height(Length::Fill)
            )
            .width(Length::Fill)
            .padding(UITheme::PADDING_SMALL)
        ]
        .spacing(UITheme::PADDING_SMALL);

        // Main layout - optimized for Pro-Q style appearance
        container(
            row![
                // Spectrum analyzer - takes all available space
                container(spectrum)
                    .width(Length::Fill) // Take all remaining space
                    .height(Length::Fill)
                    .style(UITheme::background_dark),
                // Right side controls - compact fixed width
                container(right_panel)
                    .width(Length::Fixed(80.0)) // Fixed width: 60px content + padding
                    .height(Length::Fill)
                    .padding(5) // Small padding
                    .style(UITheme::background_dark)
            ]
            .spacing(0), // No gap between areas
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(UITheme::background_dark) // Dark background
        .into()
    }

    fn theme(&self) -> Self::Theme {
        Theme::default() // Use default dark theme
    }
}
