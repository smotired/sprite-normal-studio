// Compute shader to create the normal map from the base, spritesheet, zones, and shapes.

// Input and Output textures
@group(0) @binding(0) var sprite: texture_2d<f32>;
@group(0) @binding(1) var normal: texture_2d<f32>;
@group(0) @binding(2) var output: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(16, 16, 1)
fn main(
    @builtin(global_invocation_id) id: vec3<u32>
) {
    // Output texture should be the size of the spritesheet
    let size = textureDimensions(sprite);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }

    // Get the base normal map color ignoring alpha
    var norm = vec3<f32>(0.5, 0.5, 1.0);
    let in_size = textureDimensions(normal);
    if (id.x < in_size.x && id.y < in_size.y) {
        norm = textureLoad(normal, id.xy, 0).xyz;
    }

    // TODO: Convert norm to an actual unit vector

    // TODO: Get shapes in the pixel's zone(s) and apply

    // TODO: Convert norm back to color
    let norm_out = vec4<f32>(norm, 1.0);

    // Write to the output texture
    textureStore(output, id.xy, norm_out);
}