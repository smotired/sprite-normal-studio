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

// Returns true if any point in the zone is selected.
// The pen tool only selects points of the zone it is creating, so this identifies that zone.
fn zone_has_selection(points_start: u32, point_count: u32) -> bool {
    for (var j = 0u; j < point_count; j += 1u) {
        if (is_selected(points_start + j)) { return true; }
    }
    return false;
}

fn overlay_zone_paths(
    color: vec3<f32>,   // Base color below this part of the overlay
    pos: vec2<f32>,     // World-space position of this pixel
) -> vec3<f32> {        // Returns new color after adding overlay
    var col = color;

    let base_color = vec4<f32>(1, 1, 1, 0.5);
    let selected_color = vec4<f32>(1, 0.5, 0, 1);
    let left_handle_color = vec4<f32>(1, 0, 0, 1);
    let right_handle_color = vec4<f32>(1, 1, 0, 1);

    // If only a single point is selected, draw its handles
    var selected_zone = 65536u;
    var selected_point = 65536u;
    var single_point_selected = false;
    
    // Loop through all zones
    for (var i = 0u; i < params.zone_count; i += 1u) {
        let zone = get_zone(i);
        var first_point = get_point(zone.points_start);
        var last_point = get_point(zone.points_start + zone.point_count - 1);
        var last_color = base_color;
        if (sibling_is_selected(zone.points_start + zone.point_count - 1)) {
            last_color = selected_color;
        }

        // Determine if we have joined with another path.
        let joined = zone.point_count > 1 && (last_point.syncs_left || last_point.syncs_right);
        let creating = flag(6) && !joined && zone_has_selection(zone.points_start, zone.point_count);

        // Check completion flag, and that we haven't joined with some other path
        var start = 0u;
        if (creating) {
            start = 1u;
            last_point = first_point;
            last_color = base_color;
            if (sibling_is_selected(zone.points_start)) { last_color = selected_color; }
        }

        // The loop below skips the first point while creating, so check whether it is the selected point here
        if (creating && flag(5) && is_selected(zone.points_start) && selected_point == 65536u) {
            selected_zone = i;
            selected_point = zone.points_start;
            single_point_selected = true;
        }

        // Draw the path and control point
        for (var j = start; j < zone.point_count; j += 1u) {
            let point = get_point(zone.points_start + j);
            let selected = sibling_is_selected(zone.points_start + j);

            var new_color = base_color;
            if (sibling_is_selected(zone.points_start + j)) {
                new_color = selected_color;
            }

            // Draw the path leading up to the point
            col = draw_bezier(
                col,
                pos,
                last_color,
                new_color,
                last_point.position,
                right_handle(last_point),
                left_handle(point),
                point.position,
                PATH_HALF_WIDTH
            );

            // Draw the control point if enabled
            if (flag(5)) {
                col = draw_box(col, pos, new_color, point.position, CONTROL_POINT_HALF_WIDTH);

                // Determine if this is the only selected point
                if (is_selected(zone.points_start + j)) {
                    if (single_point_selected == false) {
                        if (selected_point == 65536) {
                            selected_zone = i;
                            selected_point = zone.points_start + j;
                            single_point_selected = true;
                        }
                    } else {
                        single_point_selected = false;
                    }
                }
            }

            last_point = point;
            last_color = new_color;
        }

        // If we stopped drawing the path early, draw a ghost to complete the path at the cursor or the first point instead
        if (creating) {
            var pos2 = params.cursor_pos;
            var pos3 = params.cursor_pos;

            // If first point is selected we are completing the path now, so draw there instead
            if (is_selected(zone.points_start) && zone.point_count > 1u) {
                pos2 = left_handle(first_point);
                pos3 = first_point.position;
            }

            col = draw_bezier(
                col,
                pos,
                base_color,
                base_color,
                last_point.position,
                right_handle(last_point),
                pos2,
                pos3,
                PATH_HALF_WIDTH * 0.5
            );
            
            // Also redraw the first point
            col = draw_box(col, pos, selected_color, first_point.position, CONTROL_POINT_HALF_WIDTH);
        }
    }

    // If a single point is selected, draw its handles and point on top of everything else
    if (flag(5) && single_point_selected) {
        // Get the zone and its final point
        let zone = get_zone(selected_zone);
        var last_point = get_point(zone.points_start + zone.point_count - 1);

        // Determine if we have joined with another path.
        let joined = zone.point_count > 1 && (last_point.syncs_left || last_point.syncs_right);
        let creating = flag(6) && !joined && zone_has_selection(zone.points_start, zone.point_count);

        // Draw the handles
        let point = get_point(selected_point);
        if (!point.no_handles) {
            // Don't draw the left handle if we are branching off a path
            if (!(creating && selected_point == zone.points_start && point.syncs_left)) {
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