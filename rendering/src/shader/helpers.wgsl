/* helpers.wgsl
 * Contains helper functions that might be useful for multiple
 * parts of the shader.
 *******************************************************************/

// Function for checking constant flag IDs
fn flag(id: u32) -> bool {
    if (id >= 32) {
        return false;
    }
    return (params.overlay_flags & u32(1 << id)) > 0;
}

// Basic linear interpolation if t is in 0..=1
fn lerp(pos0: vec2<f32>, pos1: vec2<f32>, t: f32) -> vec2<f32> {
    return pos0 + (pos1 - pos0) * t;
}

// Basic linear interpolation if t is in 0..=1
fn lerp_color(col0: vec4<f32>, col1: vec4<f32>, t: f32) -> vec4<f32> {
    return col0 + (col1 - col0) * t;
}

/***********************************/
/*         DRAWING HELPERS         */
/***********************************/

const FEATHER = 1.5;       // Width of soft edge in pixels, should be no less than sqrt(2) probably as that's a full pixel along the diagonal

fn feather_color(
    color:     vec4<f32>,     // Target color at full opacity
    distance:  f32,           // The corrected distance to the path we are feathering
    width:     f32,           // The corrected width of the path we are feathering
) -> vec4<f32> {
    // Feather based on distance
    // smoothstep(a, b, x) maps [a, b] to [0, 1] and returns x's value in that space (or 0 or 1 if outside).
    // If distance is below the width, it's within the path, so opacity = 1.
    // If distance is above the width plus the feather range, it's fully off the path, so opacity = 0.
    let opacity = smoothstep(width + FEATHER * params.inv_scale, width, abs(distance));

    // Return the color
    return vec4<f32>(color.rgb, color.a * opacity);
}

// Check if a position is within a bounding box
fn check_bbox(
    pos: vec2<f32>,  // World space position
    min: vec2<f32>,  // Top left corner of bounding box
    max: vec2<f32>,  // Bottom right corner of bounding box
) -> bool {          // True if inside the bounding box (inclusive)
    return pos.x >= min.x && pos.y >= min.y && pos.x <= max.x && pos.y <= max.y;
}

// Check if a position is within a bounding box with a feathered edge
fn check_bbox_feathered(
    pos: vec2<f32>,  // World space position
    min: vec2<f32>,  // Top left corner of bounding box
    max: vec2<f32>,  // Bottom right corner of bounding box
) -> bool {          // True if inside the bounding box (inclusive)
    let f = FEATHER * params.inv_scale;
    return check_bbox(pos, min - f, max + f);
}

// Get the t position of a point on a line segment, clamped to 0,1
fn get_line_segment_t(
    pos: vec2<f32>,          // Position to check.
    start: vec2<f32>,        // Start position of the line.
    end: vec2<f32>,          // End position of the line.
) -> f32 {
    // Project onto the segment as a fraction of its length
    let relative = pos - start;
    let segment = end - start;
    let length_squared = dot(segment, segment);
    if (length_squared <= 0.0) {
        return 0.0;
    }

    return clamp(dot(relative, segment) / length_squared, 0.0, 1.0);
}

/***********************************/
/*     SHAPE DRAWING FUNCTIONS     */
/***********************************/

// Function to draw a circle over a base color.
fn draw_circle(
    base_color: vec3<f32>,   // RGB base canvas color.
    pos: vec2<f32>,          // Position of this viewport pixel.
    color: vec4<f32>,        // RGBA paint color for the circle.
    center: vec2<f32>,       // Center of the circle.
    radius: f32,             // Uncorrected radius of the circle.
    half_width: f32,         // Uncorrected half-width of the circle outline.
    filled: bool,            // If the circle should be filled with the color.
) -> vec3<f32> {
    // Correct for camera scale
    let r = radius * params.inv_scale;
    let w = half_width * params.inv_scale;

    // Do nothing if outside.
    let distance = length(center - pos);
    if (distance > r + w + FEATHER * params.inv_scale) { return base_color; }

    // Handle fill color
    if (filled && distance <= r) {
        return mix(base_color, color.rgb, color.a);
    }

    // Otherwise treat it as a circular path
    let to_circle = distance - r;
    let feathered = feather_color(color, to_circle, w);
    return mix(base_color, feathered.rgb, feathered.a);
}

// Function to fill a box with a base color (no feather)
fn draw_box(
    base_color: vec3<f32>,   // RGB base canvas color.
    pos: vec2<f32>,          // Position of this viewport pixel.
    color: vec4<f32>,        // RGBA fill color for the box.
    center: vec2<f32>,       // Center of the box.
    half_size: f32,          // Half the uncorrected width and height of the box.
) -> vec3<f32> {
    let half = vec2<f32>(half_size, half_size) * params.inv_scale;
    if (check_bbox(pos, center - half, center + half)) {
        return mix(base_color, color.rgb, color.a);
    }
    return base_color;
}

