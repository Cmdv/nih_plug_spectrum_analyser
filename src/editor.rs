use crate::audio::meter::MeterConsumer;
use crate::audio::spectrum::SpectrumConsumer;
use crate::ui::{GridOverlay, MeterDisplay, SpectrumDisplay, UITheme, GridShader};
use crate::SAPluginParams;

use atomic_float::AtomicF32;
use nih_plug::context::gui::GuiContext;
use nih_plug_iced::executor::Default;
use nih_plug_iced::futures::Subscription;
use nih_plug_iced::widget::canvas::Canvas;
use nih_plug_iced::widget::{column, container, row, stack, text, shader};
use nih_plug_iced::widgets::ResizeHandle;
use nih_plug_iced::{window, IcedState, Padding};
use nih_plug_iced::{alignment::Horizontal, Element, IcedEditor, Length, Renderer, Task, Theme};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub enum Message {
    /// Timer tick for regular redraws
    Tick,
    /// User dragged resize handle to request new size
    RequestResize(nih_plug_iced::Size),
    /// Window was actually resized (from baseview/iced event)
    WindowResized(nih_plug_iced::Size),
}

/// Grouped UI data structure
/// Contains all data needed for the editor UI thread
#[derive(Clone)]
pub struct EditorData {
    /// AUDIO STATE - Read-only from UI
    pub plugin_params: Arc<SAPluginParams>,
    pub sample_rate: Arc<AtomicF32>,
    pub process_stopped: Arc<AtomicBool>,

    /// DISPLAY DATA - Separated communication channels
    pub spectrum_output: SpectrumConsumer,
    pub meter_output: MeterConsumer,
}

#[derive(Clone)]
pub struct EditorInitFlags {
    pub plugin_params: Arc<SAPluginParams>,
    pub sample_rate: Arc<AtomicF32>,
    pub process_stopped: Arc<AtomicBool>,
    pub spectrum_output: SpectrumConsumer,
    pub meter_output: MeterConsumer,
    pub iced_state: Arc<IcedState>,
}

pub struct PluginEditor {
    /// EDITOR DATA - Grouped UI dependencies
    editor_data: EditorData,

    /// DISPLAY COMPONENTS - Pure rendering
    spectrum_display: SpectrumDisplay,
    grid_overlay: GridOverlay,
    meter_display: MeterDisplay,

    /// GPU SHADERS - High performance rendering
    grid_shader: GridShader,

    /// GUI CONTEXT
    context: Arc<dyn GuiContext>,

    /// ICED STATE - For window resize
    iced_state: Arc<IcedState>,
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
        .size(6.0)
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
            // Outer container with padding to shift the entire stack
            container(
                // Inner container for the stack without padding
                container(layered_spectrum)
                    .width(Length::Fill)
                    .height(Length::Fill)
                    .style(UITheme::background_dark)
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(Padding::default().top(5).left(10))
            .style(UITheme::background_dark),
            container(right_panel)
                .width(Length::Fixed(UITheme::METER_WIDTH + 15.0))
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
        // Create grouped editor data structure
        let editor_data = EditorData {
            plugin_params: initialization_flags.plugin_params,
            sample_rate: initialization_flags.sample_rate,
            process_stopped: initialization_flags.process_stopped,
            spectrum_output: initialization_flags.spectrum_output,
            meter_output: initialization_flags.meter_output,
        };

        let editor = Self {
            // DISPLAY COMPONENTS - Pure rendering with new communication channels
            spectrum_display: SpectrumDisplay::new(
                editor_data.spectrum_output.clone(),
                editor_data.sample_rate.clone(),
                editor_data.plugin_params.clone(),
            ),
            grid_overlay: GridOverlay::new(),
            meter_display: MeterDisplay::new(editor_data.meter_output.clone()),

            // GPU SHADERS - High performance rendering
            grid_shader: GridShader::new(),

            // ICED STATE
            iced_state: initialization_flags.iced_state.clone(),

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
            Message::RequestResize(size) => {
                // User dragged resize handle - request window resize through iced/baseview
                // This will trigger a Window::Resized event which will call Message::WindowResized
                window::resize(size)
            }
            Message::WindowResized(size) => {
                // Window was actually resized (from baseview)
                // Update iced_state to persist the size for next time window opens
                self.iced_state.set_size(size.width as u32, size.height as u32);
                // Notify the host that the window size changed
                // If the host rejects it, it will resize us back
                self.context.request_resize();
                // No task needed - the window is already resized
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

        // Set up a callback for window resize events
        window_subs.on_resize = Some(Arc::new(|size| Some(Message::WindowResized(size))));

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

        // Canvas-based grid (existing) - commented out for shader testing
        let _grid_canvas: Canvas<&GridOverlay, Message> = Canvas::new(&self.grid_overlay)
            .width(Length::FillPortion(6))
            .height(Length::Fill);

        // GPU shader-based grid (new - for testing)
        // This demonstrates our WGPU grid shader working alongside the canvas
        let grid_shader_widget = shader(&self.grid_shader)
            .width(Length::FillPortion(6))
            .height(Length::Fill);

        // Stack the canvases and shader on top of each other
        // Both grids will render - we can compare performance and visual quality
        let layered_spectrum = stack![
            spectrum_container,
            // grid_canvas,        // Comment out canvas grid to see shader grid
            grid_shader_widget,    // Our new GPU-accelerated grid
        ];

        let db_display =
            create_db_display(self.editor_data.meter_output.get_peak_hold_db_or_silence());
        let meter_canvas = create_meter_canvas(&self.meter_display);

        // Compose layout using pure functions
        let right_panel = create_right_panel(db_display, meter_canvas);

        // Add resize handle to the right panel at the bottom
        let (current_width, current_height) = self.iced_state.size();
        let current_size = nih_plug_iced::Size::new(current_width as f32, current_height as f32);

        let right_panel_with_resize = column![
            right_panel,
            container(
                ResizeHandle::new(current_size, |size| Message::RequestResize(size))
                    .size(20.0)
                    .min_size(400.0, 300.0)
                    .color(nih_plug_iced::Color::from_rgba(0.7, 0.7, 0.7, 0.6))
            )
            .width(Length::Fill)
            .align_x(Horizontal::Right)
        ];

        let main_content = create_main_layout_with_stack(layered_spectrum, right_panel_with_resize.into());

        // Apply grey overlay when processing is stopped
        if self.editor_data.process_stopped.load(Ordering::Relaxed) {
            // Create a semi-transparent grey overlay
            let overlay = container(text(""))
                .width(Length::Fill)
                .height(Length::Fill)
                .style(|_theme| container::Style {
                    background: Some(nih_plug_iced::Background::Color(
                        nih_plug_iced::Color::from_rgba(0.1, 0.1, 0.1, 0.8),
                    )),
                    ..container::Style::default()
                });

            stack![main_content, overlay].into()
        } else {
            main_content
        }
    }

    fn theme(&self) -> Self::Theme {
        Theme::Dark
    }
}
