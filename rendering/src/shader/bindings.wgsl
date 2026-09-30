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
    // Inverse scale of pixels in the camera. If one spritesheet pixel is 2 pixels onscreen, this is 0.5
    inv_scale: f32,

    // Extra flags for which parts of the overlay are shown.
    // In order of increasing magnitude:
    // 00: Normal Map on (default false, replaces spritesheet)
    // 01: Lighting on (default true, does nothing if flag 00 is set)
    // 02: Light Button On (default true)
    // 03: Light Button Halo On (default false, true when dragging light)
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
    overlay_flags: u32,

    // Position of the point light, assuming each pixel is one unit.
    // Technically passed in as a Vec2 and another f32 but this is fine. Keeping it as a vec3 is useful for shading.
    light_pos: vec3<f32>,

    // Light color
    light_color: u32,
}
@group(0) @binding(3) var<uniform> params: Params;