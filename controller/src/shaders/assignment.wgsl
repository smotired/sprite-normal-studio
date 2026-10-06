// Compute shader to assign normal map textures

/***********************************/
/*     OUTPUT BINDING/HELPERS      */
/***********************************/

// Output texture
@group(0) @binding(0) var output: texture_storage_2d<rgba16uint, write>;

// Output value struct
struct Output {
    // ID of the first zone
    zone1_id: u32,
    
    // ID of the second zone
    zone2_id: u32,

    // Coverage of the first zone
    coverage1: f32,

    // Coverage of the second zone
    coverage2: f32,

    // If we have a first zone
    has_zone1: bool,

    // If we have a second zone
    has_zone2: bool,
}

// Output value packing helper
fn pack_output(output: Output) -> vec4<u32> {
    // Convert ratio to a u16
    let ratio = output.coverage1 / (output.coverage1 + output.coverage2);
    let ratio16 = u32(ratio * 65535.0);

    // Create flags
    var flags = 0u;
    if (output.has_zone1) { flags |= 1u; }
    if (output.has_zone2) { flags |= 2u; }

    return vec4<u32>(output.zone1_id, output.zone2_id, ratio16, flags);
}

/***********************************/
/*         UNIFORM BUFFER          */
/***********************************/

struct Params {
    // Total amount of zones
    zone_count: u32,

    // Total amount of points
    point_count: u32,

    // ID of the zone to ignore, and treat as if it's empty space.
    // If above 65536, out of range, don't ignore any zones.
    // This is because, if we are adding a zone it might not be closed yet.
    zone_ignore: u32,
}
@group(0) @binding(1) var<uniform> params: Params;

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

fn get_zone(zone_id: u32) -> Zone {
    let zone = zones[zone_id];
    return Zone(
        zone.normal,
        unpack_fst(zone.points_start_and_point_count),
        unpack_snd(zone.points_start_and_point_count),
    );
}

/***********************************/
/*         CONTROL POINTS          */
/***********************************/

// Control point struct
struct ControlPointPacked {
    // Position of the control point
    position: vec2<f32>,

    // Relative position of incoming handle
    left_handle: vec2<f32>,

    // Relative position of outgoing handle
    right_handle: vec2<f32>,
    
    // u8: The handle mode of the control point, used for rendering and control.
    // 0 = Continuous
    // 1 = Broken
    // 2 = Linear
    // For our purposes, continuous == broken
    // -------
    // u8: The sync mode of the control point, unused in rendering
    // --------
    // u16: The index of the zone this control point is a part of.
    mode_and_sync_mode_and_zone_id: u32,

    // The index of the control point to share the left handle with.
    // --------
    // The index of the control point to share the right handle with.
    left_and_right_sync_ids: u32,
}
@group(1) @binding(1) var<storage, read> points: array<ControlPointPacked>;

struct ControlPoint {
    // Position of the control point
    position: vec2<f32>,

    // Relative position of incoming handle
    left_handle: vec2<f32>,

    // Relative position of outgoing handle
    right_handle: vec2<f32>,
    
    // Part of the handle mode of the control point. Controls if handles should be rendered.
    // This is the only reason we care about mode.
    no_handles: bool,

    // Part of the sync mode of the control point. Used for drawing the ghost when connecting
    // a path to another path.
    is_free: bool,

    // The index of the zone this control point is a part of.
    zone_id: u32,
    
    // Don't care about sibling ID or sync IDs for now.
}

fn get_point(point_id: u32) -> ControlPoint {
    let point = points[point_id];
    let mode_and_sync_mode = unpack_fst(point.mode_and_sync_mode_and_zone_id);
    let no_handles = (mode_and_sync_mode & 0x2u) > 0u;

    let left_sync_id = unpack_fst(point.left_and_right_sync_ids);
    let right_sync_id = unpack_snd(point.left_and_right_sync_ids);
    let is_free = left_sync_id == point_id && right_sync_id == point_id;

    return ControlPoint(
        point.position,
        point.left_handle,
        point.right_handle,
        no_handles,
        is_free,
        unpack_snd(point.mode_and_sync_mode_and_zone_id),
        // leave sibling ID and sync id
    );
}

// Get the actual position of a control point's left handle
fn left_handle(point: ControlPoint) -> vec2<f32> {
    // Don't add the handle if it's linear
    if (point.no_handles) {
        return point.position;
    } else {
        return point.position + point.left_handle;
    }
}

// Get the actual position of a control point's right handle
fn right_handle(point: ControlPoint) -> vec2<f32> {
    // Don't add the handle if it's linear
    if (point.no_handles) {
        return point.position;
    } else {
        return point.position + point.right_handle;
    }
}

/***********************************/
/*        HELPER FUNCTIONS         */
/***********************************/

// Basic linear interpolation if t is in 0..=1
fn lerp(pos0: vec2<f32>, pos1: vec2<f32>, t: f32) -> vec2<f32> {
    return pos0 + (pos1 - pos0) * t;
}

