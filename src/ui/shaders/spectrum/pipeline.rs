use crate::audio::constants;
use bytemuck::{Pod, Zeroable};
use nih_plug_iced::renderer::wgpu::wgpu::{
    self as wgpu, BindGroup, BindGroupDescriptor, BindGroupEntry, BindGroupLayoutDescriptor,
    BindGroupLayoutEntry, BindingType, BufferBindingType, BufferUsages, Device, Queue,
    RenderPipeline, ShaderStages, TextureFormat,
};
use nih_plug_iced::renderer::wgpu::primitive::Pipeline;
use nih_plug_iced::Rectangle;

/// Apply elastic easing to time for bouncy, spring-like animation
/// Audio-reactive: triggers elastic bounce when audio peaks are detected
/// Returns modified time value with bounce effect applied
fn apply_elastic_bounce(raw_time: f32, time_since_peak: Option<f32>) -> f32 {
    // Duration of the elastic bounce animation after a peak
    let bounce_duration = 1.0;

    if let Some(time_since) = time_since_peak {
        // We detected a peak recently - apply elastic bounce if still within duration
        if time_since < bounce_duration {
            let cycle_time = time_since / bounce_duration; // Normalize to 0-1

            // Elastic out easing parameters
            let p = 0.4; // Period of the sine wave (controls number of oscillations)
            let a = 1.0; // Amplitude

            let eased = if cycle_time == 0.0 {
                0.0
            } else {
                a * f32::powf(2.0, -10.0 * cycle_time)
                    * f32::sin((cycle_time - p / 4.0) * (2.0 * std::f32::consts::PI) / p)
                    + 1.0
            };

            // Add bounce offset to time (creates the visual "pop" effect)
            return raw_time + eased * 0.5;
        }
    }

    // No recent peak or bounce completed - return normal time
    raw_time
}

/// Compute average spectrum energy for audio-reactive color brightness
/// Takes log-sampled spectrum data in dB and returns normalized energy (0-1)
/// This matches the shader's get_average_spectrum_energy() but runs once on CPU
pub fn compute_spectrum_energy(log_sampled_data: &[f32], min_db: f32, max_db: f32) -> f32 {
    if log_sampled_data.is_empty() {
        return 0.0;
    }

    let db_range = max_db - min_db;
    let sum: f32 = log_sampled_data
        .iter()
        .map(|&db| {
            // Normalize dB to 0-1 range (same as shader's db_to_normalized)
            let normalized = (db - min_db) / db_range;
            normalized.clamp(0.0, 1.0)
        })
        .sum();

    sum / log_sampled_data.len() as f32
}

/// Compute mid-range frequency energy for peak detection
/// Mid-range (200Hz-2000Hz) is most perceptually important for detecting musical peaks
/// Returns normalized energy (0-1) averaged over mid-range bins
fn compute_midrange_energy(log_sampled_data: &[f32], min_db: f32, max_db: f32) -> f32 {
    if log_sampled_data.is_empty() {
        return 0.0;
    }

    // Log-sampled data spans 20Hz to 20kHz
    // We want approximately 200Hz-2000Hz range
    // Calculate indices for this range
    let min_freq = constants::MIN_FREQUENCY; // 20Hz
    let max_freq = constants::MAX_FREQUENCY; // 20000Hz
    let total_bins = log_sampled_data.len();

    // Calculate normalized positions in log space for 200Hz and 2000Hz
    let midrange_start_freq = 200.0;
    let midrange_end_freq = 2000.0;

    let start_norm = (midrange_start_freq / min_freq).ln() / (max_freq / min_freq).ln();
    let end_norm = (midrange_end_freq / min_freq).ln() / (max_freq / min_freq).ln();

    let start_idx = (start_norm * total_bins as f32) as usize;
    let end_idx = (end_norm * total_bins as f32).min(total_bins as f32) as usize;

    if start_idx >= end_idx || end_idx > log_sampled_data.len() {
        return 0.0;
    }

    // Average energy in mid-range bins
    let db_range = max_db - min_db;
    let sum: f32 = log_sampled_data[start_idx..end_idx]
        .iter()
        .map(|&db| {
            let normalized = (db - min_db) / db_range;
            normalized.clamp(0.0, 1.0)
        })
        .sum();

    sum / (end_idx - start_idx) as f32
}

