// Combined Spectrum Line + Fluid Fill Shader
// Renders both the spectrum line and animated Voronoi fluid in a single pass

// Uniforms hold data that stays constant during a draw call
// #[repr(C)] in Rust ensures this matches the GPU's memory layout
struct Uniforms {
    spectrum_resolution: vec2<f32>,      // Widget size in pixels (width, height)
    spectrum_margin_right: f32,          // Right margin for frequency labels
    spectrum_margin_bottom: f32,         // Bottom margin for amplitude labels
    min_db: f32,                         // Minimum dB value for amplitude range
    max_db: f32,                         // Maximum dB value for amplitude range
    time: f32,                           // Time in seconds for particle animation
    spectrum_energy: f32,                // Precomputed average spectrum energy (0-1)
    animation_speed: f32,                // User-controlled animation speed multiplier
    _padding: f32,                       // GPU alignment (struct size must be multiple of 16 bytes)
}

// Metadata about the spectrum data
struct SpectrumMetadata {
    bin_count: u32,                  // Number of spectrum bins (data points)
    _padding0: u32,                  // Alignment padding
    _padding1: u32,
    _padding2: u32,
}

// Bind our uniforms to group 0, binding 0
// "uniform" means this data is read-only and the same for all pixels
@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// Bind metadata to group 0, binding 1
// "storage" allows larger, dynamic-sized data
@group(0) @binding(1)
var<storage, read> metadata: SpectrumMetadata;

// Bind spectrum bin values to group 0, binding 2
// This is a dynamic array - size determined at runtime
// Each value is the dB magnitude for that frequency bin
@group(0) @binding(2)
var<storage, read> spectrum_bins: array<f32>;

// Bind precomputed particle positions from compute shader (for fluid animation)
@group(0) @binding(3)
var<storage, read> particle_positions: array<vec2<f32>>;

// Data passed from vertex shader to fragment shader
struct VertexOutput {
    @builtin(position) position: vec4<f32>,  // Clip space position (-1 to 1)
    @location(0) uv: vec2<f32>,              // UV coordinates (0 to 1)
}

// Vertex shader: Generates a fullscreen triangle that covers the entire widget
// This is called 3 times (once per vertex)
@vertex
fn vs_main(@builtin(vertex_index) vertex_index: u32) -> VertexOutput {
    var out: VertexOutput;

    // Clever bit manipulation to generate 3 vertices forming a large triangle
    // vertex 0: (-1, -1) bottom-left
    // vertex 1: (3, -1)  off-screen right
    // vertex 2: (-1, 3)  off-screen top
    // This triangle covers the entire screen with just 3 vertices!
    let x = f32((vertex_index << 1u) & 2u) * 2.0 - 1.0;
    let y = f32(vertex_index & 2u) * 2.0 - 1.0;

    out.position = vec4<f32>(x, y, 0.0, 1.0);

    // Convert clip space (-1 to 1) to UV space (0 to 1)
    // UV (0,0) is top-left, (1,1) is bottom-right
    out.uv = vec2<f32>((x + 1.0) * 0.5, (1.0 - y) * 0.5);

    return out;
}

// Helper: Convert dB value to normalized Y position (0 to 1)
// 0.0 = min_db (bottom), 1.0 = max_db (top)
fn db_to_normalized(db: f32) -> f32 {
    let db_range = uniforms.max_db - uniforms.min_db;
    let normalized = (db - uniforms.min_db) / db_range;
    return clamp(normalized, 0.0, 1.0);  // Clamp ensures value stays in 0-1 range
}

// Helper: Calculate screen position for a display point
// Data is already log-sampled on CPU, so we just map linearly to screen
// This ensures exact match with CPU renderer positioning
fn calculate_spectrum_point_position(point_index: u32) -> vec2<f32> {
    let total_points = metadata.bin_count;

    // Get dB value for this point from storage buffer
    // These are already sampled at logarithmic frequencies by the CPU
    let db_value = spectrum_bins[point_index];

    // Calculate normalized positions
    // X: Linear mapping (data is already log-spaced in frequency)
    let norm_x = f32(point_index) / f32(total_points);

    // Y: dB to normalized (accounting for amplitude range)
    let norm_y = db_to_normalized(db_value);

    // Convert to screen coordinates
    let spectrum_width = uniforms.spectrum_resolution.x - uniforms.spectrum_margin_right;
    let spectrum_height = uniforms.spectrum_resolution.y - uniforms.spectrum_margin_bottom;

    let x = norm_x * spectrum_width;
    let y = spectrum_height * (1.0 - norm_y);  // Flip Y (screen Y increases downward)

    // Shift down by 1 pixel to hide the noise floor line at the bottom
    // When there's no audio, spectrum is at min_db which maps to y=height
    // This pushes it just outside the visible area
    return vec2<f32>(x, y + 1.0);
}

