// Compute shader to create the normal map from the base, zone assignment map, zones, and shapes.

// Input and Output textures
@group(0) @binding(0) var normal: texture_2d<f32>;
@group(0) @binding(1) var assign: texture_2d<u32>;
@group(0) @binding(2) var output: texture_storage_2d<rgba8unorm, write>;

/***********************************/
/*         UNIFORM BUFFER          */
/***********************************/

struct Params {
    // Total amount of zones
    zone_count: u32,

    // Total amount of points
    point_count: u32,
}
@group(0) @binding(3) var<uniform> params: Params;

/***********************************/
/*        UNPACKING HELPERS        */
/***********************************/

// Unpack the first u16 in a packed u32
fn unpack_fst(input: u32) -> u32 { return input & 0xFFFFu; }

// Unpack the second u16 in a packed u32
fn unpack_snd(input: u32) -> u32 { return input >> 16u; }

/***********************************/
/*              ZONES              */
/***********************************/

// Zone struct
struct ZonePacked {
    // Normal vector of this zone's face
    normal: vec3<f32>,

    // Start index of the points in this path
    // --------
    // Amount of points in this path
    points_start_and_point_count: u32,
}
@group(1) @binding(0) var<storage, read> zones: array<ZonePacked>;

struct Zone {
    // Normal vector of this zone's face
    normal: vec3<f32>,

    // Start index of the points in this path
    points_start: u32,

    // Amount of points in this path
    point_count: u32,
}

fn unpack_zone(zone: ZonePacked) -> Zone {
    return Zone(
        zone.normal,
        unpack_fst(zone.points_start_and_point_count),
        unpack_snd(zone.points_start_and_point_count),
    );
}

/***********************************/
/*        HELPER FUNCTIONS         */
/***********************************/

// Get the normal map for a point in a zone
fn evaluate_zone(
    pxl: vec2<u32>,
    zone_id: u32,
) -> vec3<f32> {
    let zone = unpack_zone(zones[zone_id]);
    var norm = vec3<f32>(0, 0, 1);

    // TODO: Get and evaluate all shapes in the zone

    // Apply full zone normal map
    norm = zone.normal; // TODO: rotate instead of overriding

    return norm;
}

/***********************************/
/*          MAIN FUNCTION          */
/***********************************/

@compute @workgroup_size(16, 16, 1)
fn main(
    @builtin(global_invocation_id) id: vec3<u32>
) {
    // Output texture should be the size of the assignment texture, which should be the size of the spritesheet
    let size = textureDimensions(assign);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }

    var norm = vec3<f32>(0, 0, 1);

    // Get information about the assignment
    let assignment = textureLoad(assign, id.xy, 0);
    let z1 = assignment.r;
    let z2 = assignment.g;
    let ratio = f32(assignment.b) / 65535.0;

    let flags = assignment.a;
    let has_zone1 = (flags & 0x0001) > 0;
    let has_zone2 = (flags & 0x0002) > 0;

    // If we have a zone 1, evaluate zones
    if (has_zone1) {
        // Get the first one
        norm = evaluate_zone(id.xy, z1);

        // If we have a second zone, lerp between them according to ratio
        if (has_zone2) {
            let norm2 = evaluate_zone(id.xy, z2);

            // Non-spherical linear interpolation.
            // If this causes noticeable artifacts, swap to slower spherical linear interpolation.
            norm = normalize(norm + (norm2 - norm) * ratio);
        }
    }

    // Otherwise, use the base normal map color ignoring alpha
    else {
        let in_size = textureDimensions(normal);
        if (id.x < in_size.x && id.y < in_size.y) {
            norm = textureLoad(normal, id.xy, 0).xyz;
            norm = normalize(norm * 2 - 1);
        }
    }

    // Convert to color and write to the output texture
    textureStore(output, id.xy, vec4<f32>((norm + 1) * 0.5, 1.0));
}