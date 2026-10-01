/* overlay.wgsl
 * Contains the functions to render the overlay to the viewport, to
 * draw things like lights, zone paths, and shapes.
 *******************************************************************/

/***********************************/
/*         LIGHTS OVERLAY          */
/***********************************/

// Constants for the light overlay
const LIGHT_BUTTON_RADIUS = 10.0;            // Radius of the button where the actual light source is
const LIGHT_BUTTON_OUTLINE_HALF_WIDTH = 0.0; // Width of the light button's black outline
const LIGHT_HALO_HALF_WIDTH = 0.5;           // Width of the halo for the max range of the light source

fn overlay_light(
    color: vec3<f32>,   // Base color below this part of the overlay
    pos: vec2<f32>,     // World-space position of this pixel
) -> vec3<f32> {        // Returns new color after adding overlay
    var col = color;

    // Get the distance to the light source
    let light_distance_2d = length(params.light_pos.xy - pos.xy);
    
    // If flags 02 (LIGHT BUTTON), draw a circle at the light
    if (flag(2)) {
        col = draw_circle(col, pos, vec4<f32>(1, 1, 1, 0.8), params.light_pos.xy, LIGHT_BUTTON_RADIUS - 1, 0.0, true);
        col = draw_circle(col, pos, vec4<f32>(0, 0, 0, 1), params.light_pos.xy, LIGHT_BUTTON_RADIUS, LIGHT_BUTTON_OUTLINE_HALF_WIDTH, false);
    }

    // If flags 03 (LIGHT HALO), draw a halo around the light
    if (flag(3)) {
        col = draw_circle(col, pos, vec4<f32>(1, 1, 1, 0.8), params.light_pos.xy, params.light_pos.z / params.inv_scale, LIGHT_HALO_HALF_WIDTH, false);
    }

    return col;
}

/***********************************/
/*     CONTROL_POINTS_OVERLAY      */
/***********************************/

// Constants for the light overlay
const PATH_HALF_WIDTH = 1.0;                 // Width of the line drawn for control point paths
const CONTROL_POINT_HALF_WIDTH = 3.0;        // Width of the square drawn for control points

fn overlay_zone_paths(
    color: vec3<f32>,   // Base color below this part of the overlay
    pos: vec2<f32>,     // World-space position of this pixel
) -> vec3<f32> {        // Returns new color after adding overlay
    var col = color;
    
    // Loop through all zones
    for (var i = 0u; i < params.zone_count; i += 1u) {
        let zone = unpack_zone(zones[i]);
        var last_point = unpack_point(points[zone.points_start + zone.point_count - 1]);
        let path_color = vec4<f32>(1, 0.5, 0, 1);

        // Draw an orange box for each control point
        for (var j = 0u; j < zone.point_count; j += 1u) {
            let point = unpack_point(points[zone.points_start + j]);
            col = draw_bezier(
                col,
                pos,
                path_color,
                last_point.position,
                last_point.position + last_point.right_handle,
                point.position + point.left_handle,
                point.position,
                PATH_HALF_WIDTH
            );
            col = draw_box(col, pos, path_color, point.position, CONTROL_POINT_HALF_WIDTH);
            last_point = point;
        }
    }

    return col;
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
        color = overlay_zone_paths(color, pos);
    }

    // Lights
    color = overlay_light(color, pos);

    return color;
}