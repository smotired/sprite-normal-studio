// Compute shader to create the normal map from the base, zone assignment map, zones, and shapes.

// Input and Output textures
@group(0) @binding(0) var normal: texture_2d<f32>;
@group(0) @binding(1) var assign: texture_2d<u32>;
@group(0) @binding(2) var output: texture_storage_2d<rgba8unorm, write>;

@compute @workgroup_size(16, 16, 1)
fn main(
    @builtin(global_invocation_id) id: vec3<u32>
) {
    // Output texture should be the size of the assignment texture, which should be the size of the spritesheet
    let size = textureDimensions(assign);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }

    // Get information about the assignment
    let assignment = textureLoad(assign, id.xy, 0);
    let z1 = assignment.r;
    let z2 = assignment.g;
    let ratio = f32(assignment.b) / 65535.0;

    let flags = assignment.a;
    let has_zone1 = (flags & 0x0001) > 0;
    let has_zone2 = (flags & 0x0002) > 0;

    // Get the base normal map color ignoring alpha
    var norm = vec3<f32>(0, 0, 1);
    let in_size = textureDimensions(normal);
    if (id.x < in_size.x && id.y < in_size.y) {
        norm = textureLoad(normal, id.xy, 0).xyz;
        norm = normalize(norm * 2 - 1);
    }

    // Temp: If it's in a zone, set normal to flat
    if (has_zone1) {
        norm = vec3<f32>(1, 0, 0);
    } else {
        norm = vec3<f32>(-1, 0, 0);
    }

    // TODO: Get shapes in the pixel's zone(s) and apply

    // Convert to color and write to the output texture
    textureStore(output, id.xy, vec4<f32>((norm + 1) * 0.5, 1.0));
}