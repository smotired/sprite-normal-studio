/* spritesheet.wgsl
 * Contains the function to render the actual lit spritesheet from
 * the input texture, normal map, and lighting information.
 *******************************************************************/

// Constants for rendering the spritesheet
const GRAY_OOB = 0.2;           // Gray color for pixels outside of the spritesheet
const GRAY_BACKGROUND = 0.25;   // Gray color for the transparent areas of the spritesheet
const AMBIENT_LIGHT = 0.2;      // How much of the light is replaced with ambient white light

// Gets the base color of the spritesheet at this pixel
fn spritesheet_color(
    pos: vec2<f32>,            // World-space position of this pixel
) -> vec3<f32> { // Returns the base pixel color from lighting the spritesheet
    // Get coordinates of the actual image pixel target
    let pxl = vec2<i32>(floor(pos)); // floor puts -0.3 at -1, avoiding duplicate 0-row and 0-column

    // Base color for pixels outside of the image
    var color = vec3<f32>(1, 1, 1) * GRAY_OOB;

    // If flag 00 (NORMAL MAP) is set, draw the normal map instead
    if (flag(0)) {
        let size_normal = vec2<i32>(textureDimensions(normal));
        if (pxl.x < size_normal.x && pxl.y < size_normal.y) {
            let background: vec3<f32>  = vec3<f32>(1, 1, 1) * GRAY_BACKGROUND;
            let norm_color = textureLoad(normal, pxl.xy, 0); // vec4<f32>
            color = mix(background, norm_color.rgb, norm_color.a); // a should always be 1 but maybe not
        }
        return color;
    }

    // If this is within the spritesheet, get sprite color
    let size_spritesheet = vec2<i32>(textureDimensions(sprite));
    if (pxl.x >= 0 && pxl.y >= 0 && pxl.x < size_spritesheet.x && pxl.y < size_spritesheet.y) {
        // Get the color from the spritesheet
        let sprite_color = textureLoad(sprite, pxl.xy, 0);
        let base_color = sprite_color.rgb;

        // Get the background and light colors from constants or uniforms
        let background: vec3<f32>  = vec3<f32>(1, 1, 1) * GRAY_BACKGROUND; // shown behind pixels in the image
        let light_color: vec3<f32> = unpack4x8unorm(params.light_color).rgb * (1.0 - AMBIENT_LIGHT);

        // If flag 01 (LIGHTING) is enabled, perform lighting
        if (flag(1)) {
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

        // Otherwise just return the sprite color
        else {
            color = mix(background, sprite_color.rgb, sprite_color.a);
        }
    }

    return color;
}