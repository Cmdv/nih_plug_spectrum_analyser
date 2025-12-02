use crate::audio::meter::MeterConsumer;
use crate::audio::spectrum::SpectrumConsumer;
use crate::ui::{GridShader, MeterShader, SpectrumShader, UITheme};
use crate::SAPluginParams;

use atomic_float::AtomicF32;
use nih_plug::context::gui::GuiContext;
use nih_plug_iced::executor::Default;
use nih_plug_iced::futures::Subscription;
use nih_plug_iced::widget::{column, container, row, shader, stack, text};
use nih_plug_iced::widgets::ResizeHandle;
use nih_plug_iced::{alignment::Horizontal, Element, IcedEditor, Length, Renderer, Task, Theme};
use nih_plug_iced::{window, IcedState, Padding};
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

    /// GPU SHADERS - High performance rendering
    grid_shader: GridShader,
    spectrum_shader: SpectrumShader,
    meter_shader: MeterShader,

    /// GUI CONTEXT
    context: Arc<dyn GuiContext>,

    /// ICED STATE - For window resize
    iced_state: Arc<IcedState>,

    /// Track if we have audio (to skip spectrum rendering when silent)
    has_audio: bool,
}

/// Create dB value display text widget
pub fn create_db_display(peak_hold_db: f32) -> Element<'static, Message, Theme, Renderer> {
    text(format!("{:.1} dB", peak_hold_db))
        .size(6.0)
        .color(UITheme::TEXT_SECONDARY)
        .into()
}

/// Create right panel layout with knob and meter
pub fn create_right_panel<'a>(
    db_display: Element<'a, Message, Theme, Renderer>,
    meter_widget: Element<'a, Message, Theme, Renderer>,
) -> Element<'a, Message, Theme, Renderer> {
    column![
        container(db_display)
            .width(Length::Fill)
            .align_x(Horizontal::Center)
            .padding(UITheme::PADDING_SMALL),
        container(meter_widget)
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
            // GPU SHADERS - High performance rendering
            grid_shader: GridShader::new(),
            spectrum_shader: SpectrumShader::new(
                editor_data.sample_rate.clone(),
                editor_data.plugin_params.clone(),
            ),
            meter_shader: MeterShader::new(editor_data.meter_output.clone()),

            // ICED STATE
            iced_state: initialization_flags.iced_state.clone(),

            // GROUPED DATA
            editor_data,
            context,

            // Start with no audio
            has_audio: false,
        };

        (editor, Task::none()) // Return editor and no initial task
    }

    fn context(&self) -> &dyn GuiContext {
        self.context.as_ref()
    }

    fn update(&mut self, message: Self::Message) -> Task<Self::Message> {
        match message {
            Message::Tick => {
                // Update spectrum shader with latest data
                if let Ok(spectrum_data) = self.editor_data.spectrum_output.read() {
                    // Check if there's any meaningful audio (not just silence)
                    // If all bins are near minimum dB, we have silence (empty buffers)
                    let silence_threshold = -90.0; // dB threshold for "no audio"
                    let has_audio = spectrum_data.iter().any(|&db| db > silence_threshold);

                    // Update flag for view() to use
                    self.has_audio = has_audio;

                    if has_audio {
                        self.spectrum_shader.update_spectrum_data(spectrum_data);
                    }
                    // If silent, don't update - this stops unnecessary redraws
                }
                // Request a redraw
                Task::none()
            }
            Message::RequestResize(size) => {
                nih_plug::nih_log!(
                    "[RESIZE] RequestResize received: {}x{}",
                    size.width, size.height
                );
                // User dragged resize handle - request window resize through iced/baseview
                // This will trigger a Window::Resized event which will call Message::WindowResized
                window::resize(size)
            }
            Message::WindowResized(size) => {
                nih_plug::nih_log!(
                    "[RESIZE] WindowResized received: {}x{}",
                    size.width, size.height
                );
                // Window was actually resized (from baseview)
                // Update iced_state to persist the size for next time window opens
                self.iced_state
                    .set_size(size.width as u32, size.height as u32);
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
        // Set up a callback for window resize events
        window_subs.on_resize = Some(Arc::new(|size| Some(Message::WindowResized(size))));

        // Always run compositor frame updates to detect audio changes
        // When silent, view() doesn't render spectrum shader (minimal GPU usage)
        // Use compositor timing (vsync-aligned, ~60-70fps depending on display)
        window_subs.on_frame = Some(Arc::new(|| Some(Message::Tick)));

        Subscription::none()
    }

    fn view(&self) -> Element<'_, Self::Message, Self::Theme, Renderer> {
        // Update meter processing before reading peak hold
        self.editor_data.meter_output.update();

        // Create widgets using pure functions
        // GPU shader-based grid (always visible)
        let grid_shader_widget = shader(&self.grid_shader)
            .width(Length::FillPortion(6))
            .height(Length::Fill);

        // Stack spectrum and grid shaders - only include spectrum when there's audio
        let layered_spectrum = if self.has_audio {
            // Use GPU shader for spectrum when there's audio
            let spectrum_shader_widget = shader(&self.spectrum_shader)
                .width(Length::FillPortion(6))
                .height(Length::Fill);

            // Wrap spectrum shader in container with bottom padding to stop before -100 line
            let spectrum_container = container(spectrum_shader_widget)
                .width(Length::Fill)
                .height(Length::Fill)
                .padding(Padding::default().bottom(30)); // 30px bottom padding

            stack![spectrum_container, grid_shader_widget,]
        } else {
            // No audio - just show the grid
            stack![grid_shader_widget]
        };

        let db_display =
            create_db_display(self.editor_data.meter_output.get_peak_hold_db_or_silence());

        // GPU meter shader (handles all updates and normalization internally)
        let meter_shader = shader(&self.meter_shader)
            .width(Length::Fixed(UITheme::METER_WIDTH))
            .height(Length::Fill)
            .into();

        // Compose layout using pure functions
        let right_panel = create_right_panel(db_display, meter_shader);

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
                    .screen_cursor(self.iced_state.screen_cursor())
            )
            .width(Length::Fill)
            .align_x(Horizontal::Right)
        ];

        let main_content =
            create_main_layout_with_stack(layered_spectrum, right_panel_with_resize.into());

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
