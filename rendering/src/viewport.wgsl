// The output image as a storage texture which allows pixel-level write access.
@group(0) @binding(0) var output: texture_storage_2d<rgba8unorm, write>;

// Uniform struct
struct Params {
    blue: u32, // we just need something to pass to the buffer
}
@group(0) @binding(1) var<uniform> params: Params;

// Marks this function as a compute entry point.
// Each workgroup is a 16x16 block of threads (256 in total).
// Dispatch ceil(width / 16) x ceil(height / 16) workgroups.
@compute @workgroup_size(16, 16, 1)
fn main(
    // Position in the full grid of threads across all workgroups, from top left.
    @builtin(global_invocation_id) id: vec3<u32>
) {
    let size = textureDimensions(output);

    // Ensure this pixel is bounded in the image
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }

    // Convert the integer pixel position into a 0.0..1.0 fraction of the image.
    // Dividing by (size - 1) rather than size makes the last pixel reach exactly 1.0.
    // (so don't try to create a size 1 image)
    let uv = vec2<f32>(id.xy) / vec2<f32>(size - vec2<u32>(1u, 1u));
    let blue = f32(params.blue) / 255.0;

    // Fade red along X and green along Y
    let rgba = vec4<f32>(uv.x, uv.y, blue, 1.0);

    textureStore(output, id.xy, rgba);
}