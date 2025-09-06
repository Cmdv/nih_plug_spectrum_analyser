use crate::audio::audio_engine::AudioEngine;
use crate::audio::meter_engine::MeterEngine;
use crate::audio::spectrum_engine::SpectrumEngine;
use crate::ui::{GainKnobDisplay, MeterDisplay, SpectrumDisplay, UITheme};

use crate::PluginLearnParams;

use nih_plug::context::gui::GuiContext;
use nih_plug_iced::executor::Default;
use nih_plug_iced::futures::Subscription;
use nih_plug_iced::widget::canvas::Canvas;
use nih_plug_iced::widget::{column, container, row, text};
use nih_plug_iced::widgets::ParamSlider;
use nih_plug_iced::{alignment::Horizontal, Element, IcedEditor, Length, Renderer, Task, Theme};
use std::sync::{Arc, Mutex, RwLock};

#[derive(Debug, Clone)]
pub enum Message {
    /// Update a parameter's value.
    ParamUpdate(nih_plug_iced::widgets::ParamMessage),
    /// Timer tick for regular redraws
    Tick,
}

#[derive(Clone)]
pub struct EditorInitFlags {
    pub audio_engine: Arc<Mutex<AudioEngine>>,
    pub params: Arc<PluginLearnParams>,
    pub spectrum_data: Arc<RwLock<Vec<f32>>>,
    pub peak_level_left: Arc<atomic_float::AtomicF32>,
    pub peak_level_right: Arc<atomic_float::AtomicF32>,
}

pub struct PluginEditor {
    audio_engine: Arc<Mutex<AudioEngine>>,
    params: Arc<PluginLearnParams>,
    spectrum_display: SpectrumDisplay,
    spectrum_engine: Arc<Mutex<SpectrumEngine>>,
    meter_engine: Arc<MeterEngine>,
    meter_display: MeterDisplay,
    knob_display: GainKnobDisplay,
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
        // Create meter processor to handle all meter audio processing
        let meter_processor = Arc::new(MeterEngine::new(
            initialization_flags.peak_level_left.clone(),
            initialization_flags.peak_level_right.clone(),
        ));

        // Create spectrum engine to handle all spectrum audio processing
        let spectrum_engine = Arc::new(Mutex::new(SpectrumEngine::new(
            initialization_flags.spectrum_data.clone(),
        )));

        let editor = Self {
            audio_engine: initialization_flags.audio_engine,
            params: initialization_flags.params.clone(),
            spectrum_display: SpectrumDisplay::new(initialization_flags.spectrum_data),
            spectrum_engine,
            meter_display: MeterDisplay::new(meter_processor.clone()),
            meter_engine: meter_processor,
            knob_display: GainKnobDisplay::new(initialization_flags.params),
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
                ParamSlider::new(&self.params.gain)
                    .width(Length::Fixed(UITheme::METER_WIDTH))
                    .height(Length::Fixed(UITheme::METER_WIDTH))
                    .map(Message::ParamUpdate)
            )
            .width(Length::Fill)
            .padding(UITheme::PADDING_SMALL),
            // dB value display above meter
            container(
                text(format!("{:.1} dB", self.meter_engine.get_peak_hold_db()))
                    .size(10.0)
                    .color(UITheme::TEXT_SECONDARY)
            )
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
