pub mod pipeline;

use pipeline::GridPipeline;

use nih_plug_iced::renderer::wgpu::wgpu;
use nih_plug_iced::widget::shader::{self, Primitive};
use nih_plug_iced::{mouse, Rectangle};

// GridShader implements the Program trait, which is iced's interface for custom shaders
// It acts as the bridge between iced's widget system and our WGPU rendering code
pub struct GridShader;

impl GridShader {
    pub fn new() -> Self {
        Self
    }
}

// The Program trait tells iced how to manage and render our shader
// Message is the application's message type (for event handling)
impl<Message> shader::Program<Message> for GridShader {
    // State can hold data that persists between frames
    // We don't need any for a simple grid
    type State = ();

    // Primitive is the actual rendering data/commands
    // It implements the shader::Primitive trait
    type Primitive = GridPrimitive;

    // Called every frame to create the primitive that will be rendered
    // This is where we pass current data to the GPU
    fn draw(
        &self,
        _state: &Self::State,   // Persistent state (unused here)
        _cursor: mouse::Cursor, // Mouse position (unused here)
        bounds: Rectangle,      // Widget bounds in screen space
    ) -> Self::Primitive {
        GridPrimitive::new(bounds)
    }

    // Note: update() method omitted - using default implementation
    // The default returns None, which is perfect for our static grid
}

// GridPrimitive holds the data needed for one frame of rendering
// It's created fresh each frame by the draw() method above
#[derive(Debug)]
pub struct GridPrimitive {
    bounds: Rectangle,
}

impl GridPrimitive {
    pub fn new(bounds: Rectangle) -> Self {
        Self { bounds }
    }
}

impl Primitive for GridPrimitive {
    type Pipeline = GridPipeline;

    fn prepare(
        &self,
        pipeline: &mut Self::Pipeline,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        viewport: &nih_plug_iced::graphics::Viewport,
    ) {
        // Get physical size from viewport for accurate pixel-level rendering
        // This ensures the grid is drawn at the actual screen resolution,
        // not the logical size which would be scaled/zoomed
        let physical_size = viewport.physical_size();

        pipeline.update_with_physical_size(queue, &self.bounds, physical_size);
        pipeline.prepare_cache(device, queue, physical_size);
    }

    fn render(
        &self,
        pipeline: &Self::Pipeline,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        pipeline.render(encoder, target, *clip_bounds);
    }
}

// Default implementation for convenience
impl Default for GridShader {
    fn default() -> Self {
        Self::new()
    }
}
