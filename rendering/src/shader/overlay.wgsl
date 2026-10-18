/* overlay.wgsl
 * Contains the functions to render the overlay to the viewport, to
 * draw things like lights, zone paths, and shapes.
 *******************************************************************/

/***********************************/
/*     SELECTION BOX OVERLAY       */
/***********************************/

fn overlay_selection_box(
    color: vec3<f32>,   // Base color below this part of the overlay
    pos: vec2<f32>,     // World-space position of this pixel
) -> vec3<f32> {        // Returns new color after adding selection box overlay
    var col = color;
    let selection_box_color = vec4<f32>(1, 1, 1, 0.1);

    // Don't draw if 0 dimensions
    let size = params.selection_box_end - params.selection_box_start;
    if (size.x == 0.0 && size.y == 0.0) {
        return col;
    }

    // Draw if within the selection box
    if (pos.x >= params.selection_box_start.x && pos.x <= params.selection_box_end.x &&
        pos.y >= params.selection_box_start.y && pos.y <= params.selection_box_end.y) {
        col = mix(col, selection_box_color.rgb, selection_box_color.a);
    }

    return col;
}

/***********************************/
/*      SPRITE GRID OVERLAY        */
/***********************************/

const SPRITE_GRID_COLOR = vec4<f32>(0.3, 0.7, 1.0, 0.5); // Color of the lines between sprites

fn overlay_sprite_grid(
    color: vec3<f32>,   // Base color below this part of the overlay
    pos: vec2<f32>,     // World-space position of this pixel
) -> vec3<f32> {        // Returns new color after adding the lines between sprites
    let sheet = vec2<f32>(textureDimensions(sprite));
    let cell = params.sprite_size;

    // Nothing to draw if there is only one sprite, or outside the spritesheet
    if (cell.x <= 0.0 || cell.y <= 0.0 || (cell.x >= sheet.x && cell.y >= sheet.y)) {
        return color;
    }
    if (pos.x < 0.0 || pos.y < 0.0 || pos.x > sheet.x || pos.y > sheet.y) {
        return color;
    }

    // Distance in world space to the closest line on each axis. Lines are one screen point wide.
    let offset = pos - round(pos / cell) * cell;
    let half_width = 0.5 * params.inv_scale;
    if (abs(offset.x) <= half_width || abs(offset.y) <= half_width) {
        return mix(color, SPRITE_GRID_COLOR.rgb, SPRITE_GRID_COLOR.a);
    }
    return color;
}

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
const FEATHERED_PATH_HALF_WIDTH = 2.0;       // Width of the band drawn under feathered paths
const FEATHERED_PATH_COLOR = vec4<f32>(0.3, 0.8, 1.0, 0.5); // Color of the band drawn under feathered paths

