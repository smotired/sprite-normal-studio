/***********************************/
/*             BINDINGS            */
/***********************************/

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

/***********************************/
/*      SPRITESHEET LIGHTING       */
/***********************************/

// Constants for rendering the spritesheet
const GRAY_OOB = 0.2;
const GRAY_BACKGROUND = 0.25;
const AMBIENT_LIGHT = 0.2;

// Gets the base color of the spritesheet at this pixel
fn spritesheet_color(
    pos: vec2<f32>,            // World-space position of this pixel
) -> vec3<f32> { // Returns the base pixel color from lighting the spritesheet
    // Get coordinates of the actual image pixel target
    let pxl = vec2<i32>(floor(pos)); // floor puts -0.3 at -1, avoiding duplicate 0-row and 0-column

    // Base color for pixels outside of the image
    var color = vec3<f32>(1, 1, 1) * GRAY_OOB;

    // If this is within the spritesheet, get sprite color
    let size_spritesheet = vec2<i32>(textureDimensions(sprite));
    if (pxl.x >= 0 && pxl.y >= 0 && pxl.x < size_spritesheet.x && pxl.y < size_spritesheet.y) {
        // Get the color from the spritesheet
        let sprite_color = textureLoad(sprite, pxl.xy, 0);
        let base_color = sprite_color.rgb;

        // Get the background and light colors from constants or uniforms
        let background: vec3<f32>  = vec3<f32>(1, 1, 1) * GRAY_BACKGROUND; // shown behind pixels in the image
        let light_color: vec3<f32> = unpack4x8unorm(params.light_color).rgb;

        // Get the normal map if bounded. Normal map should have the same size, but bound just in case.
        var norm = vec3<f32>(0, 0, 1);
        let size_normal = vec2<i32>(textureDimensions(normal));
        if (pxl.x < size_normal.x && pxl.y < size_normal.y) {
            let norm_color = textureLoad(normal, pxl.xy, 0); // vec4<f32>
            norm = normalize(norm_color.rgb * 2.0 - 1.0);
        }

        // Get direction and distance to the light source
        let to_light = params.light_pos - vec3<f32>(pos, 0); // light_pos should be correct despite scale
        let light_dir = normalize(to_light);
        let light_distance = length(to_light);

        // Calculate diffuse color from cosine to light direction
        // Assuming +Z is our out direction, i.e. camera Z is at +infinity
        let diffuse = max(dot(norm, light_dir), 0.0); // clamp to 0 so backwards normals (which shouldn't exist) don't mess stuff up

        // Add falloff from light height
        let attenuation = (params.light_pos.z * params.light_pos.z) / max(light_distance * light_distance, 1.0);

        // Get actual reflected color
        let reflected = base_color * light_color * diffuse * attenuation;

        // Add ambient light
        let ambient = base_color * AMBIENT_LIGHT;

        // Update color and fade alpha
        color = mix(background, reflected + ambient, sprite_color.a);
    }

    return color;
}

/***********************************/
/*         OVERLAY: LIGHT          */
/***********************************/

// Constants for the light overlay
const LIGHT_BUTTON_RADIUS = 10.0;
const LIGHT_HALO_WIDTH = 2.0;

fn overlay_light(
    pos: vec2<f32>,     // World-space position of this pixel
) -> vec4<f32> {        // Returns pixel color from overlay
    var color = vec4<f32>(0, 0, 0, 0);

    // Get the distance to the light source
    let light_distance_2d = length(params.light_pos.xy - pos.xy);

    // Draw a circle at the light
    if (light_distance_2d <= LIGHT_BUTTON_RADIUS / f32(params.camera_scale)) {
        color = vec4<f32>(1, 1, 1, 1);
    }

    // Draw a halo around the light
    else if (abs(light_distance_2d - params.light_pos.z) <= LIGHT_HALO_WIDTH * 0.5 / f32(params.camera_scale)) {
        color = vec4<f32>(1, 1, 1, 1);
    }

    return color;
}

/***********************************/
/*         OVERLAY: FULL           */
/***********************************/

fn overlay_color(
    pos: vec2<f32>,     // World-space position of this pixel
) -> vec4<f32> {        // Returns pixel color from overlay
    // Render the overlay one by one.
    var color = vec4<f32>(0, 0, 0, 0);

    // Shapes

    // Zone Interiors

    // Zones

    // Lights
    let light = overlay_light(pos);
    color = mix(color, vec4<f32>(light.rgb, 1.0), light.a);

    return color;
}

/***********************************/
/*          SHADER FUNCTION        */
/***********************************/

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

    // Get the world-space pixel coordinates from camera info
    let scale_f = f32(params.camera_scale);
    let screen_offset = vec2<f32>(id.xy) - vec2<f32>(half);
    let pos = screen_offset / scale_f + params.camera_pos;

    // Get information about the light
    let light_distance_2d = length(params.light_pos.xy - pos.xy);

    // Get the base color of this pixel from spritesheet.
    var color = spritesheet_color(pos);

    // Add in the overlay
    let overlay = overlay_color(pos);
    color = mix(color, overlay.rgb, overlay.a);

    // Output final color to viewport texture
    textureStore(output, id.xy, vec4<f32>(color, 1.0));
}