// Function to draw a line
fn draw_line(
    base_color: vec3<f32>,   // RGB base canvas color.
    pos: vec2<f32>,          // Position of this viewport pixel.
    color: vec4<f32>,        // RGBA fill color for the box.
    start: vec2<f32>,        // Start position of the line.
    end: vec2<f32>,          // End position of the line.
    half_width: f32,         // Uncorrected half-width of the line.
) -> vec3<f32> {
    let h = half_width * params.inv_scale;
    let b_min = vec2<f32>(min(start.x, end.x), min(start.y, end.y)) - h;
    let b_max = vec2<f32>(max(start.x, end.x), max(start.y, end.y)) + h;
    if (!check_bbox_feathered(pos, b_min, b_max)) {
        return base_color;
    }

    let t = get_line_segment_t(pos, start, end);
    let point = start + t * (end - start);
    let distance = length(pos - point);

    let feathered = feather_color(color, distance, h);
    return mix(base_color, feathered.rgb, feathered.a);
}

// Distance on screen, in pixels, each end of a curve holds its color, then fades to the middle color
const CURVE_FADE_PIXELS = 40.0;

// Function to draw a cubic bezier curve
fn draw_bezier(
    base_color: vec3<f32>,   // RGB base canvas color.
    pos: vec2<f32>,          // Position of this viewport pixel.
    color_start: vec4<f32>,  // RGBA stroke color at pos0
    color_mid: vec4<f32>,    // RGBA stroke color between the fades at each end
    color_end: vec4<f32>,    // RGBA stroke color at pos3
    pos0: vec2<f32>,         // Position of the curve's 0th control point.
    pos1: vec2<f32>,         // Position of the curve's 1st control point.
    pos2: vec2<f32>,         // Position of the curve's 2nd control point.
    pos3: vec2<f32>,         // Position of the curve's 3rd control point.
    half_width: f32,         // Uncorrected half-width of the line.
) -> vec3<f32> {
    let h = half_width * params.inv_scale;
    let b_min = vec2<f32>(min(min(pos0.x, pos1.x), min(pos2.x, pos3.x)), min(min(pos0.y, pos1.y), min(pos2.y, pos3.y))) - h;
    let b_max = vec2<f32>(max(max(pos0.x, pos1.x), max(pos2.x, pos3.x)), max(max(pos0.y, pos1.y), max(pos2.y, pos3.y))) + h;
    if (!check_bbox_feathered(pos, b_min, b_max)) {
        return base_color;
    }

    // Split the curve into individual lines with de Casteljau's method.
    // Determine segment count from curvature. For a cubic, deviation is at most M / 8n^2.
    // Chord error = 0.25px means largest distance from polyline to curve is at most 0.25px.
    // M is the largest magnitude of d^2B(t)/dt^2 where B is the curve. Acceleration/tightness.
    let M = 6 * max(length(pos0 - 2 * pos1 + pos2), length(pos1 - 2 * pos2 + pos3));
    let tolerance = 0.25 * params.inv_scale;
    let segment_count = clamp(u32(ceil(sqrt(M / (8.0 * tolerance)))), 1, 64);
    let segment_length = 1.0f / f32(segment_count);

    // Draw line segments between each point and its previous point
    var last_point = pos0;
    var length_so_far = 0.0;                 // arc length of the polyline up to last_point
    var min_dist = length(pos - pos0);       // minimum distance to the curve
    var min_length = 0.0;                    // arc length along the curve of the closest point
    for (var i = 1u; i <= segment_count; i += 1u) {
        let t = f32(i) * segment_length;

        let a0 = lerp(pos0, pos1, t);
        let a1 = lerp(pos1, pos2, t);
        let a2 = lerp(pos2, pos3, t);

        let b0 = lerp(a0, a1, t);
        let b1 = lerp(a1, a2, t);

        let point = lerp(b0, b1, t);

        let segment_t = get_line_segment_t(pos, last_point, point);
        let approx = last_point + segment_t * (point - last_point);
        let distance = length(pos - approx);

        let this_length = length(point - last_point);
        if (distance < min_dist) {
            min_dist = distance;
            min_length = length_so_far + segment_t * this_length;
        }

        length_so_far += this_length;
        last_point = point;
    }

    // From each endpoint, hold its color for a fixed distance on screen, then fade to the middle color over the same distance.
    // Short curves shrink both distances so the two ends still fit.
    let fade_length = min(CURVE_FADE_PIXELS * params.inv_scale, length_so_far * 0.25);
    var min_color = color_mid;
    if (fade_length > 0.0) {
        let from_end = length_so_far - min_length;
        if (min_length < fade_length) {
            min_color = color_start;
        } else if (min_length < 2.0 * fade_length) {
            min_color = lerp_color(color_start, color_mid, (min_length - fade_length) / fade_length);
        } else if (from_end < fade_length) {
            min_color = color_end;
        } else if (from_end < 2.0 * fade_length) {
            min_color = lerp_color(color_end, color_mid, (from_end - fade_length) / fade_length);
        }
    } else {
        min_color = color_start;
    }

    // Draw with the final color
    let feathered = feather_color(min_color, min_dist, h);
    return mix(base_color, feathered.rgb, feathered.a);
}