// Particle Position Compute Shader
// OPTIMIZATION: Calculate particle positions once per frame on GPU (not per-pixel!)
// Industry standard approach - eliminates millions of noise calculations per frame

// Uniforms needed for particle animation
struct Uniforms {
    spectrum_resolution: vec2<f32>,
    spectrum_margin_right: f32,
    spectrum_margin_bottom: f32,
    min_db: f32,
    max_db: f32,
    time: f32,
    spectrum_energy: f32,
    animation_speed: f32,
    _padding: f32,
}

// Metadata about the spectrum data
struct SpectrumMetadata {
    bin_count: u32,
    _padding0: u32,
    _padding1: u32,
    _padding2: u32,
}

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// Output: Particle positions (vec2 for each particle)
@group(0) @binding(1)
var<storage, read_write> particle_positions: array<vec2<f32>>;

// Input: Spectrum metadata (bin count)
@group(0) @binding(2)
var<storage, read> metadata: SpectrumMetadata;

// Input: Spectrum bin values (dB magnitudes)
@group(0) @binding(3)
var<storage, read> spectrum_bins: array<f32>;

// === NOISE FUNCTIONS ===
// Optimized hash-based functions (no expensive sin() calls)

// 2D Random - Hash-based
fn random(st: vec2<f32>) -> f32 {
    var p3 = fract(vec3<f32>(st.x, st.y, st.x) * 0.1031);
    p3 += dot(p3, vec3<f32>(p3.y, p3.z, p3.x) + 33.33);
    return fract((p3.x + p3.y) * p3.z);
}

// 2D Noise
fn noise(st: vec2<f32>) -> f32 {
    let i = floor(st);
    let f = fract(st);

    // Four corners in 2D of a tile
    let a = random(i);
    let b = random(i + vec2<f32>(1.0, 0.0));
    let c = random(i + vec2<f32>(0.0, 1.0));
    let d = random(i + vec2<f32>(1.0, 1.0));

    // Smooth interpolation - Cubic Hermite Curve
    let u = f * f * (3.0 - 2.0 * f);

    // Mix 4 corners
    return mix(a, b, u.x) +
           (c - a) * u.y * (1.0 - u.x) +
           (d - b) * u.x * u.y;
}

// === SPECTRUM BOUNDARY HELPERS ===

// Convert dB value to normalized Y position (0 to 1)
// 0.0 = min_db (bottom), 1.0 = max_db (top)
fn db_to_normalized(db: f32) -> f32 {
    let db_range = uniforms.max_db - uniforms.min_db;
    let normalized = (db - uniforms.min_db) / db_range;
    return clamp(normalized, 0.0, 1.0);
}

// Get spectrum Y position at a given X coordinate (in UV space 0-1)
// This constrains particles to stay below the spectrum line
// Returns Y in UV space where 0.0 is top, 1.0 is bottom
fn get_spectrum_y_at_x_uv(uv_x: f32) -> f32 {
    // Handle edge cases
    if metadata.bin_count == 0u {
        return 1.0; // Bottom of screen when no data
    }

    // Clamp X to valid range
    let norm_x = clamp(uv_x, 0.0, 1.0);

    // Find which bin(s) this X position corresponds to
    let bin_position = norm_x * f32(metadata.bin_count - 1u);
    let bin_index = u32(floor(bin_position));
    let bin_fraction = fract(bin_position);

    // Calculate spectrum height in pixels (accounting for margin)
    let spectrum_height = uniforms.spectrum_resolution.y - uniforms.spectrum_margin_bottom;

    // Handle last bin edge case
    if bin_index >= metadata.bin_count - 1u {
        let db_value = spectrum_bins[metadata.bin_count - 1u];
        let norm_y = db_to_normalized(db_value);
        // Convert to pixel space, add 1px offset (same as spectrum line), convert back to UV
        let pixel_y = spectrum_height * (1.0 - norm_y) + 1.0;
        return pixel_y / uniforms.spectrum_resolution.y;
    }

    // Interpolate between two adjacent bins
    let db1 = spectrum_bins[bin_index];
    let db2 = spectrum_bins[bin_index + 1u];

    let norm_y1 = db_to_normalized(db1);
    let norm_y2 = db_to_normalized(db2);
    let norm_y = mix(norm_y1, norm_y2, bin_fraction);

    // Convert to pixel space, add 1px offset (same as spectrum line), convert back to UV
    let pixel_y = spectrum_height * (1.0 - norm_y) + 1.0;
    return pixel_y / uniforms.spectrum_resolution.y;
}

// Compute shader entry point
// Each invocation calculates one particle's position
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) global_id: vec3<u32>) {
    let particle_id = global_id.x;

    // Early exit if this thread doesn't correspond to a particle
    // (workgroup size 64 might be larger than particle count)
    if (particle_id >= arrayLength(&particle_positions)) {
        return;
    }

    // Apply animation speed to time
    let speed = 0.2 * uniforms.animation_speed;
    let animated_time = uniforms.time * speed;

    // Calculate noise-based particle position
    // Same logic as original fragment shader, but calculated once per particle
    var pos = vec2<f32>(
        noise(vec2<f32>(f32(particle_id), 1.0) + vec2<f32>(animated_time, animated_time)),
        noise(vec2<f32>(f32(particle_id), 0.0) + vec2<f32>(animated_time, animated_time))
    );

    // EDGE-AWARE CONSTRAINT: Keep particles below the spectrum line
    // This makes the fluid appear to be "pushed" by the spectrum boundary
    let spectrum_y = get_spectrum_y_at_x_uv(pos.x);

    // If particle is above (y < spectrum_y in UV space), push it below
    // Add small offset (0.02) to prevent particles from sitting exactly on the line
    if pos.y < spectrum_y {
        pos.y = spectrum_y + 0.02;
    }

    // Clamp particles to canvas bounds (0-1 UV space)
    // This prevents particles from escaping the visible area
    pos = clamp(pos, vec2<f32>(0.0), vec2<f32>(1.0));

    // Write to storage buffer - fragment shader will read this
    particle_positions[particle_id] = pos;
}