/// Transform raw FFT bins into logarithmically-sampled display data
/// This matches the CPU renderer's approach: sample at log-spaced frequencies,
/// interpolate dB values from FFT bins at those frequencies
///
/// Returns: Vec of dB values sampled at logarithmic frequencies (20Hz to 20kHz)
/// The shader then draws these linearly (since they're already log-sampled)
pub fn compute_log_sampled_spectrum(fft_bins: &[f32], sample_rate: f32) -> Vec<f32> {
    if fft_bins.len() < 2 {
        return Vec::new();
    }

    let num_display_points = fft_bins.len();
    let min_freq = constants::MIN_FREQUENCY;
    let max_freq = constants::MAX_FREQUENCY;

    (0..num_display_points)
        .map(|i| {
            // Calculate logarithmic frequency for this display point
            // Same as spectrum_display.rs::calculate_log_frequency
            let norm_pos = i as f32 / num_display_points as f32;
            let frequency = min_freq * (max_freq / min_freq).powf(norm_pos);

            // Interpolate dB value from FFT bins at this frequency
            // Same as spectrum_display.rs::interpolate_bin_value
            let nyquist_frequency = sample_rate / 2.0;
            let bin_position = (frequency / nyquist_frequency) * (fft_bins.len() - 1) as f32;
            let bin_index = bin_position.floor() as usize;
            let bin_fraction = bin_position.fract();

            if bin_index + 1 < fft_bins.len() {
                // Linear interpolation between two bins
                let current_bin = fft_bins[bin_index];
                let next_bin = fft_bins[bin_index + 1];
                current_bin + (next_bin - current_bin) * bin_fraction
            } else if bin_index < fft_bins.len() {
                fft_bins[bin_index]
            } else {
                -100.0 // Out of range
            }
        })
        .collect()
}

// Uniforms are data passed from CPU to GPU that remain constant during a draw call
// They're used for screen resolution, margins, amplitude range, etc.
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
    pub spectrum_resolution: [f32; 2],

    // Spectrum area margins (matching UITheme constants)
    pub spectrum_margin_right: f32,
    pub spectrum_margin_bottom: f32,

    // Amplitude range in dB for vertical scaling
    pub min_db: f32,
    pub max_db: f32,

    // Time in seconds for particle animation
    pub time: f32,

    // OPTIMIZATION: Precomputed average spectrum energy (0-1 normalized)
    // Previously calculated in shader for every pixel - now computed once on CPU
    // Controls color brightness based on audio loudness
    pub spectrum_energy: f32,

    // User-controlled animation speed multiplier
    // Controls how fast the fluid particles move
    pub animation_speed: f32,

    // Padding for alignment (ensures struct meets GPU alignment requirements)
    // WGSL uniform buffers require proper alignment - do not remove
    pub _padding: f32,
}

// Storage buffer structure for spectrum metadata
// This tells the shader how many frequency bins we have
#[repr(C)]
#[derive(Debug, Copy, Clone, Pod, Zeroable)]
pub struct SpectrumMetadata {
    pub bin_count: u32,
    // Padding for 16-byte alignment (ensures struct size is multiple of 16)
    // WGSL requires proper alignment - do not remove
    // Matches WGSL: _padding0, _padding1, _padding2
    pub _padding0: u32,
    pub _padding1: u32,
    pub _padding2: u32,
}

// The Pipeline encapsulates all GPU state needed to render our spectrum
// Think of it as a "recipe" for the GPU that defines:
// - What shaders to run
// - How to interpret the data
// - What render settings to use
pub struct SpectrumPipeline {
    // The compiled shader program and render state configuration
    // Now renders BOTH spectrum line and fluid fill in a single pass
    render_pipeline: RenderPipeline,

    // OPTIMIZATION: Compute shader for particle position calculation
    particle_compute_pipeline: wgpu::ComputePipeline,

    // GPU buffer that holds our uniform data (resolution, margins, dB range)
    uniform_buffer: wgpu::Buffer,

    // Storage buffer for spectrum metadata (bin count)
    spectrum_metadata_buffer: wgpu::Buffer,

    // Storage buffer for spectrum bin values (dB magnitudes)
    // This is dynamically sized based on the number of bins
    spectrum_bins_buffer: wgpu::Buffer,

    // Bind group links our buffers/textures to shader variables
    // It's like connecting wires between CPU data and GPU shader inputs
    bind_group: BindGroup,

