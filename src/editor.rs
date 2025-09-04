use crate::audio::processor::AudioProcessor;
use crate::ui::SpectrumView;
use crate::PluginLearnParams;
use nih_plug::context::gui::GuiContext;
use nih_plug_iced::executor::Default;
use nih_plug_iced::widget::canvas::Canvas;
use nih_plug_iced::{Element, IcedEditor, Length, Renderer, Task, Theme};
use nih_plug_iced::futures::Subscription;
use std::sync::{Arc, Mutex};

#[derive(Debug, Clone)]
pub enum Message {
    Tick, // Timer tick for regular redraws
}

#[derive(Clone)]
pub struct EditorInitFlags {
    pub audio_processor: Arc<Mutex<AudioProcessor>>,
    pub params: Arc<PluginLearnParams>,
}

pub struct PluginEditor {
    audio_processor: Arc<Mutex<AudioProcessor>>,
    params: Arc<PluginLearnParams>,
    spectrum_view: SpectrumView,
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
        let frequency_bins = initialization_flags
            .audio_processor
            .lock()
            .unwrap()
            .spectrum_data
            .clone();

        let editor = Self {
            audio_processor: initialization_flags.audio_processor,
            params: initialization_flags.params,
            spectrum_view: SpectrumView { frequency_bins },
            context,
        };

        (editor, Task::none()) // Return editor and no initial task
    }

    fn context(&self) -> &dyn GuiContext {
        self.context.as_ref()
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
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
        Canvas::new(&self.spectrum_view)
            .width(Length::Fill) // Fill available width
            .height(Length::Fill) // Fill available height
            .into()
    }

    fn theme(&self) -> Self::Theme {
        Theme::default() // Use default dark theme
    }
}
