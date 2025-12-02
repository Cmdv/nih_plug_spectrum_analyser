pub mod pipeline;

use pipeline::SpectrumPipeline;

use crate::SAPluginParams;
use atomic_float::AtomicF32;
use nih_plug_iced::renderer::wgpu::wgpu;
use nih_plug_iced::widget::shader::{self, Primitive};
use nih_plug_iced::{mouse, Rectangle};
use std::sync::{atomic::Ordering, Arc, Mutex};

// SpectrumShader implements the Program trait, which is iced's interface for custom shaders
// It acts as the bridge between iced's widget system and our WGPU rendering code
pub struct SpectrumShader {
    // We use Arc<Mutex> for thread-safe interior mutability
    // This is necessary because:
    // 1. view() only gives us immutable access (&self)
    // 2. spectrum data changes each frame
    // 3. iced requires Send trait (can be moved between threads)
    // Mutex provides thread-safe runtime locking instead of RefCell's single-thread borrowing

    // Raw FFT bins from audio thread (in dB)
    spectrum_data: Arc<Mutex<Vec<f32>>>,
    // Shared reference to sample rate for frequency calculations
    sample_rate: Arc<AtomicF32>,
    // Shared reference to plugin parameters for amplitude range
    plugin_params: Arc<SAPluginParams>,
}

impl SpectrumShader {
    pub fn new(sample_rate: Arc<AtomicF32>, plugin_params: Arc<SAPluginParams>) -> Self {
        Self {
            spectrum_data: Arc::new(Mutex::new(Vec::new())),
            sample_rate,
            plugin_params,
        }
    }

    // Update raw spectrum data from audio thread
    // Can be called with &self (not &mut self) thanks to Arc<Mutex>
    // The lock() ensures thread-safe access to the data
    pub fn update_spectrum_data(&self, data: Vec<f32>) {
        if let Ok(mut guard) = self.spectrum_data.lock() {
            *guard = data;
        }
    }
}

// The Program trait tells iced how to manage and render our shader
// Message is the application's message type (for event handling)
impl<Message> shader::Program<Message> for SpectrumShader {
    // State can hold data that persists between frames
    // We don't need any persistent state beyond what's in SpectrumShader itself
    type State = ();

    // Primitive is the actual rendering data/commands
    // It implements the shader::Primitive trait
    type Primitive = SpectrumPrimitive;

    // Called every frame to create the primitive that will be rendered
    // This is where we pass current data to the GPU
    fn draw(
        &self,
        _state: &Self::State,   // Persistent state (unused here)
        _cursor: mouse::Cursor, // Mouse position (unused for now)
        bounds: Rectangle,      // Widget bounds in screen space
    ) -> Self::Primitive {
        // Lock and read spectrum data
        let raw_fft_bins = self
            .spectrum_data
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default();

        // Read sample rate from shared atomic
        let sample_rate = self.sample_rate.load(Ordering::Relaxed);

        // Read dB range from plugin parameters
        let (min_db, max_db) = self.plugin_params.range.value().to_db_range();

        // Read animation speed and convert to multiplier
        let animation_speed = self
            .plugin_params
            .speed
            .value()
            .to_animation_speed_multiplier();

        // Transform raw FFT bins to logarithmically-sampled display data
        // This ensures correct frequency scaling for musical perception
        let log_sampled_data = pipeline::compute_log_sampled_spectrum(&raw_fft_bins, sample_rate);

        SpectrumPrimitive::new(bounds, log_sampled_data, min_db, max_db, animation_speed)
    }

    // Note: update() method omitted - using default implementation
    // The default returns None, which is fine for our spectrum display
}

// SpectrumPrimitive holds the data needed for one frame of rendering
// It's created fresh each frame by the draw() method above
#[derive(Debug)]
pub struct SpectrumPrimitive {
    bounds: Rectangle,
    spectrum_data: Vec<f32>, // The log-sampled dB values for display
    min_db: f32,             // Minimum dB for amplitude range (from user parameter)
    max_db: f32,             // Maximum dB for amplitude range (from user parameter)
    animation_speed: f32,    // Animation speed multiplier for fluid shader (from user parameter)
}

impl SpectrumPrimitive {
    pub fn new(
        bounds: Rectangle,
        spectrum_data: Vec<f32>,
        min_db: f32,
        max_db: f32,
        animation_speed: f32,
    ) -> Self {
        Self {
            bounds,
            spectrum_data,
            min_db,
            max_db,
            animation_speed,
        }
    }
}

// The Primitive trait defines how our custom GPU primitive works
// Based on the iced 0.14 API, it uses Renderer associated type with initialize/prepare/render
impl Primitive for SpectrumPrimitive {
    type Pipeline = SpectrumPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        viewport: &nih_plug_iced::graphics::Viewport,
    ) {
        // Get physical size from viewport for accurate pixel-level rendering
        // This ensures the spectrum is drawn at the actual screen resolution,
        // not the logical size which would be scaled/zoomed
        let physical_size = viewport.physical_size();

        pipeline.update_with_physical_size(
            device,
            queue,
            &self.bounds,
            physical_size,
            &self.spectrum_data,
            self.min_db,
            self.max_db,
            self.animation_speed,
        );
    }

    fn render(
        &self,
        pipeline: &Self::Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        // Execute the render commands
        pipeline.render(encoder, target, *clip_bounds, &self.bounds);
    }
}
