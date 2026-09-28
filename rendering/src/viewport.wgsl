// The output image as a storage texture which allows pixel-level write access.
@group(0) @binding(0) var output: texture_storage_2d<rgba8unorm, write>;

// The input textures for the sprite and normal map
@group(0) @binding(1) var sprite: texture_2d<f32>;
@group(0) @binding(2) var normal: texture_2d<f32>;

// Uniform struct
struct Params {
    // Gray color value of the background
    background: u32,
}
@group(0) @binding(3) var<uniform> params: Params;

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

    // Base colors and normals for pixels outside of the image
    var color = vec3<f32>(0.2, 0.2, 0.2);
    var norm = vec3<f32>(0, 0, 1);

    // Get the background color from uniforms
    let bg_gray = f32(params.background) / 255.0;
    let background = vec3<f32>(bg_gray, bg_gray, bg_gray); // shown behind pixels in the image

    // If this is within the spritesheet, get sprite color
    // TODO: sRGB correction if necessary
    let size_spritesheet = textureDimensions(sprite);
    if (id.x < size_spritesheet.x && id.y < size_spritesheet.y) {
        let sprite_color = textureLoad(sprite, id.xy, 0);
        color = mix(background, sprite_color.rgb, sprite_color.a);

        // Also set the normal map if it's within that
        let size_normal = textureDimensions(normal);
        if (id.x < size_normal.x && id.y < size_normal.y) {
            let norm_color = textureLoad(normal, id.xy, 0); // vec4<f32>
            norm = (norm_color * 2).xyz - vec3<f32>(1.0, 1.0, 1.0);
        }
    }
    
    // TODO: Render the light with the normal

    // Output to final viewport texture
    let rgba = vec4<f32>(color, 1.0);
    textureStore(output, id.xy, rgba);
}