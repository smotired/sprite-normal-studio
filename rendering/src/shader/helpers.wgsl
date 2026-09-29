/* helpers.wgsl
 * Contains helper functions that might be useful for multiple
 * parts of the shader.
 *******************************************************************/

const FEATHER = 1.5;       // Width of soft edge in pixels, should be no less than sqrt(2) probably as that's a full pixel along the diagonal

fn feather_color(
    color:     vec4<f32>,     // Target color at full opacity
    distance:  f32,           // The distance to the path we are feathering
    width:     f32,           // The width of the path we are feathering
) -> vec4<f32> {
    // Feather based on distance
    // smoothstep(a, b, x) maps [a, b] to [0, 1] and returns x's value in that space (or 0 or 1 if outside).
    // If distance is below the width, it's within the path, so opacity = 1.
    // If distance is above the width plus the feather range, it's fully off the path, so opacity = 0.
    let opacity = smoothstep((width + FEATHER) * params.inv_scale, width * params.inv_scale, abs(distance));

    // Return the color
    return vec4<f32>(color.rgb, color.a * opacity);
}