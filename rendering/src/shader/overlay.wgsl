/* overlay.wgsl
 * Contains the functions to render the overlay to the viewport, to
 * draw things like lights, zone paths, and shapes.
 *******************************************************************/

/***********************************/
/*         LIGHTS OVERLAY          */
/***********************************/

// Constants for the light overlay
const LIGHT_BUTTON_RADIUS = 10.0;       // Radius of the button where the actual light source is
const LIGHT_BUTTON_OUTLINE_WIDTH = 0.0; // Width of the light button's black outline
const LIGHT_HALO_WIDTH = 1.0;           // Width of the halo for the max range of the light source

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
    let light_outline_distance = light_distance_2d - LIGHT_BUTTON_RADIUS;
    let button_color = feather_color(vec4<f32>(0, 0, 0, 1), light_outline_distance, LIGHT_BUTTON_OUTLINE_WIDTH * 0.5 / f32(params.camera_scale));
    color = mix(color, vec4<f32>(button_color.rgb, 1.0), button_color.a);

    // Draw a circle at the light

    // Draw a halo around the light
    let halo_distance = light_distance_2d - params.light_pos.z;
    let halo_color = feather_color(vec4<f32>(1, 1, 1, 1), halo_distance, LIGHT_HALO_WIDTH * 0.5 / f32(params.camera_scale));
    color = mix(color, vec4<f32>(halo_color.rgb, 1.0), halo_color.a);

    return color;
}

/***********************************/
/*      MAIN OVERLAY METHOD        */
/***********************************/

fn overlay_color(
    pos: vec2<f32>,     // World-space position of this pixel
) -> vec4<f32> {        // Returns pixel color from overlay
    // Render the overlay layer by layer.
    var color = vec4<f32>(0, 0, 0, 0);

    // Shapes

    // Zone Interiors

    // Zones

    // Lights
    let light = overlay_light(pos);
    color = mix(color, vec4<f32>(light.rgb, 1.0), light.a);

    return color;
}