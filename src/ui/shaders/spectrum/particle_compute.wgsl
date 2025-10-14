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

@group(0) @binding(0)
var<uniform> uniforms: Uniforms;

// Output: Particle positions (vec2 for each particle)
@group(0) @binding(1)
var<storage, read_write> particle_positions: array<vec2<f32>>;

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
    let pos = vec2<f32>(
        noise(vec2<f32>(f32(particle_id), 1.0) + vec2<f32>(animated_time, animated_time)),
        noise(vec2<f32>(f32(particle_id), 0.0) + vec2<f32>(animated_time, animated_time))
    );

    // Write to storage buffer - fragment shader will read this
    particle_positions[particle_id] = pos;
}
