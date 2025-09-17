use crate::audio::meter::MeterConsumer;
use crate::audio::spectrum::SpectrumConsumer;
use crate::ui::{GridOverlay, MeterDisplay, SpectrumDisplay, UITheme};

use atomic_float::AtomicF32;
use nih_plug::context::gui::GuiContext;
use nih_plug_iced::executor::Default;
use nih_plug_iced::futures::Subscription;
use nih_plug_iced::widget::canvas::Canvas;
use nih_plug_iced::widget::{column, container, row, stack, text};
use nih_plug_iced::Padding;
use nih_plug_iced::{alignment::Horizontal, Element, IcedEditor, Length, Renderer, Task, Theme};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Message {
    /// Timer tick for regular redraws
    Tick,
}

/// Grouped UI data following Diopser pattern
/// Contains all data needed for the editor UI thread
#[derive(Clone)]
pub struct EditorData {
    /// AUDIO STATE - Read-only from UI
    pub sample_rate: Arc<AtomicF32>,

    /// DISPLAY DATA - Separated communication channels
    pub spectrum_output: SpectrumConsumer,
    pub meter_output: MeterConsumer,
}

#[derive(Clone)]
pub struct EditorInitFlags {
    pub sample_rate: Arc<AtomicF32>,
    pub spectrum_output: SpectrumConsumer,
    pub meter_output: MeterConsumer,
}

pub struct PluginEditor {
    /// EDITOR DATA - Grouped UI dependencies
    editor_data: EditorData,

    /// DISPLAY COMPONENTS - Pure rendering
    spectrum_display: SpectrumDisplay,
    grid_overlay: GridOverlay,
    meter_display: MeterDisplay,

    /// GUI CONTEXT
    context: Arc<dyn GuiContext>,
}

/// Create spectrum analyser canvas widget
pub fn create_spectrum_canvas(
    spectrum_display: &SpectrumDisplay,
) -> Canvas<&SpectrumDisplay, Message> {
    Canvas::new(spectrum_display)
        .width(Length::FillPortion(6))
        .height(Length::Fill)
}

/// Create dB value display text widget
pub fn create_db_display(peak_hold_db: f32) -> Element<'static, Message, Theme, Renderer> {
    text(format!("{:.1} dB", peak_hold_db))
        .size(10.0)
        .color(UITheme::TEXT_SECONDARY)
        .into()
}

/// Create level meter canvas widget
pub fn create_meter_canvas(meter_display: &MeterDisplay) -> Canvas<&MeterDisplay, Message> {
    Canvas::new(meter_display)
        .width(Length::Fixed(UITheme::METER_WIDTH))
        .height(Length::Fill)
}

/// Create right panel layout with knob and meter
pub fn create_right_panel<'a>(
    db_display: Element<'a, Message, Theme, Renderer>,
    meter_canvas: Canvas<&'a MeterDisplay, Message>,
) -> Element<'a, Message, Theme, Renderer> {
    column![
        container(db_display)
            .width(Length::Fill)
            .align_x(Horizontal::Center)
            .padding(UITheme::PADDING_SMALL),
        container(meter_canvas)
            .width(Length::Fill)
            .padding(UITheme::PADDING_SMALL)
    ]
    .spacing(UITheme::PADDING_SMALL)
    .into()
}

/// Create main layout container with stacked canvases
pub fn create_main_layout_with_stack<'a>(
    layered_spectrum: nih_plug_iced::widget::Stack<'a, Message, Theme, Renderer>,
    right_panel: Element<'a, Message, Theme, Renderer>,
) -> Element<'a, Message, Theme, Renderer> {
    container(
        row![
            container(layered_spectrum)
                .width(Length::Fill)
                .height(Length::Fill)
                .style(UITheme::background_dark),
            container(right_panel)
                .width(Length::Fixed(80.0))
                .height(Length::Fill)
                .padding(5)
                .style(UITheme::background_dark)
        ]
        .spacing(0),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(UITheme::background_dark)
    .into()
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
            grid_overlay: GridOverlay::new(),
            meter_display: MeterDisplay::new(editor_data.meter_output.clone()),

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
        // Update meter processing before reading peak hold
        self.editor_data.meter_output.update();

        // Create widgets using pure functions
        let spectrum_canvas = create_spectrum_canvas(&self.spectrum_display);

        // Wrap spectrum canvas in container with bottom padding to stop before -100 line
        let spectrum_container = container(spectrum_canvas)
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::default().bottom(30)); // 30px bottom padding

        let grid_canvas = Canvas::new(&self.grid_overlay)
            .width(Length::FillPortion(6))
            .height(Length::Fill);

        // Stack the canvases on top of each other
        let layered_spectrum = stack![spectrum_container, grid_canvas];

        let db_display = create_db_display(self.editor_data.meter_output.get_peak_hold_db_or_silence());
        let meter_canvas = create_meter_canvas(&self.meter_display);

        // Compose layout using pure functions
        let right_panel = create_right_panel(db_display, meter_canvas);
        create_main_layout_with_stack(layered_spectrum, right_panel)
    }

    fn theme(&self) -> Self::Theme {
        Theme::default() // Use default dark theme
    }
}
