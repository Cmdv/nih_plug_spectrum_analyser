use bytemuck::{Pod, Zeroable};
use nih_plug_iced::Rectangle;
use nih_plug_iced::renderer::wgpu::wgpu::{
    self as wgpu, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingType, BufferBindingType, BufferUsages, Device, Queue,
    RenderPipeline, ShaderStages, TextureFormat,
};
use crate::audio::constants;

// Uniforms are data passed from CPU to GPU that remain constant during a draw call
// They're used for things like screen resolution, time, user settings, etc.
//
// The #[repr(C)] attribute ensures this struct has the same memory layout as C,
// which is required for GPU compatibility. The GPU expects data in a specific format.
//
// Pod (Plain Old Data) and Zeroable are bytemuck traits that guarantee:
// - The type can be safely transmuted to/from bytes
// - All bit patterns are valid (no undefined behavior)
// - Can be safely initialized with zeros
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct Uniforms {
    // Screen resolution in pixels - needed to calculate aspect ratio and scaling
    pub resolution: [f32; 2],

    // Width of all grid lines in pixels (uniform thickness)
    pub line_width: f32,

    // Spectrum area margins (matching UITheme constants)
    pub spectrum_margin_right: f32,
    pub spectrum_margin_bottom: f32,

    // Inset from right edge where grid stops (leaves room for frequency labels)
    pub grid_inset_right: f32,

    // Padding for alignment (ensures struct meets GPU alignment requirements)
    // WGSL uniform buffers require proper alignment - do not remove
    pub _padding: [f32; 2],
}

impl Uniforms {
    pub fn new(bounds: &Rectangle) -> Self {
        Self {
            resolution: [bounds.width, bounds.height],
            line_width: 0.3,         // Line anti-aliasing width (smoothstep falloff distance)
            spectrum_margin_right: 30.0,  // Right margin for frequency labels
            spectrum_margin_bottom: 30.0, // Bottom margin for amplitude labels
            grid_inset_right: 20.0,  // Stop grid 20px before right edge for label space
            _padding: [0.0, 0.0],    // Alignment padding
        }
    }
}

// Storage buffer structure matching the WGSL definition
// This holds metadata about our grid lines
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct GridMetadata {
    pub db_line_count: u32,
    pub freq_line_count: u32,
    // Padding for 16-byte alignment (ensures struct size is multiple of 16)
    // WGSL requires proper alignment - do not remove
    // Matches WGSL: _padding: [u32; 2]
    pub _padding: [u32; 2],
}

// Helper function to build line position data
// Returns (metadata, positions_vec) where positions contains:
// [db_normalized_positions...][freq_normalized_positions...][is_major_flags...]
//
// The flag array structure allows O(1) lookup in the fragment shader to determine
// line type without nested loops, improving per-pixel performance
fn build_grid_data() -> (GridMetadata, Vec<f32>) {
    let mut positions = Vec::new();

    // Add dB line positions (normalized Y values)
    let db_markers = constants::DB_MARKERS;
    for &(db, _) in db_markers {
        let normalized = constants::db_to_normalized(db);
        positions.push(normalized);
    }
    let db_line_count = db_markers.len() as u32;

    // Generate frequency positions with major/minor distinction
    let freq_positions = constants::generate_frequency_grid_positions();

    // First, add all frequency positions
    for &(freq, _is_major) in freq_positions.iter() {
        let log_pos = constants::freq_to_log_position(freq);
        positions.push(log_pos);
    }
    let freq_line_count = freq_positions.len() as u32;

    // Then, add is_major flags as parallel array (1.0 = major, 0.0 = minor)
    // Parallel flag array enables constant-time line type determination in shader
    for &(_freq, is_major) in freq_positions.iter() {
        positions.push(if is_major { 1.0 } else { 0.0 });
    }

    let metadata = GridMetadata {
        db_line_count,
        freq_line_count,
        _padding: [0, 0],
    };

    // Debug: Print first few frequencies and their major status
    nih_plug::nih_log!("Grid data - dB lines: {}, freq lines: {}", db_line_count, freq_line_count);
    nih_plug::nih_log!("First 20 frequencies with major flags:");
    for (idx, &(freq, is_major)) in freq_positions.iter().take(20).enumerate() {
        nih_plug::nih_log!("  [{}] {}Hz - major: {}", idx, freq, is_major);
    }

    // Debug: Print the actual flag array values in the buffer
    let flag_offset = (db_line_count + freq_line_count) as usize;
    nih_plug::nih_log!("Flag array starts at index {} in positions buffer", flag_offset);
    nih_plug::nih_log!("First 20 flag values in buffer:");
    for i in 0..20.min(freq_line_count as usize) {
        nih_plug::nih_log!("  flag[{}] = {}", i, positions[flag_offset + i]);
    }

    // Debug: Print log positions for major frequencies
    nih_plug::nih_log!("Log positions for major frequencies:");
    nih_plug::nih_log!("  100Hz  (idx 8):  log_pos = {}", positions[db_line_count as usize + 8]);
    nih_plug::nih_log!("  1000Hz (idx 17): log_pos = {}", positions[db_line_count as usize + 17]);
    if freq_line_count > 26 {
        nih_plug::nih_log!("  10kHz  (idx 26): log_pos = {}", positions[db_line_count as usize + 26]);
    }

    (metadata, positions)
}

