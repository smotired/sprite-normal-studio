/* bindings.wgsl
 * Contains the bindings of textures, buffers, and uniforms for the
 * viewport shader.
 *******************************************************************/

// The output image as a storage texture which allows pixel-level write access.
@group(0) @binding(0) var output: texture_storage_2d<rgba8unorm, write>;

// The input textures for the sprite and normal map
@group(0) @binding(1) var sprite: texture_2d<f32>;
@group(0) @binding(2) var normal: texture_2d<f32>;

// Uniform struct
struct Params {
    // Position of the camera center ignoring scale
    camera_pos: vec2<f32>,
    // Scale of pixels per camera
    camera_scale: u32,

    // Extra flags for which parts of the overlay are shown.
    // In order of increasing magnitude:
    // 00: 
    // 01: 
    // 02: 
    // 03: 
    // 04: 
    // 05: 
    // 06: 
    // 07: 
    // 08: 
    // 09: 
    // 10: 
    // 11: 
    // 12: 
    // 13: 
    // 14: 
    // 15: 
    // 16: 
    // 17: 
    // 18: 
    // 19: 
    // 20: 
    // 21: 
    // 22: 
    // 23: 
    // 24: 
    // 25: 
    // 26: 
    // 27: 
    // 28: 
    // 29: 
    // 30: 
    // 31: 
    overlay: u32,

    // Position of the point light, assuming each pixel is one unit
    light_pos: vec3<f32>,
    // Light color
    light_color: u32,
}
@group(0) @binding(3) var<uniform> params: Params;