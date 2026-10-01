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
    width: f32,              // Uncorrected width of the circle outline.
    filled: bool,            // If the circle should be filled with the color.
) -> vec3<f32> {
    // Correct for camera scale
    let r = radius * params.inv_scale;
    let w = width * params.inv_scale;

    // Do nothing if outside.
    let distance = length(center - pos);
    if (distance > r + w * 0.5 + FEATHER * params.inv_scale) { return base_color; }

    // Handle fill color
    if (filled && distance <= r) {
        return mix(base_color, color.rgb, color.a);
    }

    // Otherwise treat it as a circular path
    let to_circle = distance - r;
    let feathered = feather_color(color, to_circle, w * 0.5);
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