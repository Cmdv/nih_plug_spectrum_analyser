use crate::audio::constants;
use bytemuck::{Pod, Zeroable};
use nih_plug_iced::renderer::wgpu::wgpu::{
    self as wgpu, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingType, BufferBindingType, BufferUsages, Device, Queue,
    RenderPipeline, ShaderStages, TextureFormat,
};
use nih_plug_iced::renderer::wgpu::primitive::Pipeline;
use nih_plug_iced::Rectangle;

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
            line_width: 0.8, // Line anti-aliasing width (smoothstep falloff distance)
            spectrum_margin_right: 30.0, // Right margin for frequency labels
            spectrum_margin_bottom: 30.0, // Bottom margin for amplitude labels
            grid_inset_right: 20.0, // Stop grid 20px before right edge for label space
            _padding: [0.0, 0.0], // Alignment padding
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

    // Blit pipeline to copy cached texture to screen
    blit_pipeline: RenderPipeline,
    blit_sampler: wgpu::Sampler,
    blit_bind_group_layout: wgpu::BindGroupLayout,
    blit_bind_group: Option<BindGroup>,

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

    // Cached texture that holds the pre-rendered grid
    // This texture is only re-rendered when size changes, dramatically reducing GPU usage
    cached_texture: Option<wgpu::Texture>,
    cached_texture_view: Option<wgpu::TextureView>,

    // Texture format for the cached texture
    texture_format: TextureFormat,

    // Cache to avoid unnecessary buffer updates and texture re-renders
    // Only update GPU buffer when bounds or size actually changes
    last_bounds: Option<Rectangle>,
    last_physical_size: Option<nih_plug_iced::Size<u32>>,

    // Flag to track if we need to re-render the cached texture
    needs_redraw: bool,
}

