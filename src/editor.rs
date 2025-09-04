use crate::audio::processor::AudioProcessor;
use crate::ui::SpectrumView;
use crate::PluginLearnParams;
use nih_plug::context::gui::GuiContext;
use nih_plug_iced::executor::Default;
use nih_plug_iced::{Element, IcedEditor, Renderer, Task, Theme};
use std::sync::Arc;
use std::sync::Mutex;

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
    type Message = (); // Your message enum
    type InitializationFlags = EditorInitFlags; // Data needed to create editor
    type Theme = Theme;

    fn new(
        initialization_flags: Self::InitializationFlags,
        context: Arc<dyn GuiContext>,
    ) -> (Self, Task<Self::Message>) {
        let frequency_bins = Arc::new(vec![0.0; 1024]); // Placeholder data

        let editor = Self {
            audio_processor: initialization_flags.audio_processor,
            params: initialization_flags.params,
            spectrum_view: SpectrumView {
                frequency_bins,
                width: 800.0,
                height: 400.0,
            },
            context, // Store the context!
        };

        (editor, Task::none()) // Return editor and no initial task
    }

    fn context(&self) -> &dyn GuiContext {
        self.context.as_ref()
    }

    fn update(&mut self, _message: Self::Message) -> Task<Self::Message> {
        Task::none() // No updates yet
    }

    fn view(&self) -> Element<'_, Self::Message, Self::Theme, Renderer> {
        // Create a Canvas widget using your SpectrumView
        use nih_plug_iced::widget::canvas::Canvas;
        Canvas::new(&self.spectrum_view).into()
    }

    fn theme(&self) -> Self::Theme {
        Theme::default() // Use default dark theme
    }
}
