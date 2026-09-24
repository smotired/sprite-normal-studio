// The output image as a vector of packed 32-bit RGBA ints.
//
// group/binding: used to match bindings here with bindings when we create the texture
// Must match the format of the texture created in program, which needs STORAGE_BINDING usage.
// One u32 per pixel, holding RGBA packed as 4 bytes.
@group(0) @binding(0) var<storage, read_write> output: array<u32>;

// Uniform struct
struct Params {
    screen: vec2<u32>,
    buffer_width: u32,
    _pad: u32,
}
@group(0) @binding(1) var<uniform> params: Params;

// Marks this function as a compute entry point.
@compute
// Each workgroup is a 16x16 block of threads (256 in total).
// Dispatch ceil(width / 16) x ceil(height / 16) workgroups.
@workgroup_size(16, 16, 1)
fn main(
    // Position in the full grid of threads across all workgroups, from top left.
    @builtin(global_invocation_id) id: vec3<u32>
) {
    // Ensure this pixel is bounded in the image
    if (id.x >= params.screen.x || id.y >= params.screen.y) {
        return;
    }

    // Convert the integer pixel position into a 0.0..1.0 fraction of the image.
    // Dividing by (size - 1) rather than size makes the last pixel reach exactly 1.0.
    // (so don't try to create a size 1 image)
    let uv = vec2<f32>(id.xy) / vec2<f32>(params.screen - vec2<u32>(1u, 1u));

    // Fade red along X and green along Y
    let index = id.y * params.buffer_width + id.x;
    output[index] = pack4x8unorm(vec4<f32>(uv.x, uv.y, 0.0, 1.0));
}