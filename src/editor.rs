use crate::audio::processor::AudioProcessor;
use crate::ui::{SpectrumView, LevelMeter, GainKnob};
use crate::PluginLearnParams;
use nih_plug::context::gui::GuiContext;
use nih_plug_iced::executor::Default;
use nih_plug_iced::futures::Subscription;
use nih_plug_iced::widget::canvas::Canvas;
use nih_plug_iced::widget::{container, row, column};
use nih_plug_iced::widgets::ParamSlider;
use nih_plug_iced::{Element, IcedEditor, Length, Renderer, Task, Theme};
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
    pub audio_processor: Arc<Mutex<AudioProcessor>>,
    pub params: Arc<PluginLearnParams>,
    pub spectrum_data: Arc<RwLock<Vec<f32>>>,
    pub peak_level_left: Arc<atomic_float::AtomicF32>,
    pub peak_level_right: Arc<atomic_float::AtomicF32>,
}

pub struct PluginEditor {
    audio_processor: Arc<Mutex<AudioProcessor>>,
    params: Arc<PluginLearnParams>,
    spectrum_view: SpectrumView,
    level_meter: LevelMeter,
    gain_knob: GainKnob,
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
        let editor = Self {
            audio_processor: initialization_flags.audio_processor,
            params: initialization_flags.params.clone(),
            spectrum_view: SpectrumView::new(initialization_flags.spectrum_data),
            level_meter: LevelMeter::new(
                initialization_flags.peak_level_left,
                initialization_flags.peak_level_right
            ),
            gain_knob: GainKnob::new(initialization_flags.params),
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
        use crate::ui::AudioTheme;
        
        // Main spectrum analyzer (left side, Pro-Q style)
        let spectrum = Canvas::new(&self.spectrum_view)
            .width(Length::FillPortion(4)) // 80% width like Pro-Q
            .height(Length::Fill);

        // Right side panel with knob and meter (Pro-Q style)
        let right_panel = column![
            // Gain knob at the top right
            container(
                ParamSlider::new(&self.params.gain)
                    .width(Length::Fixed(80.0))
                    .height(Length::Fixed(80.0))
                    .map(Message::ParamUpdate)
            )
            .width(Length::Fill)
            .style(container::dark)
            .padding(10), // Simple uniform padding

            // Level meter below the knob  
            Canvas::new(&self.level_meter)
                .width(Length::Fixed(60.0))
                .height(Length::Fill)
        ]
        .spacing(5);

        // Main layout (Pro-Q style: spectrum + right panel) with dark background
        container(
            row![
                // Spectrum analyzer (main area)
                container(spectrum)
                    .padding(5) // Small padding around spectrum
                    .width(Length::FillPortion(4))
                    .style(container::dark),
                    
                // Right side controls
                container(right_panel)
                    .width(Length::FillPortion(1)) // 20% width
                    .height(Length::Fill)
                    .padding(5) // Padding around right panel
                    .style(container::dark)
            ]
            .spacing(0) // No gap between main areas
            .width(Length::Fill)
            .height(Length::Fill)
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .style(container::dark) // Apply dark background to entire UI
        .into()
    }

    fn theme(&self) -> Self::Theme {
        Theme::default() // Use default dark theme
    }
}
