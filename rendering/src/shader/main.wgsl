/* main.wgsl
 * Contains the shader's main function, which uses the functions in
 * spritesheet.wgsl and bindings.wgsl to draw each viewport pixel.
 *******************************************************************/
 
// Marks this function as a compute entry point.
// Each workgroup is a 16x16 block of threads (256 in total).
// Dispatch ceil(width / 16) x ceil(height / 16) workgroups.
@compute @workgroup_size(16, 16, 1)
fn main(
    // Position in the full grid of threads across all workgroups, from top left.
    @builtin(global_invocation_id) id: vec3<u32>
) {
    let size = textureDimensions(output);
    let half = size / 2;

    // Ensure this pixel is bounded to the viewport image
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }

    // Get the world-space pixel coordinates from camera info
    let screen_offset = vec2<f32>(id.xy) - vec2<f32>(half);
    let pos = screen_offset * params.inv_scale + params.camera_pos;

    // Get information about the light
    let light_distance_2d = length(params.light_pos.xy - pos.xy);

    // Get the base color of this pixel from spritesheet.
    var color = spritesheet_color(pos);

    // Add in the overlay
    let overlay = overlay_color(pos);
    color = mix(color, overlay.rgb, overlay.a);

    // Output final color to viewport texture
    textureStore(output, id.xy, vec4<f32>(color, 1.0));
}