// The Pipeline encapsulates all GPU state needed to render our grid
// Think of it as a "recipe" for the GPU that defines:
// - What shaders to run
// - How to interpret the data
// - What render settings to use
pub struct GridPipeline {
    // The compiled shader program and render state configuration
    render_pipeline: RenderPipeline,

    // GPU buffer that holds our uniform data
    // Buffers are blocks of memory on the GPU
    uniform_buffer: wgpu::Buffer,

    // Storage buffer for grid metadata (used by GPU shader)
    #[allow(dead_code)]
    grid_metadata_buffer: wgpu::Buffer,

    // Storage buffer for line positions (used by GPU shader)
    #[allow(dead_code)]
    line_positions_buffer: wgpu::Buffer,

    // Bind group links our buffers/textures to shader variables
    // It's like connecting wires between CPU data and GPU shader inputs
    bind_group: BindGroup,
}

impl GridPipeline {
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        // Step 1: Compile our WGSL shader code
        // The shader is embedded in the binary using include_str!
        // This happens at compile time, so the shader becomes part of the executable
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grid Shader"),  // Labels help with debugging
            source: wgpu::ShaderSource::Wgsl(include_str!("grid.wgsl").into()),
        });

        // Step 2: Define the layout of resources the shader will access
        // This tells the GPU what kind of data to expect and where to find it
        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Grid Bind Group Layout"),
            entries: &[
                // Binding 0: Uniform buffer for basic parameters
                BindGroupLayoutEntry {
                    binding: 0,  // Matches @binding(0) in shader
                    visibility: ShaderStages::FRAGMENT,  // Only fragment shader needs this
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform,  // It's a uniform buffer (read-only in shader)
                        has_dynamic_offset: false,  // The buffer location doesn't change
                        min_binding_size: None,  // No minimum size requirement
                    },
                    count: None,  // Not an array of buffers
                },
                // Binding 1: Storage buffer for grid metadata
                BindGroupLayoutEntry {
                    binding: 1,  // Matches @binding(1) in shader
                    visibility: ShaderStages::FRAGMENT,  // Only fragment shader needs this
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage {
                            read_only: true  // Read-only storage buffer
                        },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Binding 2: Storage buffer for line positions (dynamic array)
                BindGroupLayoutEntry {
                    binding: 2,  // Matches @binding(2) in shader
                    visibility: ShaderStages::FRAGMENT,  // Only fragment shader needs this
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage {
                            read_only: true  // Read-only storage buffer
                        },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
            ],
        });

        // Step 3: Create pipeline layout
        // This defines the overall structure of resources for the entire pipeline
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Grid Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],  // Can have multiple bind groups
            push_constant_ranges: &[],  // Push constants are another way to pass small data
        });

        // Step 4: Create the render pipeline
        // This is the main configuration that tells the GPU how to render
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Grid Render Pipeline"),
            layout: Some(&pipeline_layout),
            cache: None,

            // Vertex shader configuration
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),  // Function name in WGSL
                buffers: &[],  // No vertex buffers - we generate vertices in shader
                compilation_options: Default::default(),
            },

            // Fragment shader configuration
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"),  // Function name in WGSL
                targets: &[Some(wgpu::ColorTargetState {
                    format,  // Output format (matches screen/window format)
                    // Alpha blending allows transparency
                    // This lets our grid overlay on top of other content
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,  // Write all color channels
                })],
                compilation_options: Default::default(),
            }),

            // Primitive assembly - how vertices form triangles
            primitive: wgpu::PrimitiveState {
                // Triangle strip: each vertex after the first 2 creates a new triangle
                // For 3 vertices: creates 1 fullscreen triangle
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,  // Counter-clockwise winding
                cull_mode: None,  // Don't cull any faces
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,  // Fill triangles, not wireframe
                conservative: false,
            },

            // No depth buffer needed for 2D grid
            depth_stencil: None,

            // Anti-aliasing settings
            multisample: wgpu::MultisampleState {
                count: 1,  // No multisampling
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,  // Not using multiview rendering
        });

        // Step 5: Build grid data from constants
        let (metadata, positions) = build_grid_data();

        // Step 6: Create GPU buffers
        // Uniform buffer for basic parameters
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Grid Uniform Buffer"),
            size: std::mem::size_of::<Uniforms>() as u64,  // Size in bytes
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST,  // Can be updated from CPU
            mapped_at_creation: false,  // Don't map to CPU memory immediately
        });

        // Storage buffer for grid metadata
        let grid_metadata_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Grid Metadata Buffer"),
            size: std::mem::size_of::<GridMetadata>() as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: true,  // Map immediately to write data
        });

        // Write metadata to buffer
        {
            let mut buffer_view = grid_metadata_buffer.slice(..).get_mapped_range_mut();
            buffer_view.copy_from_slice(bytemuck::bytes_of(&metadata));
        }
        grid_metadata_buffer.unmap();

        // Storage buffer for line positions
        let line_positions_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Grid Line Positions Buffer"),
            size: (positions.len() * std::mem::size_of::<f32>()) as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: true,  // Map immediately to write data
        });

        // Write positions to buffer
        {
            let mut buffer_view = line_positions_buffer.slice(..).get_mapped_range_mut();
            buffer_view.copy_from_slice(bytemuck::cast_slice(&positions));
        }
        line_positions_buffer.unmap();

        // Step 7: Create bind group
        // This connects our actual buffers to the bind group layout
        // It's like plugging in the actual data sources
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Grid Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0,  // Uniform buffer
                    resource: uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1,  // Grid metadata storage buffer
                    resource: grid_metadata_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2,  // Line positions storage buffer
                    resource: line_positions_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            render_pipeline,
            uniform_buffer,
            grid_metadata_buffer,
            line_positions_buffer,
            bind_group,
        }
    }

    // Update uniform data when window resizes or settings change
    #[allow(dead_code)]
    pub fn update(&mut self, queue: &Queue, bounds: &Rectangle) {
        self.update_with_bounds(queue, bounds);
    }

    // Update uniform data with current bounds
    pub fn update_with_bounds(&mut self, queue: &Queue, bounds: &Rectangle) {
        // Create new uniforms with current bounds
        let uniforms = Uniforms::new(bounds);

        // Write the uniform data to GPU
        // bytemuck::bytes_of safely converts our struct to raw bytes
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    // Alternative update method that accepts line counts (currently unused)
    // Line counts are determined by constants in build_grid_data()
    #[allow(dead_code)]
    pub fn update_with_lines(&mut self, queue: &Queue, bounds: &Rectangle, _h_lines: u32, _v_lines: u32) {
        self.update_with_bounds(queue, bounds);
    }

    // Render the grid to the screen
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,  // Records GPU commands
        target: &wgpu::TextureView,  // The texture we're rendering to (usually the screen)
        clip_bounds: Rectangle<u32>,  // Scissor rectangle for clipping
    ) {
        // Begin a render pass - this is where actual drawing happens
        // A render pass is a sequence of draw commands that write to the same set of attachments
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Grid Render Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,  // Where to draw
                resolve_target: None,  // No multisampling resolve
                ops: wgpu::Operations {
                    // Load existing content (don't clear) - allows layering
                    load: wgpu::LoadOp::Load,
                    // Store the result
                    store: wgpu::StoreOp::Store,
                },
                depth_slice: None,
            })],
            depth_stencil_attachment: None,  // No depth buffer
            timestamp_writes: None,  // No timing queries
            occlusion_query_set: None,  // No occlusion queries
        });

        // Set scissor rectangle to clip rendering to bounds
        // This prevents drawing outside the widget area
        render_pass.set_scissor_rect(
            clip_bounds.x,
            clip_bounds.y,
            clip_bounds.width,
            clip_bounds.height,
        );

        // Configure the pipeline for this draw call
        render_pass.set_pipeline(&self.render_pipeline);

        // Bind our resources (uniforms) to the pipeline
        render_pass.set_bind_group(0, &self.bind_group, &[]);

        // Draw 3 vertices (forms 1 triangle that covers entire screen)
        // Instance count is 1 (draw once)
        render_pass.draw(0..3, 0..1);
    }
}