// Method to check if a ray intersects a line segment.
// Assumes direction is normalized.
fn check_ray_intersect_line_segment(
    origin: vec2<f32>,
    direction: vec2<f32>,
    pos0: vec2<f32>,
    pos1: vec2<f32>,
) -> bool {
    // If parallel return false early.
    let segment = pos1 - pos0;
    if ((pos0.x == pos1.x && pos0.y == pos1.y) || abs(dot(normalize(segment), direction)) == 1.0) {
        return false;
    }

    // from https://rootllama.wordpress.com/2014/06/20/ray-line-segment-intersection-test-in-2d/
    let v1 = origin - pos0;
    let v3 = vec2<f32>(-direction.y, direction.x); // perpendicular to d
    let denom = 1.0 / dot(segment, v3);
    let t1 = (segment.x * v1.y - v1.x * segment.y) * denom; // t for ray = abs(cross(v2, v1)) / dot(v2, v3)
    let t2 = dot(v1, v3) * denom; // t for line segment = dot(v1, v3) / dot(v2, v3)

    // Intersect if the point is on both the ray and the line segment
    return t1 >= 0 && t2 >= 0 && t2 < 1;

}

// Method to check if a ray intersects a bezier curve.
// Assumes direction is normalized.
// Return the amount of times it intersects with this path.
fn count_ray_intersect_bezier(
    origin: vec2<f32>,
    direction: vec2<f32>,
    pos0: vec2<f32>,
    pos1: vec2<f32>,
    pos2: vec2<f32>,
    pos3: vec2<f32>,
) -> u32 {
    // Split into line segments and check each one.
    // see rendering/helpers.wgsl bezier drawing for what this means
    let M = 6 * max(length(pos0 - 2 * pos1 + pos2), length(pos1 - 2 * pos2 + pos3));
    let tolerance = 0.5; // tolerate half a pixel
    let segment_count = clamp(u32(ceil(sqrt(M / (8.0 * tolerance)))), 1, 64);

    // Check line segments
    var count = 0u;
    var last_point = pos0;
    for (var i = 1u; i <= segment_count; i += 1u) {
        let t = f32(i) / f32(segment_count);

        let a0 = lerp(pos0, pos1, t);
        let a1 = lerp(pos1, pos2, t);
        let a2 = lerp(pos2, pos3, t);

        let b0 = lerp(a0, a1, t);
        let b1 = lerp(a1, a2, t);

        let point = lerp(b0, b1, t);
        if (check_ray_intersect_line_segment(origin, direction, last_point, point)) {
            count += 1;
        }
        last_point = point;
    }

    // Check final segment
    if (check_ray_intersect_line_segment(origin, direction, last_point, pos3)) {
        count += 1;
    }

    return count;
}

// Method to check if a point is inside a zone
// If a ray intersects a bezier path an odd number of times, the ray's origin is inside the path.
fn check_point_in_zone(
    pos: vec2<f32>,
    zone: Zone,
) -> bool {
    // Raycast straight down because that's easy to be normalized
    let direction = vec2<f32>(0, 1);

    // Sum up intersections for curves at each control point
    var sum = 0u;
    var last_point = get_point(zone.points_start + zone.point_count - 1);
    for (var j = 0u; j < zone.point_count; j += 1u) {
        let point = get_point(zone.points_start + j);
        sum += count_ray_intersect_bezier(
            pos, direction,
            last_point.position,
            right_handle(last_point),
            left_handle(point),
            point.position
        );
        last_point = point;
    }

    // If we intersect an odd number of times, we are inside the path.
    return sum % 2u == 1u;
}

// Method to check how much of a pixel is inside a zone.
fn check_pixel_coverage(
    pxl: vec2<u32>,
    zone: Zone,
) -> f32 {
    // Check a 2x2 grid of points
    // TODO: Performance willing, expand to a 4x4. Maybe make that a setting in the future.
    var inside = 0u;
    let offset = 0.25;
    for (var i = 0u; i < 2u; i += 1u) {
        for (var j = 0u; j < 2u; j += 1u) {
            let pos = vec2<f32>(pxl) + vec2<f32>(f32(i), f32(j)) * 0.5 + offset;
            if (check_point_in_zone(pos, zone)) {
                inside += 1u;
            }
        }
    }

    return f32(inside) / 4.0;
}

// Evaluate a zone and add to the output if it's better
fn evaluate_zone(
    pxl: vec2<u32>,
    zone_id: u32,
    zone: Zone,
    previous: Output,
) -> Output {
    let coverage = check_pixel_coverage(pxl, zone);
    if (coverage == 0.0) { return previous; }

    var result = previous;
    if (!previous.has_zone1 || coverage > previous.coverage1) {
        // Move zone1 to zone2
        if (previous.has_zone1) {
            result.zone2_id = previous.zone1_id;
            result.coverage2 = previous.coverage1;
            result.has_zone2 = true;
        }

        result.zone1_id = zone_id;
        result.coverage1 = coverage;
        result.has_zone1 = true;
    }
    
    else if (!previous.has_zone2 || coverage > previous.coverage2) {
        result.zone2_id = zone_id;
        result.coverage2 = coverage;
        result.has_zone2 = true;
    }

    return result;
}

/***********************************/
/*         MAIN FUNCTION           */
/***********************************/

@compute @workgroup_size(16, 16, 1)
fn main(
    @builtin(global_invocation_id) id: vec3<u32>
) {
    let size = textureDimensions(output);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }

    // Set up the output for this pixel
    var result = Output(0u, 0u, 1.0, 0.0, false, false);

    // Loop through and evaluate all zones
    for (var i = 0u; i < params.zone_count; i += 1u) {
        if (i != params.zone_ignore) {
            result = evaluate_zone(id.xy, i, get_zone(i), result);
        }
    }

    // Pack and output
    textureStore(output, id.xy, pack_output(result));
}