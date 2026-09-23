// The output image. Storage textures are like normal textures (whatever they are)
// except that these can be written to one pixel at a time.
//
// - `texture_storage_2d`: a 2D image we can read/write by integer pixel coordinate
// - `rgba8unorm`: 4 channels, 8 bits each. Shader values are floats in 0.0..1.0,
//   which get converted to 0..255 on write. Should be the format egui expects.
// - `write`: access modifier. write permissions, no read
//
// group/binding: used to match bindings here with bindings when we create the texture
// Must match the format of the texture created in program, which needs STORAGE_BINDING usage.
// One u32 per pixel, holding RGBA packed as 4 bytes.
@group(0) @binding(0) var<storage, read_write> output: array<u32>;

// TODO: Pass in as uniform buffers
const WIDTH: u32 = 240u;
const HEIGHT: u32 = 240u;
const BWIDTH: u32 = 256u; // this is probably available as a builtin

// Marks this function as a compute entry point.
@compute
// Each workgroup is a 16x16 block of threads (256 in total).
// Dispatch ceil(width / 16) x ceil(height / 16) workgroups.
@workgroup_size(16, 16, 1)
fn main(
    // Position in the full grid of threads across all workgroups, from top left.
    @builtin(global_invocation_id) id: vec3<u32>
) {
    // Get the size of the output image in pixels, as a vec2<u32>.
    // TODO: Get form uniform buffers
    let size = vec2<u32>(WIDTH, HEIGHT);

    // Ensure this pixel is bounded in the image
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }

    // Convert the integer pixel position into a 0.0..1.0 fraction of the image.
    // Dividing by (size - 1) rather than size makes the last pixel reach exactly 1.0.
    // (so don't try to create a size 1 image)
    let uv = vec2<f32>(id.xy) / vec2<f32>(size - vec2<u32>(1u, 1u));

    // Fade red along X and green along Y
    let index = id.y * BWIDTH + id.x;
    output[index] = pack4x8unorm(vec4<f32>(uv.x, uv.y, 0.0, 1.0));
}