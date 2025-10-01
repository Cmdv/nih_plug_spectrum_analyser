struct Uniforms {
    resolution: vec2<f32>,
    line_width: f32,
    spectrum_margin_right: f32,
    spectrum_margin_bottom: f32,
    padding: f32,
}

// Storage buffer for grid line data
// This allows dynamic indexing unlike local arrays
struct GridData {
    // Number of horizontal dB lines
    db_line_count: u32,
    // Number of vertical frequency lines
    freq_line_count: u32,
    // Number of major frequency lines
    major_freq_count: u32,
    // Padding for 16-byte alignment
    _padding: u32,
    // Array of normalized Y positions for dB lines (0.0 to 1.0)
    // Followed by array of normalized X positions for frequency lines
    // Followed by array of major frequency indices
    // Data layout: [db_positions...][freq_positions...][major_indices...]
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
    // Convert UV to pixel coordinates
    let pixel_coord = input.uv * uniforms.resolution;

    // Calculate spectrum area (matching grid_overlay.rs and spectrum_display.rs)
    let spectrum_width = uniforms.resolution.x - uniforms.spectrum_margin_right;
    let spectrum_height = uniforms.resolution.y - uniforms.spectrum_margin_bottom;

    // Only draw within spectrum area
    if pixel_coord.x >= spectrum_width || pixel_coord.y >= spectrum_height {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    let line_width_pixels = uniforms.line_width; // Should be 0.5 like original

    // Check for dB horizontal lines using storage buffer
    // Storage buffers allow dynamic indexing, unlike local arrays
    var db_line_found = false;

    for (var i = 0u; i < grid_data.db_line_count; i++) {
        // Access normalized Y position from storage buffer
        let normalized = line_positions[i];
        let line_y = spectrum_height * (1.0 - normalized);

        if abs(pixel_coord.y - line_y) <= line_width_pixels {
            db_line_found = true;
            break;
        }
    }

    // Check for frequency vertical lines using storage buffer
    var freq_line_found = false;
    var is_major_line = false;

    let freq_start = grid_data.db_line_count;
    let major_start = freq_start + grid_data.freq_line_count;

    for (var i = 0u; i < grid_data.freq_line_count; i++) {
        // Access normalized X position from storage buffer
        let log_pos = line_positions[freq_start + i];
        let line_x = log_pos * spectrum_width;

        if abs(pixel_coord.x - line_x) <= line_width_pixels {
            freq_line_found = true;

            // Check if this is a major frequency line
            for (var j = 0u; j < grid_data.major_freq_count; j++) {
                let major_idx = u32(line_positions[major_start + j]);
                if i == major_idx {
                    is_major_line = true;
                    break;
                }
            }
            break;
        }
    }

    // Return appropriate color based on line type (matching UITheme colors)
    if freq_line_found && is_major_line {
        // Major frequency lines: GRID_LINE = rgba(0.3, 0.3, 0.4, 0.3)
        return vec4<f32>(0.3, 0.3, 0.4, 0.3);
    } else if freq_line_found || db_line_found {
        // Minor frequency lines and dB lines: GRID_LINE_LIGHT = rgba(0.25, 0.25, 0.3, 0.15)
        return vec4<f32>(0.25, 0.25, 0.3, 0.15);
    }

    // No grid line at this pixel
    return vec4<f32>(0.0, 0.0, 0.0, 0.0);
}