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
    
    // If flags 02 (LIGHT BUTTON), draw a circle at the light
    if (flag(2)) {
        if (light_distance_2d <= LIGHT_BUTTON_RADIUS * params.inv_scale) {
            color = vec4<f32>(1, 1, 1, 1);
        }
        let light_outline_distance = light_distance_2d - LIGHT_BUTTON_RADIUS * params.inv_scale;
        let button_color = feather_color(vec4<f32>(0, 0, 0, 1), light_outline_distance, LIGHT_BUTTON_OUTLINE_WIDTH * 0.5);
        color = mix(color, vec4<f32>(button_color.rgb, 1.0), button_color.a);
    }

    // If flags 03 (LIGHT HALO), draw a halo around the light
    if (flag(3)) {
        let halo_distance = light_distance_2d - params.light_pos.z;
        let halo_color = feather_color(vec4<f32>(1, 1, 1, 1), halo_distance, LIGHT_HALO_WIDTH * 0.5);
        color = mix(color, vec4<f32>(halo_color.rgb, 1.0), halo_color.a);
    }

    return color;
}

/***********************************/
/*     CONTROL_POINTS_OVERLAY      */
/***********************************/

// Constants for the light overlay
const CONTROL_POINT_WIDTH = 4.0;        // Width of the square drawn for control points

fn overlay_zone_paths(
    pos: vec2<f32>,     // World-space position of this pixel
) -> vec4<f32> {        // Returns pixel color from overlay
    let h = CONTROL_POINT_WIDTH * 0.5;
    
    // Loop through all zones
    for (var i = 0u; i < params.zone_count; i += 1u) {
        let zone = unpack_zone(zones[i]);

        for (var j = 0u; j < zone.point_count; j += 1u) {
            let to_point = unpack_point(points[zone.points_start + j]).position - pos;

            // If within the square return orange
            if (abs(to_point.x) <= h && abs(to_point.y) <= h) {
                return vec4<f32>(1, 0.5, 0, 1);
            }
        }
    }

    return vec4<f32>(0, 0, 0, 0);
}

/***********************************/
/*      MAIN OVERLAY METHOD        */
/***********************************/

fn overlay_color(
    start_color: vec3<f32>,   // Base color before overlay
    pos: vec2<f32>,           // World-space position of this pixel
) -> vec3<f32> {              // Returns pixel color from overlay
    // Render the overlay layer by layer.
    var color = start_color;

    // Shapes

    // Zone Interiors

    // Zone paths/points if paths are enabled
    if (flag(4)) {
        let paths = overlay_zone_paths(pos);
        color = mix(color, paths.rgb, paths.a);
    }

    // Lights
    let light = overlay_light(pos);
    color = mix(color, light.rgb, light.a);

    return color;
}