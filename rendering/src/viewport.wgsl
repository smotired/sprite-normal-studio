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

    // The ambient light color (nothing should be in total darkness)
    ambient_light: u32,

    // Position of the point light, assuming each pixel is one unit
    light_pos: vec3<f32>,
    // Light color
    light_color: u32,
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
    let half = size / 2;

    // Ensure this pixel is bounded to the viewport image
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }

    // Get the pixel coordinates in the reference image from camera info
    let scale_f = f32(params.camera_scale);
    let screen_offset = vec2<f32>(id.xy) - vec2<f32>(half);
    let pos_f = screen_offset / scale_f + params.camera_pos;
    let pxl = vec2<i32>(floor(pos_f)); // floor puts -0.3 at -1, avoiding duplicate 0-row and 0-column

    // Base colors and normals for pixels outside of the image
    var color = vec3<f32>(0.2, 0.2, 0.2);

    // If this is within the spritesheet, get sprite color
    let size_spritesheet = vec2<i32>(textureDimensions(sprite));
    if (pxl.x >= 0 && pxl.y >= 0 && pxl.x < size_spritesheet.x && pxl.y < size_spritesheet.y) {
        // Get the color from the spritesheet
        let sprite_color = textureLoad(sprite, pxl.xy, 0);
        let base_color = sprite_color.rgb;

        // Get the background, ambient, and light colors from uniforms
        let background: vec3<f32>    = vec3<f32>(0.25, 0.25, 0.25); // shown behind pixels in the image
        let light_color: vec3<f32>   = unpack4x8unorm(params.light_color).rgb;
        let ambient_color: vec3<f32> = unpack4x8unorm(params.ambient_light).rgb;

        // Get the normal map if bounded. Normal map should have the same size, but bound just in case.
        var norm = vec3<f32>(0, 0, 1);
        let size_normal = vec2<i32>(textureDimensions(normal));
        if (pxl.x < size_normal.x && pxl.y < size_normal.y) {
            let norm_color = textureLoad(normal, pxl.xy, 0); // vec4<f32>
            norm = normalize(norm_color.rgb * 2.0 - 1.0);
        }
    
        // Render the light with the normal
        let pos = vec3<f32>(vec2<f32>(pxl.xy), 0);

        // Get direction and distance to the light source
        let to_light = params.light_pos - pos; // light_pos should be correct despite scale
        let distance = length(to_light);
        let light_dir = normalize(to_light);

        // Calculate diffuse color from cosine to light direction
        // Assuming +Z is our out direction, i.e. camera Z is at +infinity
        let diffuse = max(dot(norm, light_dir), 0.0); // clamp to 0 so backwards normals (which shouldn't exist) don't mess stuff up

        // Add falloff from light height
        let attenuation = (params.light_pos.z * params.light_pos.z) / max(distance * distance, 1.0);

        // Get actual reflected color
        let reflected = base_color * light_color * diffuse * attenuation;

        // Add ambient light
        let ambient = base_color * ambient_color;

        // Update color and fade alpha
        color = mix(background, reflected + ambient, sprite_color.a);
    }

    // Output to final viewport texture
    let rgba = vec4<f32>(color, 1.0);
    textureStore(output, id.xy, rgba);
}