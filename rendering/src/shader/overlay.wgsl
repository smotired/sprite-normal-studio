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
const CONTROL_POINT_HANDLE_HALF_WIDTH = 1.0; // Width of the line drawn for handles of control points
const CONTROL_POINT_HANDLE_RADIUS = 2.0;     // Radius of the circle drawn for handles of control points

fn overlay_zone_paths(
    color: vec3<f32>,   // Base color below this part of the overlay
    pos: vec2<f32>,     // World-space position of this pixel
) -> vec3<f32> {        // Returns new color after adding overlay
    var col = color;
    
    // Loop through all zones
    for (var i = 0u; i < params.zone_count; i += 1u) {
        let zone = unpack_zone(zones[i]);
        var last_point = unpack_point(points[zone.points_start + zone.point_count - 1]);

        // If not selected, just draw the path
        let selected = (i == params.selected_zone);
        let base_color = vec4<f32>(1, 1, 1, 0.5);
        let selected_color = vec4<f32>(1, 0.5, 0, 1);

        // If selected check completion flag
        var start = 0u;
        if (selected && flag(5)) {
            start = 1u;
            last_point = unpack_point(points[zone.points_start]);
        }

        // Draw the path between control points
        for (var j = start; j < zone.point_count; j += 1u) {
            let point = unpack_point(points[zone.points_start + j]);

            var path_color = base_color;
            if (selected) { path_color = selected_color; }

            col = draw_bezier(
                col,
                pos,
                path_color,
                last_point.position,
                right_handle(last_point),
                left_handle(point),
                point.position,
                PATH_HALF_WIDTH
            );

            last_point = point;
        }

        // If we stopped drawing the path early, complete the path at the cursor instead
        if (selected && flag(5)) {
            col = draw_bezier(
                col,
                pos,
                base_color,
                last_point.position,
                right_handle(last_point),
                params.cursor_pos,
                params.cursor_pos,
                PATH_HALF_WIDTH * 0.5
            );
        }

        // If the zone is selected, also draw the control points on top of the path
        if (selected) {
            for (var j = 0u; j < zone.point_count; j += 1u) {
                let point = unpack_point(points[zone.points_start + j]);

                // If the point is not selected just draw a box
                if (j + zone.points_start != params.selected_point) {
                    col = draw_box(col, pos, selected_color, point.position, CONTROL_POINT_HALF_WIDTH);
                }

                // Otherwise, draw the handles and then the box
                else {
                    if (!point.no_handles) {
                        let left_handle = left_handle(point);
                        let right_handle = right_handle(point);

                        let left_handle_color = vec4<f32>(1, 0, 0, 1);
                        let right_handle_color = vec4<f32>(1, 1, 0, 1);

                        col = draw_line(col, pos, left_handle_color, point.position, left_handle, CONTROL_POINT_HANDLE_HALF_WIDTH);
                        col = draw_line(col, pos, right_handle_color, point.position, right_handle, CONTROL_POINT_HANDLE_HALF_WIDTH);

                        col = draw_circle(col, pos, left_handle_color, left_handle, CONTROL_POINT_HANDLE_RADIUS, 0.0, true);
                        col = draw_circle(col, pos, right_handle_color, right_handle, CONTROL_POINT_HANDLE_RADIUS, 0.0, true);
                    }

                    col = draw_box(col, pos, vec4<f32>(1, 1, 1, 1), point.position, CONTROL_POINT_HALF_WIDTH);
                }
            }
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