impl GridPipeline {
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        // Step 1: Compile our WGSL shader code
        // The shader is embedded in the binary using include_str!
        // This happens at compile time, so the shader becomes part of the executable
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grid Shader"), // Labels help with debugging
            source: wgpu::ShaderSource::Wgsl(include_str!("grid.wgsl").into()),
        });

        // Step 2: Define the layout of resources the shader will access
        // This tells the GPU what kind of data to expect and where to find it
        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Grid Bind Group Layout"),
            entries: &[
                // Binding 0: Uniform buffer for basic parameters
                BindGroupLayoutEntry {
                    binding: 0,                         // Matches @binding(0) in shader
                    visibility: ShaderStages::FRAGMENT, // Only fragment shader needs this
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Uniform, // It's a uniform buffer (read-only in shader)
                        has_dynamic_offset: false,      // The buffer location doesn't change
                        min_binding_size: None,         // No minimum size requirement
                    },
                    count: None, // Not an array of buffers
                },
                // Binding 1: Storage buffer for grid metadata
                BindGroupLayoutEntry {
                    binding: 1,                         // Matches @binding(1) in shader
                    visibility: ShaderStages::FRAGMENT, // Only fragment shader needs this
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage {
                            read_only: true, // Read-only storage buffer
                        },
                        has_dynamic_offset: false,
                        min_binding_size: None,
                    },
                    count: None,
                },
                // Binding 2: Storage buffer for line positions (dynamic array)
                BindGroupLayoutEntry {
                    binding: 2,                         // Matches @binding(2) in shader
                    visibility: ShaderStages::FRAGMENT, // Only fragment shader needs this
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage {
                            read_only: true, // Read-only storage buffer
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
            bind_group_layouts: &[&bind_group_layout], // Can have multiple bind groups
            push_constant_ranges: &[], // Push constants are another way to pass small data
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
                entry_point: Some("vs_main"), // Function name in WGSL
                buffers: &[],                 // No vertex buffers - we generate vertices in shader
                compilation_options: Default::default(),
            },

            // Fragment shader configuration
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_main"), // Function name in WGSL
                targets: &[Some(wgpu::ColorTargetState {
                    format, // Output format (matches screen/window format)
                    // Alpha blending allows transparency
                    // This lets our grid overlay on top of other content
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL, // Write all color channels
                })],
                compilation_options: Default::default(),
            }),

            // Primitive assembly - how vertices form triangles
            primitive: wgpu::PrimitiveState {
                // Triangle strip: each vertex after the first 2 creates a new triangle
                // For 3 vertices: creates 1 fullscreen triangle
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw, // Counter-clockwise winding
                cull_mode: None,                  // Don't cull any faces
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill, // Fill triangles, not wireframe
                conservative: false,
            },

            // No depth buffer needed for 2D grid
            depth_stencil: None,

            // Anti-aliasing settings
            multisample: wgpu::MultisampleState {
                count: 1, // No multisampling
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None, // Not using multiview rendering
        });

        // Step 5: Create blit shader and pipeline for copying cached texture to screen
        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Grid Blit Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("blit.wgsl").into()),
        });

        // Create sampler for texture sampling
        let blit_sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Grid Blit Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        // Create bind group layout for blit (texture + sampler)
        let blit_bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Grid Blit Bind Group Layout"),
            entries: &[
                // Binding 0: Texture
                BindGroupLayoutEntry {
                    binding: 0,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                // Binding 1: Sampler
                BindGroupLayoutEntry {
                    binding: 1,
                    visibility: ShaderStages::FRAGMENT,
                    ty: BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        // Create pipeline layout for blit
        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Grid Blit Pipeline Layout"),
            bind_group_layouts: &[&blit_bind_group_layout],
            push_constant_ranges: &[],
        });

        // Create blit render pipeline
        let blit_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Grid Blit Render Pipeline"),
            layout: Some(&blit_pipeline_layout),
            cache: None,
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: Default::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleStrip,
                strip_index_format: None,
                front_face: wgpu::FrontFace::Ccw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState {
                count: 1,
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None,
        });

        // Step 6: Build grid data from constants
        let (metadata, positions) = build_grid_data();

        // Step 6: Create GPU buffers
        // Uniform buffer for basic parameters
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Grid Uniform Buffer"),
            size: std::mem::size_of::<Uniforms>() as u64, // Size in bytes
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST, // Can be updated from CPU
            mapped_at_creation: false,                    // Don't map to CPU memory immediately
        });

        // Storage buffer for grid metadata
        let grid_metadata_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Grid Metadata Buffer"),
            size: std::mem::size_of::<GridMetadata>() as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: true, // Map immediately to write data
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
            mapped_at_creation: true, // Map immediately to write data
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
                    binding: 0, // Uniform buffer
                    resource: uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1, // Grid metadata storage buffer
                    resource: grid_metadata_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2, // Line positions storage buffer
                    resource: line_positions_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            render_pipeline,
            blit_pipeline,
            blit_sampler,
            blit_bind_group_layout,
            blit_bind_group: None,
            uniform_buffer,
            grid_metadata_buffer,
            line_positions_buffer,
            bind_group,
            cached_texture: None,
            cached_texture_view: None,
            texture_format: format,
            last_bounds: None,
            last_physical_size: None,
            needs_redraw: true, // Force initial render
        }
    }

    // Update uniform data when window resizes or settings change
    #[allow(dead_code)]
    pub fn update(&mut self, queue: &Queue, bounds: &Rectangle) {
        self.update_with_bounds(queue, bounds);
    }

    // Update uniform data with current bounds (logical size)
    // Note: This may cause zooming if scale factor changes
    pub fn update_with_bounds(&mut self, queue: &Queue, bounds: &Rectangle) {
        // Create new uniforms with current bounds
        let uniforms = Uniforms::new(bounds);

        // Write the uniform data to GPU
        // bytemuck::bytes_of safely converts our struct to raw bytes
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    // Update uniform data with physical size for accurate rendering
    // This is the preferred method to avoid zoom/scale issues
    pub fn update_with_physical_size(
        &mut self,
        queue: &Queue,
        bounds: &Rectangle,
        physical_size: nih_plug_iced::Size<u32>,
    ) {
        // Check if anything has changed - if not, skip the GPU buffer update
        // This optimization prevents unnecessary buffer writes when the grid is static
        let bounds_changed = self.last_bounds.map_or(true, |last| {
            last.x != bounds.x
                || last.y != bounds.y
                || last.width != bounds.width
                || last.height != bounds.height
        });

        let size_changed = self.last_physical_size.map_or(true, |last| {
            last.width != physical_size.width || last.height != physical_size.height
        });

        // Skip update if nothing changed - this is the key optimization
        // Prevents writing to GPU buffer 60 times per second when grid is static
        if !bounds_changed && !size_changed {
            return;
        }

        // Cache the current values for next frame comparison
        self.last_bounds = Some(*bounds);
        self.last_physical_size = Some(physical_size);

        // Mark that we need to re-render the cached texture
        self.needs_redraw = true;

        // Calculate scale factor from physical vs logical size
        let scale_x = physical_size.width as f32 / bounds.width;
        let scale_y = physical_size.height as f32 / bounds.height;

        // Use physical dimensions for resolution (what the GPU actually renders)
        // Scale the margins proportionally to maintain their logical sizes
        let uniforms = Uniforms {
            resolution: [physical_size.width as f32, physical_size.height as f32],
            line_width: 0.8,
            // Scale margins from logical to physical space
            spectrum_margin_right: 30.0 * scale_x,
            spectrum_margin_bottom: 30.0 * scale_y,
            grid_inset_right: 20.0 * scale_x,
            _padding: [0.0, 0.0],
        };

        // Write the uniform data to GPU
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));
    }

    // Alternative update method that accepts line counts (currently unused)
    // Line counts are determined by constants in build_grid_data()
    #[allow(dead_code)]
    pub fn update_with_lines(
        &mut self,
        queue: &Queue,
        bounds: &Rectangle,
        _h_lines: u32,
        _v_lines: u32,
    ) {
        self.update_with_bounds(queue, bounds);
    }

    // Create or recreate the cached texture when size changes
    fn ensure_cached_texture(&mut self, device: &Device, physical_size: nih_plug_iced::Size<u32>) {
        // Check if we need to create a new texture (size changed or doesn't exist)
        let needs_new_texture = self.cached_texture.as_ref().map_or(true, |texture| {
            texture.width() != physical_size.width || texture.height() != physical_size.height
        });

        if needs_new_texture {
            // Create a new cached texture with the current size
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("Grid Cached Texture"),
                size: wgpu::Extent3d {
                    width: physical_size.width,
                    height: physical_size.height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: self.texture_format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });

            let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

            // Store texture and view first
            self.cached_texture = Some(texture);
            self.cached_texture_view = Some(view);

            // Create new blit bind group using the stored view
            let blit_bind_group = device.create_bind_group(&BindGroupDescriptor {
                label: Some("Grid Blit Bind Group"),
                layout: &self.blit_bind_group_layout,
                entries: &[
                    BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(
                            self.cached_texture_view.as_ref().unwrap(),
                        ),
                    },
                    BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&self.blit_sampler),
                    },
                ],
            });

            self.blit_bind_group = Some(blit_bind_group);
            self.needs_redraw = true; // Force redraw with new texture
        }
    }

    // Public method to be called from prepare() - ensures cache is ready
    pub fn prepare_cache(
        &mut self,
        device: &Device,
        queue: &Queue,
        physical_size: nih_plug_iced::Size<u32>,
    ) {
        self.render_to_cache(device, queue, physical_size);
    }

    // Render the grid to the cached texture (only when needed)
    fn render_to_cache(
        &mut self,
        device: &Device,
        queue: &Queue,
        physical_size: nih_plug_iced::Size<u32>,
    ) {
        // Ensure cached texture exists and has the right size
        self.ensure_cached_texture(device, physical_size);

        // Only render if we need to
        if !self.needs_redraw {
            return;
        }

        // Get the cached texture view (we just ensured it exists)
        let texture_view = self.cached_texture_view.as_ref().unwrap();

        // Create a command encoder for this render operation
        let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("Grid Cache Render Encoder"),
        });

        // Render the grid to the cached texture
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Grid Cache Render Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: texture_view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::TRANSPARENT),
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            // Set scissor to full texture size
            render_pass.set_scissor_rect(0, 0, physical_size.width, physical_size.height);
            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }

        // Submit the render commands
        queue.submit(Some(encoder.finish()));

        // Mark that we've completed the redraw
        self.needs_redraw = false;
    }

    // Render the grid to the screen by blitting the cached texture
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder, // Records GPU commands
        target: &wgpu::TextureView,         // The texture we're rendering to (usually the screen)
        clip_bounds: Rectangle<u32>,        // Scissor rectangle for clipping
    ) {
        // If we have a cached texture, blit it to the target
        // Otherwise, fall back to direct rendering (shouldn't happen in normal operation)
        if let Some(blit_bind_group) = &self.blit_bind_group {
            // Blit the cached grid texture to the screen
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Grid Blit Pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load, // Preserve existing content
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_scissor_rect(
                clip_bounds.x,
                clip_bounds.y,
                clip_bounds.width,
                clip_bounds.height,
            );

            // Use blit pipeline to sample from cached texture
            render_pass.set_pipeline(&self.blit_pipeline);
            render_pass.set_bind_group(0, blit_bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        } else {
            // Fallback: render directly (shouldn't happen after prepare_cache is called)
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Grid Render Pass (Fallback)"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                    depth_slice: None,
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
            });

            render_pass.set_scissor_rect(
                clip_bounds.x,
                clip_bounds.y,
                clip_bounds.width,
                clip_bounds.height,
            );

            render_pass.set_pipeline(&self.render_pipeline);
            render_pass.set_bind_group(0, &self.bind_group, &[]);
            render_pass.draw(0..3, 0..1);
        }
    }
}

impl Pipeline for GridPipeline {
    fn new(device: &Device, _queue: &Queue, format: TextureFormat) -> Self {
        GridPipeline::new(device, format)
    }
}
