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
const PATH_HALF_WIDTH = 0.0;                 // Width of the line drawn for control point paths
const CONTROL_POINT_HALF_WIDTH = 3.0;        // Width of the square drawn for control points
const CONTROL_POINT_HANDLE_HALF_WIDTH = 1.0; // Width of the line drawn for handles of control points
const CONTROL_POINT_HANDLE_RADIUS = 2.0;     // Radius of the circle drawn for handles of control points

fn overlay_zone_paths(
    color: vec3<f32>,   // Base color below this part of the overlay
    pos: vec2<f32>,     // World-space position of this pixel
) -> vec3<f32> {        // Returns new color after adding overlay
    var col = color;
    var found_selected = false;

    let base_color = vec4<f32>(1, 1, 1, 0.5);
    let left_handle_color = vec4<f32>(1, 0, 0, 1);
    let right_handle_color = vec4<f32>(1, 1, 0, 1);
    let selected_color = vec4<f32>(1, 0.5, 0, 1);
    
    // Loop through all zones
    for (var i = 0u; i < params.zone_count; i += 1u) {
        // Don't draw the selected path
        if (i == params.selected_zone) {
            found_selected = true;
            continue;
        }

        let zone = get_zone(i);
        let first_point = get_point(zone.points_start);
        var last_point = get_point(zone.points_start + zone.point_count - 1);

        // Draw the path between control points
        for (var j = 0u; j < zone.point_count; j += 1u) {
            let point = get_point(zone.points_start + j);

            col = draw_bezier(
                col,
                pos,
                base_color,
                last_point.position,
                right_handle(last_point),
                left_handle(point),
                point.position,
                PATH_HALF_WIDTH
            );

            last_point = point;
        }

        // If flag 5 is set, also draw the control points on top of the path
        if (flag(5)) {
            for (var j = 0u; j < zone.point_count; j += 1u) {
                let point = get_point(zone.points_start + j);

                // Path is not selected, so assume point is not selected and just draw a box.
                col = draw_box(col, pos, base_color, point.position, CONTROL_POINT_HALF_WIDTH);
            }
        }
    }

    // Draw the selected path on top of the other paths
    if (found_selected) {
        let zone = get_zone(params.selected_zone);
        let first_point = get_point(zone.points_start);
        var last_point = get_point(zone.points_start + zone.point_count - 1);

        // Determine if we have joined with another path.
        let joined = zone.point_count > 1 && (last_point.syncs_left || last_point.syncs_right);
        let creating = flag(6) && !joined;

        // Check completion flag, and that we haven't joined with some other path
        var start = 0u;
        if (creating) {
            start = 1u;
            last_point = first_point;
        }

        // Draw the path between control points
        for (var j = start; j < zone.point_count; j += 1u) {
            let point = get_point(zone.points_start + j);

            col = draw_bezier(
                col,
                pos,
                selected_color,
                last_point.position,
                right_handle(last_point),
                left_handle(point),
                point.position,
                PATH_HALF_WIDTH
            );

            last_point = point;
        }

        // If we stopped drawing the path early, draw a ghost to complete the path at the cursor or the first point instead
        if (creating) {
            var pos2 = params.cursor_pos;
            var pos3 = params.cursor_pos;

            // If first point is selected we are completing the path now, so draw there instead
            if (params.selected_point == zone.points_start && zone.point_count > 1u) {
                pos2 = left_handle(first_point);
                pos3 = first_point.position;
            }

            col = draw_bezier(
                col,
                pos,
                base_color,
                last_point.position,
                right_handle(last_point),
                pos2,
                pos3,
                PATH_HALF_WIDTH * 0.5
            );
        }

        // If flag 5 is set, also draw the control points on top of the path
        if (flag(5)) {
            for (var j = 0u; j < zone.point_count; j += 1u) {
                let point = get_point(zone.points_start + j);

                // If the point is not selected just draw a box
                if (j + zone.points_start != params.selected_point) {
                    col = draw_box(col, pos, selected_color, point.position, CONTROL_POINT_HALF_WIDTH);
                }

                // Otherwise, draw the handles and then the box
                else {
                    if (!point.no_handles) {
                        // Don't draw the left handle if we are branching off a path
                        if (!(creating && params.selected_point != zone.points_start && !point.syncs_left)) {
                            let left_handle = left_handle(point);
                            col = draw_line(col, pos, left_handle_color, point.position, left_handle, CONTROL_POINT_HANDLE_HALF_WIDTH);
                            col = draw_circle(col, pos, left_handle_color, left_handle, CONTROL_POINT_HANDLE_RADIUS, 0.0, true);
                        }

                        let right_handle = right_handle(point);
                        col = draw_line(col, pos, right_handle_color, point.position, right_handle, CONTROL_POINT_HANDLE_HALF_WIDTH);
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