fn overlay_zone_paths(
    color: vec3<f32>,   // Base color below this part of the overlay
    pos: vec2<f32>,     // World-space position of this pixel
) -> vec3<f32> {        // Returns new color after adding overlay
    var col = color;

    let base_color = vec4<f32>(1, 1, 1, 0.5);
    let selected_color = vec4<f32>(1, 0.5, 0, 1);
    let left_handle_color = vec4<f32>(1, 0, 0, 1);
    let right_handle_color = vec4<f32>(1, 1, 0, 1);

    // The CPU works out which point's handles are drawn, so zones far from this pixel can be skipped
    let selected_point = params.handles_point;
    let single_point_selected = flag(5) && selected_point < params.point_count;

    // Skip zones whose path, points and lines can't reach this pixel
    let margin = (CONTROL_POINT_HALF_WIDTH + FEATHER) * params.inv_scale * 1.5;

    // Loop through all zones
    for (var i = 0u; i < params.zone_count; i += 1u) {
        let zone = get_zone(i);
        var first_point = get_point(zone.points_start);
        var last_point = get_point(zone.points_start + zone.point_count - 1);
        var last_id = zone.points_start + zone.point_count - 1;
        var last_color = base_color;
        if (sibling_is_selected(zone.points_start + zone.point_count - 1)) {
            last_color = selected_color;
        }

        // Determine if we have joined with another path.
        let joined = zone.point_count > 1 && (last_point.syncs_left || last_point.syncs_right);
        let creating = flag(6) && !joined && i + 1u == params.zone_count; // the path being created is always the last zone

        // The path being created draws a ghost to the cursor, which can be anywhere
        if (!creating && !check_bbox(pos, zone.path_min - margin, zone.path_max + margin)) {
            continue;
        }

        // Check completion flag, and that we haven't joined with some other path
        var start = 0u;
        if (creating) {
            start = 1u;
            last_point = first_point;
            last_id = zone.points_start;
            last_color = base_color;
            if (sibling_is_selected(zone.points_start)) { last_color = selected_color; }
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
            // The middle of the path is only highlighted if both ends are selected
            var mid_color = base_color;
            if (all(last_color == selected_color) && all(new_color == selected_color)) {
                mid_color = selected_color;
            }

            // Feathered paths get a wide band under them
            if (is_feathered(last_id)) {
                col = draw_bezier(
                    col, pos,
                    FEATHERED_PATH_COLOR, FEATHERED_PATH_COLOR, FEATHERED_PATH_COLOR,
                    last_point.position, right_handle(last_point), left_handle(point), point.position,
                    FEATHERED_PATH_HALF_WIDTH
                );
            }

            col = draw_bezier(
                col,
                pos,
                last_color,
                mid_color,
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
            }

            last_point = point;
            last_id = zone.points_start + j;
            last_color = new_color;
        }

        // If we stopped drawing the path early, draw a ghost to complete the path at the cursor or the first point instead
        if (creating) {
            var pos2 = params.cursor_pos;
            var pos3 = params.cursor_pos;

            // If the latest point is the first point we are completing the path now, so draw there instead
            if (flag(7) && zone.point_count > 1u) {
                pos2 = left_handle(first_point);
                pos3 = first_point.position;
            }

            col = draw_bezier(
                col,
                pos,
                base_color,
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
        let selected_zone = get_point(selected_point).zone_id;
        let zone = get_zone(selected_zone);
        var last_point = get_point(zone.points_start + zone.point_count - 1);

        // Determine if we have joined with another path.
        let joined = zone.point_count > 1 && (last_point.syncs_left || last_point.syncs_right);
        let creating = flag(6) && !joined && selected_zone + 1u == params.zone_count;

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
/*      TRANSFORM BOX OVERLAY      */
/***********************************/

// Shortest edge of the transform box, in screen points, that has a midpoint handle. Must match MIN_MIDPOINT_EDGE in the zone tool.
const TRANSFORM_MIN_MIDPOINT_EDGE = 24.0;

fn overlay_transform_box(
    color: vec3<f32>,   // Base color below this part of the overlay
    pos: vec2<f32>,     // World-space position of this pixel
) -> vec3<f32> {        // Returns new color after adding overlay
    var col = color;
    let box_color = vec4<f32>(1, 1, 1, 1);
    let center = params.transform_center;
    let half = params.transform_half_size;
    let rotation = params.transform_rotation;

    // Skip pixels nowhere near the box or its handles
    let margin = (CONTROL_POINT_HALF_WIDTH + FEATHER) * params.inv_scale * 1.5;
    let local = unrotate(pos - center, rotation);
    if (abs(local.x) > half.x + margin || abs(local.y) > half.y + margin) {
        return col;
    }

    // Corners, going around the box
    var corners = array<vec2<f32>, 4>(
        center + rotate(vec2<f32>(-half.x, -half.y), rotation),
        center + rotate(vec2<f32>( half.x, -half.y), rotation),
        center + rotate(vec2<f32>( half.x,  half.y), rotation),
        center + rotate(vec2<f32>(-half.x,  half.y), rotation),
    );

    // Edges
    for (var i = 0u; i < 4u; i += 1u) {
        col = draw_line(col, pos, box_color, corners[i], corners[(i + 1u) % 4u], PATH_HALF_WIDTH);
    }

    // Handles at each corner, and at the midpoint of each edge long enough on screen to fit one between the corners
    for (var i = 0u; i < 4u; i += 1u) {
        let next = corners[(i + 1u) % 4u];
        col = draw_rotated_box(col, pos, box_color, corners[i], CONTROL_POINT_HALF_WIDTH, rotation);
        if (length(next - corners[i]) >= TRANSFORM_MIN_MIDPOINT_EDGE * params.inv_scale) {
            col = draw_rotated_box(col, pos, box_color, (corners[i] + next) * 0.5, CONTROL_POINT_HALF_WIDTH, rotation);
        }
    }

    return col;
}

/***********************************/
/*         SHAPES OVERLAY          */
/***********************************/

const SHAPE_COLOR = vec3<f32>(0.0, 0.55, 0.55);           // Dark cyan for shapes in the current zone
const SELECTED_SHAPE_COLOR = vec3<f32>(0.45, 1.0, 1.0);   // Lighter cyan for selected shapes
const SHAPE_OPACITY = 0.5;                                // Opacity of fills, and of outlines other than a selected shape's
const OTHER_ZONE_SHAPE_OPACITY = 0.25;                    // Opacity of the outlines of shapes in other zones, which aren't filled
const SHAPE_STRIPE_WIDTH = 10.0;                          // Width in screen points of each stripe of a fill, and of each gap between them
const SHAPE_VERTEX_RADIUS = 3.0;                          // Radius of the knobs on shape vertices
const SHAPE_RADIUS_HANDLE_HALF_WIDTH = 3.0;               // Half width of the knobs for a shape's radii

// Returns true if a zone is the current zone, whose shapes are filled and drawn brighter.
// That's the active zone of a shape tool, or any zone with a selected point in other tools.
fn is_current_zone(zone_id: u32, zone: Zone) -> bool {
    if (flag(10)) {
        return zone_id == params.active_zone;
    }
    for (var j = 0u; j < zone.point_count; j += 1u) {
        if (is_selected(zone.points_start + j)) {
            return true;
        }
    }
    return false;
}

// Get the opacity of the diagonal stripes that fill shapes at a screen position, feathered at their edges.
// Every other stripe is transparent.
fn stripe_opacity(screen: vec2<f32>) -> f32 {
    let period = SHAPE_STRIPE_WIDTH * 2.0;
    let across = (screen.x + screen.y) * 0.70710678;               // Distance across the stripes
    let phase = across - floor(across / period) * period;          // Opaque from 0 to the stripe width, then transparent

    // Distance to the nearest edge of an opaque stripe, positive inside one
    var edge = min(phase, SHAPE_STRIPE_WIDTH - phase);
    if (phase >= SHAPE_STRIPE_WIDTH) {
        edge = -min(phase - SHAPE_STRIPE_WIDTH, period - phase);
    }
    return clamp(0.5 + edge / FEATHER, 0.0, 1.0);
}

// Fill the cores of the shapes in the current zone with stripes
fn fill_shapes(
    color: vec3<f32>,   // Base color below this part of the overlay
    pos: vec2<f32>,     // World-space position of this pixel
    screen: vec2<f32>,  // Screen-space position of this pixel, in points
) -> vec3<f32> {        // Returns new color after adding overlay
    // Pixels in the gaps between stripes are never filled, so they don't need to look at any shapes
    let stripe = stripe_opacity(screen);
    if (stripe <= 0.0) {
        return color;
    }

    var col = color;
    for (var i = 0u; i < params.zone_count; i += 1u) {
        let zone = get_zone(i);
        if (zone.shape_count == 0u || !check_bbox(pos, zone.shapes_min, zone.shapes_max) || !is_current_zone(i, zone)) {
            continue;
        }

        for (var j = 0u; j < zone.shape_count; j += 1u) {
            let shape = get_shape(zone.shape_start + j, zone.vertex_start);
            if (shape.pending) {
                continue;
            }

            // Skip shapes nowhere near this pixel
            let lower = min(min(shape.a, shape.b), shape.c) - shape.inner_radius;
            let upper = max(max(shape.a, shape.b), shape.c) + shape.inner_radius;
            if (!check_bbox(pos, lower, upper) || !in_shape_core(shape, pos)) {
                continue;
            }

            var fill_color = SHAPE_COLOR;
            if (is_shape_selected(shape)) {
                fill_color = SELECTED_SHAPE_COLOR;
            }
            col = mix(col, fill_color, SHAPE_OPACITY * stripe);
        }
    }
    return col;
}

// Outline the core of every shape, and the outer radius where it meets the ground
fn outline_shapes(
    color: vec3<f32>,   // Base color below this part of the overlay
    pos: vec2<f32>,     // World-space position of this pixel
) -> vec3<f32> {        // Returns new color after adding overlay
    var col = color;
    let margin = FEATHER * params.inv_scale;
    for (var i = 0u; i < params.zone_count; i += 1u) {
        let zone = get_zone(i);
        if (zone.shape_count == 0u || !check_bbox(pos, zone.shapes_min - margin, zone.shapes_max + margin)) {
            continue;
        }
        let current = is_current_zone(i, zone);

        for (var j = 0u; j < zone.shape_count; j += 1u) {
            let shape = get_shape(zone.shape_start + j, zone.vertex_start);

            // Skip shapes nowhere near this pixel
            let reach = shape.inner_radius + shape.outer_radius + margin;
            let lower = min(min(shape.a, shape.b), shape.c) - reach;
            let upper = max(max(shape.a, shape.b), shape.c) + reach;
            if (!check_bbox(pos, lower, upper)) {
                continue;
            }
            let core = hull_distance(shape, pos).distance - shape.inner_radius;

            // Shapes in a path still being drawn are plain outlines, like the path the pen is drawing
            if (shape.pending) {
                let feathered = feather_color(vec4<f32>(1, 1, 1, 0.5), core, PATH_HALF_WIDTH * params.inv_scale);
                col = mix(col, feathered.rgb, feathered.a);
                continue;
            }

            var outline_color = vec4<f32>(SHAPE_COLOR, OTHER_ZONE_SHAPE_OPACITY);
            if (current) {
                outline_color.a = SHAPE_OPACITY;
                if (is_shape_selected(shape)) {
                    outline_color = vec4<f32>(SELECTED_SHAPE_COLOR, 1.0);
                }
            }

            // The core's outline, and a fainter one where the surface meets the ground
            let feathered_core = feather_color(outline_color, core, PATH_HALF_WIDTH * params.inv_scale);
            col = mix(col, feathered_core.rgb, feathered_core.a);
            let feathered_outer = feather_color(vec4<f32>(outline_color.rgb, outline_color.a * 0.5), core - shape.outer_radius, 0.0);
            col = mix(col, feathered_outer.rgb, feathered_outer.a);
        }
    }
    return col;
}

// Draw the knobs for editing shapes: one on each of the active zone's shape vertices, and the selected group's radius knobs.
// Also draw the ghost line from the path of shapes being drawn to the cursor.
fn overlay_shape_handles(
    color: vec3<f32>,   // Base color below this part of the overlay
    pos: vec2<f32>,     // World-space position of this pixel
) -> vec3<f32> {        // Returns new color after adding overlay
    var col = color;
    let selected_color = vec4<f32>(1, 0.5, 0, 1);
    let vertex_color = vec4<f32>(1, 1, 1, 0.9);

    // The ghost line, under the knobs
    if (flag(11)) {
        col = draw_line(col, pos, vec4<f32>(1, 1, 1, 0.5), params.ghost_start, params.cursor_pos, PATH_HALF_WIDTH * 0.5);
    }

    // The selected group's radius knobs, as diamonds. The CPU works out where they go.
    if (flag(12)) {
        let diamond = vec2<f32>(0.70710678, 0.70710678);
        col = draw_rotated_box(col, pos, vec4<f32>(SELECTED_SHAPE_COLOR, 1.0), params.inner_knob, SHAPE_RADIUS_HANDLE_HALF_WIDTH, diamond);
        col = draw_rotated_box(col, pos, vec4<f32>(SELECTED_SHAPE_COLOR, 1.0), params.outer_knob, SHAPE_RADIUS_HANDLE_HALF_WIDTH, diamond);
    }

    // Knobs on the active zone's vertices, on top
    if (params.active_zone < params.zone_count) {
        let zone = get_zone(params.active_zone);
        for (var j = 0u; j < zone.vertex_count; j += 1u) {
            let vertex_id = zone.vertex_start + j;
            var knob_color = vertex_color;
            if (is_vertex_selected(vertex_id)) {
                knob_color = selected_color;
            }
            col = draw_circle(col, pos, knob_color, shape_vertices[vertex_id], SHAPE_VERTEX_RADIUS, 0.0, true);
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
    screen: vec2<f32>,        // Screen-space position of this pixel, in points
) -> vec3<f32> {              // Returns pixel color from overlay
    // Render the overlay layer by layer.
    var color = start_color;

    // Lines between sprites, below everything else
    if (flag(13)) {
        color = overlay_sprite_grid(color, pos);
    }

    // Shapes, below the zone paths
    if (flag(9)) {
        color = fill_shapes(color, pos, screen);
        color = outline_shapes(color, pos);
    }

    // Zone Interiors

    // Zone paths/points if paths are enabled
    if (flag(4)) {
        color = overlay_zone_paths(color, pos);
    }

    // Knobs for editing shapes, above the zone paths
    if (flag(9) && flag(10)) {
        color = overlay_shape_handles(color, pos);
    }

    // Transform box around the selection, below the light
    if (flag(8)) {
        color = overlay_transform_box(color, pos);
    }

    // Lights
    color = overlay_light(color, pos);

    return color;
}