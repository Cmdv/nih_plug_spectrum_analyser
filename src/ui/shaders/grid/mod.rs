pub mod pipeline;

use pipeline::GridPipeline;

use nih_plug_iced::{mouse, Rectangle};
use nih_plug_iced::widget::shader::{self, Primitive};
use nih_plug_iced::renderer::wgpu::wgpu;

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
        _state: &Self::State,    // Persistent state (unused here)
        _cursor: mouse::Cursor,   // Mouse position (unused here)
        bounds: Rectangle,        // Widget bounds in screen space
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
        Self {
            bounds,
        }
    }
}

// The Primitive trait defines how our custom GPU primitive works
// Based on the iced 0.14 API, it uses Renderer associated type with initialize/prepare/render
impl Primitive for GridPrimitive {
    // The renderer type that persists between frames
    type Renderer = GridPipeline;

    // Called once to initialize the renderer
    fn initialize(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
    ) -> Self::Renderer {
        GridPipeline::new(device, format)
    }

    // Called before rendering to prepare GPU resources
    fn prepare(
        &self,
        renderer: &mut Self::Renderer,
        _device: &wgpu::Device,
        queue: &wgpu::Queue,
        _bounds: &Rectangle,
        _viewport: &nih_plug_iced::graphics::Viewport,
    ) {
        // Update uniforms with current bounds
        // This uploads the new data to the GPU
        renderer.update(queue, &self.bounds);
    }

    // Called to execute the actual rendering
    fn render(
        &self,
        renderer: &Self::Renderer,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        clip_bounds: &Rectangle<u32>,
    ) {
        // Execute the render commands
        renderer.render(encoder, target, *clip_bounds);
    }
}

// Default implementation for convenience
impl Default for GridShader {
    fn default() -> Self {
        Self::new()
    }
}