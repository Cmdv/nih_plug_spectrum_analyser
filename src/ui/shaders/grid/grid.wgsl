struct Uniforms {
    resolution: vec2<f32>,
    line_width: f32,
    spectrum_margin_right: f32,
    spectrum_margin_bottom: f32,
    grid_inset_right: f32,
    _padding0: f32,
    _padding1: f32,
}

// Storage buffer for grid line data
// This allows dynamic indexing unlike local arrays
struct GridData {
    // Number of horizontal dB lines
    db_line_count: u32,
    // Number of vertical frequency lines
    freq_line_count: u32,
    // Padding for 16-byte alignment (ensures struct size is multiple of 16)
    // WGSL requires proper alignment - do not remove
    _padding0: u32,
    _padding1: u32,
    // Array of normalized Y positions for dB lines (0.0 to 1.0)
    // Followed by array of normalized X positions for frequency lines
    // Followed by array of major frequency flags (1.0 = major, 0.0 = minor)
    // Data layout: [db_positions...][freq_positions...][is_major_flags...]
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

@group(0) @binding(1)
var<storage, read> grid_data: GridData;

// Access the dynamic arrays in the storage buffer
// Storage buffers support runtime indexing
@group(0) @binding(2)
var<storage, read> line_positions: array<f32>;

struct VertexOutput {
    @builtin(position) position: vec4<f32>,
    @location(0) uv: vec2<f32>,
}

// Generate fullscreen quad vertices
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    // Generate fullscreen triangle (covers entire screen efficiently)
    // This creates a large triangle that covers the whole screen
    let x = f32((vertex_index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vertex_index & 2u) * 2.0 - 1.0;

    out.position = vec4<f32>(x, y, 0.0, 1.0);
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);

    return out;
}

@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Convert UV (0-1) to widget-local pixel coordinates
    // UVs are widget-local, unlike @builtin(position) which is window-absolute
    let pixel_coord = input.uv * uniforms.resolution;

    // Calculate spectrum area
    let spectrum_width = uniforms.resolution.x - uniforms.spectrum_margin_right;
    let spectrum_height = uniforms.resolution.y - uniforms.spectrum_margin_bottom;

    // Calculate grid drawing area
    let grid_max_x = spectrum_width - uniforms.grid_inset_right;

    // Early exit for out-of-bounds pixels
    if pixel_coord.x >= grid_max_x || pixel_coord.y >= spectrum_height {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // Brighter grid lines that are visible on dark backgrounds
    let base_color = vec3<f32>(0.2, 0.2, 0.225);  // 2x brighter gray with slight blue tint

    var final_alpha = 0.0;
    var is_major_line = false;

    // Check for dB horizontal lines using storage buffer
    // Storage buffers allow dynamic indexing, unlike local arrays
    for (var i = 0u; i < grid_data.db_line_count; i++) {
        let normalized = line_positions[i];
        let line_y = spectrum_height * (1.0 - normalized);

        // Exact 1-pixel line: only the pixel containing the line center
        // floor comparison ensures consistent single-pixel width regardless of sub-pixel position
        let line_contrib = select(0.0, 1.0, floor(pixel_coord.y) == floor(line_y));
        if line_contrib > 0.0 {
            // dB lines are major lines with higher opacity
            final_alpha = max(final_alpha, line_contrib * 0.5);  // Increased from 0.25
            is_major_line = true;
        }
    }

    // Check for frequency vertical lines using storage buffer
    let freq_start = grid_data.db_line_count;
    let major_start = freq_start + grid_data.freq_line_count;

    for (var i = 0u; i < grid_data.freq_line_count; i++) {
        let log_pos = line_positions[freq_start + i];
        let line_x = log_pos * spectrum_width;

        // Exact 1-pixel line: only the pixel containing the line center
        // floor comparison ensures consistent single-pixel width regardless of sub-pixel position
        let line_contrib = select(0.0, 1.0, floor(pixel_coord.x) == floor(line_x));
        if line_contrib > 0.0 {
            // Direct O(1) flag lookup: 1.0 = major line, 0.0 = minor line
            let is_major_flag = line_positions[major_start + i];
            let is_major = is_major_flag >= 0.9;

            // Select opacity based on major/minor status
            let alpha = select(0.25, 0.7, is_major);  // Increased: minor 0.25, major 0.7
            let contrib_alpha = line_contrib * alpha;

            // Take maximum alpha to handle intersections correctly
            final_alpha = max(final_alpha, contrib_alpha);
            if is_major {
                is_major_line = true;
            }
        }
    }

    // Return final color with accumulated alpha
    if final_alpha > 0.0 {
        return vec4<f32>(base_color, final_alpha);
    }

    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}
