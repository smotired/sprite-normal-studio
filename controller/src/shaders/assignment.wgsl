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

    // Top left corner of the region to reassign
    stale_min: vec2<f32>,

    // Bottom right corner of the region to reassign
    stale_max: vec2<f32>,
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

    // Start index of this zone's shapes
    // --------
    // Amount of shapes in this zone
    shapes_start_and_shape_count: u32,

    // Start index of this zone's shape vertices
    // --------
    // Amount of shape vertices in this zone
    vertices_start_and_vertex_count: u32,

    // The struct is rounded up to the 16 byte alignment of the vec3 anyway. Must match Zone in zone.rs.
    _padding0: u32,
    _padding1: u32,
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

// How many subsamples of each pixel are tested, in a 4x4 grid
const SUBSAMPLES = 16u;

// No zone owns a subsample
const NO_ZONE = 0xFFFFFFFFu;

// Which of a pixel's subsamples are inside a zone, as a bit mask. Each subsample is in the middle of its sixteenth of the pixel.
fn pixel_coverage_mask(
    pxl: vec2<u32>,
    zone: Zone,
) -> u32 {
    var mask = 0u;
    let offset = 0.125;
    for (var i = 0u; i < 4u; i += 1u) {
        for (var j = 0u; j < 4u; j += 1u) {
            let pos = vec2<f32>(pxl) + vec2<f32>(f32(i), f32(j)) * 0.25 + offset;
            if (check_point_in_zone(pos, zone)) {
                mask |= 1u << (i * 4u + j);
            }
        }
    }
    return mask;
}

// Assign a pixel to the two zones that own the most of it.
// Zones later in the list are in front, so each subsample belongs to the last zone covering it, and zones behind it don't show through.
// Along a front zone's edge, the pixel is shared with whatever shows beside it, so edges are still smooth.
fn assign_pixel(pxl: vec2<u32>) -> Output {
    var owner: array<u32, 16>;
    for (var s = 0u; s < SUBSAMPLES; s += 1u) { owner[s] = NO_ZONE; }

    for (var i = 0u; i < params.zone_count; i += 1u) {
        let mask = pixel_coverage_mask(pxl, get_zone(i));
        if (mask == 0u) { continue; }
        for (var s = 0u; s < SUBSAMPLES; s += 1u) {
            if ((mask & (1u << s)) != 0u) { owner[s] = i; }
        }
    }

    // Count each owner's subsamples once, at its first subsample, keeping the two with the most. Ties go to the zone in front.
    var result = Output(0u, 0u, 1.0, 0.0, false, false);
    for (var s = 0u; s < SUBSAMPLES; s += 1u) {
        let zone_id = owner[s];
        if (zone_id == NO_ZONE) { continue; }
        var counted = false;
        for (var t = 0u; t < s; t += 1u) {
            if (owner[t] == zone_id) { counted = true; }
        }
        if (counted) { continue; }

        var count = 0u;
        for (var t = s; t < SUBSAMPLES; t += 1u) {
            if (owner[t] == zone_id) { count += 1u; }
        }
        let coverage = f32(count) / f32(SUBSAMPLES);

        let beats_first = !result.has_zone1 || coverage > result.coverage1 || (coverage == result.coverage1 && zone_id > result.zone1_id);
        let beats_second = !result.has_zone2 || coverage > result.coverage2 || (coverage == result.coverage2 && zone_id > result.zone2_id);
        if (beats_first) {
            if (result.has_zone1) {
                result.zone2_id = result.zone1_id;
                result.coverage2 = result.coverage1;
                result.has_zone2 = true;
            }
            result.zone1_id = zone_id;
            result.coverage1 = coverage;
            result.has_zone1 = true;
        } else if (beats_second) {
            result.zone2_id = zone_id;
            result.coverage2 = coverage;
            result.has_zone2 = true;
        }
    }
    return result;
}

/***********************************/
/*         FEATHERED EDGES         */
/***********************************/

// Where a curve's feather width starts in its control point's flags. Must match FEATHER_SHIFT in point.rs.
const FEATHER_SHIFT = 2u;
const PI = 3.14159265;

// How far the curve starting at a point is feathered, in pixels. The flags are the high byte of the first u16.
fn point_feather(point_id: u32) -> f32 {
    let flags = (unpack_fst(points[point_id].mode_and_sync_mode_and_zone_id) >> 8u) & 0xFFu;
    return f32(flags >> FEATHER_SHIFT);
}

// Point on a cubic bezier curve
fn bezier_at(pos0: vec2<f32>, pos1: vec2<f32>, pos2: vec2<f32>, pos3: vec2<f32>, t: f32) -> vec2<f32> {
    let a0 = lerp(pos0, pos1, t);
    let a1 = lerp(pos1, pos2, t);
    let a2 = lerp(pos2, pos3, t);
    return lerp(lerp(a0, a1, t), lerp(a1, a2, t), t);
}

// Blend a pixel's zone with another zone across a feathered curve of that zone, if one reaches the pixel.
// Each zone has its own copy of an edge it shares, so the pixel's zone only looks at the other zones' curves.
// The feather reaches its full width halfway along the curve and tapers to nothing at its control points,
// and the two zones are 50/50 on the curve, fading to all the pixel's own zone at the edge of the feather.
fn feather_pixel(pxl: vec2<u32>, assigned: Output) -> Output {
    if (!assigned.has_zone1) { return assigned; }
    let pos = vec2<f32>(pxl) + 0.5;

    var best_distance = 1e9;
    var best_own = 1.0;
    var partner = NO_ZONE;
    for (var i = 0u; i < params.zone_count; i += 1u) {
        if (i == assigned.zone1_id) { continue; }
        let zone = get_zone(i);
        if (zone.point_count < 2u) { continue; }

        for (var j = 0u; j < zone.point_count; j += 1u) {
            let start_id = zone.points_start + j;
            let width = point_feather(start_id);
            if (width == 0.0) { continue; }

            let start = get_point(start_id);
            let end = get_point(zone.points_start + (j + 1u) % zone.point_count);
            let p0 = start.position;
            let p1 = right_handle(start);
            let p2 = left_handle(end);
            let p3 = end.position;

            // Skip curves too far away to reach
            let low = min(min(p0, p1), min(p2, p3)) - width;
            let high = max(max(p0, p1), max(p2, p3)) + width;
            if (any(pos < low) || any(pos > high)) { continue; }

            // Closest point on the curve, split into straight pieces
            var last = p0;
            for (var k = 1u; k <= 16u; k += 1u) {
                let next = bezier_at(p0, p1, p2, p3, f32(k) / 16.0);
                let along = next - last;
                let length_squared = dot(along, along);
                var u = 0.0;
                if (length_squared > 0.0) { u = clamp(dot(pos - last, along) / length_squared, 0.0, 1.0); }
                let distance = length(pos - (last + along * u));
                let reach = width * sin(PI * (f32(k - 1u) + u) / 16.0);
                if (distance < reach && distance < best_distance) {
                    best_distance = distance;
                    best_own = 0.5 + 0.5 * smoothstep(0.0, reach, distance);
                    partner = i;
                }
                last = next;
            }
        }
    }

    if (partner == NO_ZONE) { return assigned; }
    return Output(assigned.zone1_id, partner, best_own, 1.0 - best_own, true, true);
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

    // Only reassign pixels in the stale region
    let pxl = vec2<f32>(id.xy);
    if (any(pxl < params.stale_min) || any(pxl >= params.stale_max)) {
        return;
    }

    textureStore(output, id.xy, pack_output(feather_pixel(id.xy, assign_pixel(id.xy))));
}