// Helper: Calculate shortest distance from point p to line segment (a, b)
// Uses vector projection to find the closest point on the segment
fn distance_to_line_segment(p: vec2<f32>, a: vec2<f32>, b: vec2<f32>) -> f32 {
    let ab = b - a;  // Direction vector of the line segment
    let ap = p - a;  // Vector from segment start to pixel

    // Project ap onto ab to find closest point parameter
    // t=0.0: closest point is at 'a' (segment start)
    // t=1.0: closest point is at 'b' (segment end)
    // t in (0,1): closest point is between a and b
    let t = clamp(dot(ap, ab) / dot(ab, ab), 0.0, 1.0);

    // Calculate the actual closest point on the segment
    let closest_point = a + t * ab;

    // Return Euclidean distance from pixel to closest point
    return distance(p, closest_point);
}

// Helper: Get spectrum Y position at a given X coordinate (for fluid boundary)
fn get_spectrum_y_at_x(x: f32) -> f32 {
    let spectrum_width = uniforms.spectrum_resolution.x - uniforms.spectrum_margin_right;
    let spectrum_height = uniforms.spectrum_resolution.y - uniforms.spectrum_margin_bottom;
    let norm_x = clamp(x / spectrum_width, 0.0, 1.0);

    if metadata.bin_count == 0u {
        return spectrum_height;
    }

    let bin_position = norm_x * f32(metadata.bin_count - 1u);
    let bin_index = u32(floor(bin_position));
    let bin_fraction = fract(bin_position);

    if bin_index >= metadata.bin_count - 1u {
        let db_value = spectrum_bins[metadata.bin_count - 1u];
        let norm_y = db_to_normalized(db_value);
        return spectrum_height * (1.0 - norm_y) + 1.0;
    }

    let db1 = spectrum_bins[bin_index];
    let db2 = spectrum_bins[bin_index + 1u];

    let norm_y1 = db_to_normalized(db1);
    let norm_y2 = db_to_normalized(db2);
    let norm_y = mix(norm_y1, norm_y2, bin_fraction);

    return spectrum_height * (1.0 - norm_y) + 1.0;
}


// Fragment shader: Runs once for every pixel
// Renders Voronoi fluid constrained below the spectrum boundary
@fragment
fn fs_main(input: VertexOutput) -> @location(0) vec4<f32> {
    // Convert UV (0-1) to widget-local pixel coordinates
    let pixel_coord = input.uv * uniforms.spectrum_resolution;

    // Calculate spectrum drawing area
    let spectrum_width = uniforms.spectrum_resolution.x - uniforms.spectrum_margin_right;
    let spectrum_height = uniforms.spectrum_resolution.y - uniforms.spectrum_margin_bottom;

    // Early exit: Don't draw outside the spectrum area
    if pixel_coord.x >= spectrum_width || pixel_coord.y >= spectrum_height {
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);  // Transparent
    }

    // Check if below spectrum line -> draw fluid fill
    let spectrum_y = get_spectrum_y_at_x(pixel_coord.x);
    if pixel_coord.y < spectrum_y {
        // Above spectrum line - transparent
        return vec4<f32>(0.0, 0.0, 0.0, 0.0);
    }

    // STEP 4: Render Voronoi fluid (we're below the spectrum line)
    let uv = input.uv;
    let npts = arrayLength(&particle_positions);

    var nearest = 1.0;
    var nearest_pt = 0u;
    var nearest_pos = vec2<f32>(0.0, 0.0);

    let speed_centre = 3.0;
    let time = uniforms.time * 0.2 * uniforms.animation_speed;

    // Find nearest particle (positions precomputed by compute shader)
    for (var i = 0u; i < npts; i++) {
        let point = particle_positions[i];
        let d = distance(point, uv);
        if d < nearest {
            nearest = d;
            nearest_pt = i;
            nearest_pos = point;
        }
    }

    // Cell distance coloring
    let cellDist = 1.0 - distance(nearest_pos, uv);

    // Audio-reactive center point
    let spectrum_energy = uniforms.spectrum_energy;
    let centre = vec2<f32>(
        sin(time * speed_centre) * 0.5 + 0.5,
        cos(time * speed_centre) * 0.5 + 0.5
    ) * 0.5 + 0.25;

    let centreDist = 1.0 - distance(nearest_pos, centre);

    // Audio-reactive color modulation
    let energy_boost = spectrum_energy * 0.5 + 0.5;

    // Rainbow color based on position and time
    var col = 0.5 + 0.5 * cos(time + nearest_pos.xyx + vec3<f32>(0.0, 2.0, 4.0));

    // Apply distance-based brightness
    col = col * centreDist * cellDist * energy_boost;

    return vec4<f32>(col, 1.0);
}