    // Compute bind group for particle position calculation
    compute_bind_group: BindGroup,

    // Track buffer capacity to avoid recreating it every frame
    current_bin_capacity: usize,

    // Track start time for animation
    start_time: std::time::Instant,

    // Number of particles for Voronoi cells
    particle_count: u32,

    // Peak detection state for audio-reactive bouncing
    prev_midrange_energy: f32,
    last_peak_time: Option<f32>, // Time (in seconds) of last detected peak
}

impl SpectrumPipeline {
    pub fn new(device: &Device, format: TextureFormat) -> Self {
        // Step 1: Compile our WGSL shader code
        // The shader is embedded in the binary using include_str!
        // This happens at compile time, so the shader becomes part of the executable
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Spectrum Shader"), // Labels help with debugging
            source: wgpu::ShaderSource::Wgsl(include_str!("spectrum.wgsl").into()),
        });

        // Step 2: Define the layout of resources the shader will access
        // This tells the GPU what kind of data to expect and where to find it
        let bind_group_layout = device.create_bind_group_layout(&BindGroupLayoutDescriptor {
            label: Some("Spectrum Bind Group Layout"),
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
                // Binding 1: Storage buffer for spectrum metadata
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
                // Binding 2: Storage buffer for spectrum bin values (dynamic array)
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
                // Binding 3: OPTIMIZATION - Precomputed particle positions from compute shader
                BindGroupLayoutEntry {
                    binding: 3,                         // Matches @binding(3) in fluid shader
                    visibility: ShaderStages::FRAGMENT, // Only fluid fragment shader needs this
                    ty: BindingType::Buffer {
                        ty: BufferBindingType::Storage {
                            read_only: true, // Read-only in fragment shader (written by compute)
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
            label: Some("Spectrum Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout], // Can have multiple bind groups
            push_constant_ranges: &[], // Push constants are another way to pass small data
        });

        // Step 4: Create the render pipeline
        // This is the main configuration that tells the GPU how to render
        let render_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Spectrum Render Pipeline"),
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
                    // Alpha blending allows transparency and additive glow effects
                    // This lets our spectrum overlay on top of other content
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

            // No depth buffer needed for 2D spectrum
            depth_stencil: None,

            // Anti-aliasing settings
            multisample: wgpu::MultisampleState {
                count: 1, // No multisampling (shader handles anti-aliasing)
                mask: !0,
                alpha_to_coverage_enabled: false,
            },
            multiview: None, // Not using multiview rendering
        });

        // Step 5: Create GPU buffers
        // Uniform buffer for basic parameters
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Spectrum Uniform Buffer"),
            size: std::mem::size_of::<Uniforms>() as u64, // Size in bytes
            usage: BufferUsages::UNIFORM | BufferUsages::COPY_DST, // Can be updated from CPU
            mapped_at_creation: false,                    // Don't map to CPU memory immediately
        });

        // Storage buffer for spectrum metadata
        let spectrum_metadata_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Spectrum Metadata Buffer"),
            size: std::mem::size_of::<SpectrumMetadata>() as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false, // We'll update this when we have data
        });

        // Storage buffer for spectrum bins (initial size, will grow as needed)
        // Maximum resolution can have up to ~2049 bins, so use 4096 for safety
        let initial_capacity = 4096;
        let spectrum_bins_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Spectrum Bins Buffer"),
            size: (initial_capacity * std::mem::size_of::<f32>()) as u64,
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // OPTIMIZATION: Create particle positions storage buffer
        // Number of particles for Voronoi cells
        let particle_count = 50u32; // Currently testing with 10 particles
        let particle_positions_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Particle Positions Buffer"),
            size: (particle_count as u64 * std::mem::size_of::<[f32; 2]>() as u64),
            usage: BufferUsages::STORAGE | BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Step 6: Create bind group
        // This connects our actual buffers to the bind group layout
        // It's like plugging in the actual data sources
        let bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Spectrum Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0, // Uniform buffer
                    resource: uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1, // Spectrum metadata storage buffer
                    resource: spectrum_metadata_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2, // Spectrum bins storage buffer
                    resource: spectrum_bins_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3, // OPTIMIZATION: Particle positions buffer
                    resource: particle_positions_buffer.as_entire_binding(),
                },
            ],
        });

        // Step 7: Create particle compute shader and pipeline
        let particle_compute_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Particle Compute Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("particle_compute.wgsl").into()),
        });

        // Create compute bind group layout
        let compute_bind_group_layout =
            device.create_bind_group_layout(&BindGroupLayoutDescriptor {
                label: Some("Particle Compute Bind Group Layout"),
                entries: &[
                    // Binding 0: Uniform buffer (same as render shaders)
                    BindGroupLayoutEntry {
                        binding: 0,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Binding 1: Particle positions storage buffer (read_write for compute)
                    BindGroupLayoutEntry {
                        binding: 1,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Storage {
                                read_only: false, // Compute shader writes to this
                            },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Binding 2: Spectrum metadata storage buffer (read-only for compute)
                    // EDGE-AWARE OPTIMIZATION: Allows particles to read spectrum boundary
                    BindGroupLayoutEntry {
                        binding: 2,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Storage {
                                read_only: true, // Compute shader reads spectrum metadata
                            },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                    // Binding 3: Spectrum bins storage buffer (read-only for compute)
                    // EDGE-AWARE OPTIMIZATION: Allows particles to constrain to spectrum line
                    BindGroupLayoutEntry {
                        binding: 3,
                        visibility: ShaderStages::COMPUTE,
                        ty: BindingType::Buffer {
                            ty: BufferBindingType::Storage {
                                read_only: true, // Compute shader reads spectrum bins for boundary constraint
                            },
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    },
                ],
            });

        // Create compute pipeline layout
        let compute_pipeline_layout =
            device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Particle Compute Pipeline Layout"),
                bind_group_layouts: &[&compute_bind_group_layout],
                push_constant_ranges: &[],
            });

        // Create compute pipeline
        let particle_compute_pipeline =
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some("Particle Compute Pipeline"),
                layout: Some(&compute_pipeline_layout),
                module: &particle_compute_shader,
                entry_point: Some("main"),
                cache: None,
                compilation_options: Default::default(),
            });

        // Create compute bind group
        // EDGE-AWARE: Now includes spectrum bins and metadata for boundary constraints
        let compute_bind_group = device.create_bind_group(&BindGroupDescriptor {
            label: Some("Particle Compute Bind Group"),
            layout: &compute_bind_group_layout,
            entries: &[
                BindGroupEntry {
                    binding: 0, // Uniform buffer
                    resource: uniform_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 1, // Particle positions buffer (read_write)
                    resource: particle_positions_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 2, // Spectrum metadata buffer (read-only)
                    resource: spectrum_metadata_buffer.as_entire_binding(),
                },
                BindGroupEntry {
                    binding: 3, // Spectrum bins buffer (read-only)
                    resource: spectrum_bins_buffer.as_entire_binding(),
                },
            ],
        });

        Self {
            render_pipeline,
            particle_compute_pipeline,
            uniform_buffer,
            spectrum_metadata_buffer,
            spectrum_bins_buffer,
            bind_group,
            compute_bind_group,
            current_bin_capacity: initial_capacity,
            start_time: std::time::Instant::now(),
            particle_count,
            prev_midrange_energy: 0.0,
            last_peak_time: None,
        }
    }

    // Update uniform data and spectrum bins with physical size for accurate rendering
    // This is the main update method called every frame from prepare()
    // Takes log-sampled display data, dB range, and animation speed
    pub fn update_with_physical_size(
        &mut self,
        _device: &Device,
        queue: &Queue,
        bounds: &Rectangle,
        physical_size: nih_plug_iced::Size<u32>,
        log_sampled_data: &[f32],
        min_db: f32,
        max_db: f32,
        animation_speed: f32,
    ) {
        // Calculate scale factor from physical vs logical size
        let scale_x = physical_size.width as f32 / bounds.width;
        let scale_y = physical_size.height as f32 / bounds.height;

        // Calculate elapsed time in seconds for particle animation
        let raw_time = self.start_time.elapsed().as_secs_f32();

        // PEAK DETECTION: Calculate mid-range energy for audio-reactive bouncing
        let current_midrange_energy = compute_midrange_energy(log_sampled_data, min_db, max_db);

        // Detect drum hit ATTACK (rising edge) - this gives perfect timing with snare/kick hits
        // Sharp rise in energy indicates the moment of impact
        const PEAK_THRESHOLD: f32 = 0.3; // Minimum energy level to consider a peak
        const RISE_THRESHOLD: f32 = 0.08; // Minimum energy increase to detect attack (tune this for sensitivity)

        let energy_rise = current_midrange_energy - self.prev_midrange_energy;
        let should_trigger_peak =
            energy_rise > RISE_THRESHOLD && current_midrange_energy > PEAK_THRESHOLD;

        // NEW HITS OVERRIDE: No cooldown! Let drum hits immediately restart the bounce
        // This gives responsive, punchy animation that follows fast drum patterns
        if should_trigger_peak {
            self.last_peak_time = Some(raw_time); // Immediately reset - overrides existing bounce
        }

        // Calculate time since last peak for elastic bounce
        let time_since_peak = self.last_peak_time.map(|peak_time| raw_time - peak_time);

        // Apply audio-reactive elastic bounce when peaks are detected
        let time = apply_elastic_bounce(raw_time, time_since_peak);

        // Update previous energy for next frame's peak detection
        self.prev_midrange_energy = current_midrange_energy;

        // OPTIMIZATION: Compute spectrum energy once on CPU instead of per-pixel on GPU
        let spectrum_energy = compute_spectrum_energy(log_sampled_data, min_db, max_db);

        let uniforms = Uniforms {
            spectrum_resolution: [physical_size.width as f32, physical_size.height as f32],
            // Scale margins from logical to physical space
            spectrum_margin_right: 30.0 * scale_x,
            spectrum_margin_bottom: 30.0 * scale_y,
            min_db,
            max_db,
            time,
            spectrum_energy,
            animation_speed,
            _padding: 0.0,
        };

        // Write the uniform data to GPU
        // bytemuck::bytes_of safely converts our struct to raw bytes
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        // Update spectrum bin data - clamp to buffer capacity to prevent overflow
        // Data is already logarithmically-sampled by editor.rs
        if !log_sampled_data.is_empty() {
            // Determine how much data we can actually write (clamp to buffer capacity)
            let write_count = log_sampled_data.len().min(self.current_bin_capacity);

            // Warn if we're dropping data due to buffer size limitation
            if log_sampled_data.len() > self.current_bin_capacity {
                eprintln!(
                    "Warning: Spectrum buffer capacity exceeded ({} bins > {} capacity). Clamping to buffer size.",
                    log_sampled_data.len(),
                    self.current_bin_capacity
                );
            }

            // Write only what fits in the buffer (prevents buffer overflow)
            queue.write_buffer(
                &self.spectrum_bins_buffer,
                0,
                bytemuck::cast_slice(&log_sampled_data[..write_count]),
            );

            // Update metadata to reflect actual number of bins written
            // This ensures the shader reads the correct amount of data
            let metadata = SpectrumMetadata {
                bin_count: write_count as u32,
                _padding0: 0,
                _padding1: 0,
                _padding2: 0,
            };

            queue.write_buffer(
                &self.spectrum_metadata_buffer,
                0,
                bytemuck::bytes_of(&metadata),
            );
        }

        // Fluid shader is stateless - no update needed!
        // All animation driven by uniforms.time
    }

    // Render the spectrum to the screen
    pub fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder, // Records GPU commands
        target: &wgpu::TextureView,         // The texture we're rendering to (usually the screen)
        clip_bounds: Rectangle<u32>,        // Scissor rectangle for clipping
        _bounds: &Rectangle,
    ) {
        // STEP 1: OPTIMIZATION - Dispatch compute shader to calculate particle positions
        // This runs ONCE per frame, not per-pixel!
        {
            let mut compute_pass = encoder.begin_compute_pass(&wgpu::ComputePassDescriptor {
                label: Some("Particle Position Compute Pass"),
                timestamp_writes: None,
            });

            compute_pass.set_pipeline(&self.particle_compute_pipeline);
            compute_pass.set_bind_group(0, &self.compute_bind_group, &[]);

            // Dispatch compute shader - workgroup size is 64, so calculate workgroups needed
            let workgroup_count = (self.particle_count + 63) / 64; // Ceiling division
            compute_pass.dispatch_workgroups(workgroup_count, 1, 1);
        } // compute_pass dropped here

        // STEP 2: Render combined spectrum (fluid fill + line) in a single pass
        {
            let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("Spectrum Render Pass"),
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
        } // render_pass dropped here
    }
}

impl Pipeline for SpectrumPipeline {
    fn new(device: &Device, _queue: &Queue, format: TextureFormat) -> Self {
        SpectrumPipeline::new(device, format)
    }
}
