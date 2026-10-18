// Compute shader to create the normal map from the base, zone assignment map, zones, and shapes.

// Input and Output textures
@group(0) @binding(0) var normal: texture_2d<f32>;
@group(0) @binding(1) var assign: texture_2d<u32>;
@group(0) @binding(2) var output: texture_storage_2d<rgba8unorm, write>;

/***********************************/
/*         UNIFORM BUFFER          */
/***********************************/

struct Params {
    // Top left corner of the region to regenerate
    stale_min: vec2<f32>,

    // Bottom right corner of the region to regenerate
    stale_max: vec2<f32>,
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

    // Start index of this zone's shapes
    // --------
    // Amount of shapes in this zone
    shapes_start_and_shape_count: u32,

    // Start index of this zone's shape vertices
    // --------
    // Amount of shape vertices in this zone
    vertices_start_and_vertex_count: u32,

    // The struct is rounded up to the 16 byte alignment of the vec3 anyway. Must match ZoneGpu in zone.rs.
    _padding0: u32,
    _padding1: u32,

    // Top left (xy) and bottom right (zw) corners around the zone's path, including its handles
    path_bounds: vec4<f32>,

    // Top left (xy) and bottom right (zw) corners around everything the zone's shapes reach. Empty (min > max) if it has none.
    shapes_bounds: vec4<f32>,
}
@group(1) @binding(0) var<storage, read> zones: array<ZonePacked>;

struct Zone {
    // Normal vector of this zone's face
    normal: vec3<f32>,

    // Start index of the points in this path
    points_start: u32,

    // Amount of points in this path
    point_count: u32,

    // Start index of this zone's shapes
    shape_start: u32,

    // Amount of shapes in this zone
    shape_count: u32,

    // Start index of this zone's shape vertices, which shapes refer to by their index from here
    vertex_start: u32,
}

fn unpack_zone(zone: ZonePacked) -> Zone {
    return Zone(
        zone.normal,
        unpack_fst(zone.points_start_and_point_count),
        unpack_snd(zone.points_start_and_point_count),
        unpack_fst(zone.shapes_start_and_shape_count),
        unpack_snd(zone.shapes_start_and_shape_count),
        unpack_fst(zone.vertices_start_and_vertex_count),
    );
}

/***********************************/
/*             SHAPES              */
/***********************************/

// Shape struct. Must match Shape in shape.rs.
struct ShapePacked {
    // u16: The zone this shape is a part of
    // --------
    // u8: The profile of the shape. 0 = Cone, 1 = Dome
    // --------
    // u8: Flags. Bit 0 is set for shapes in a path that is still being drawn, which don't affect normals.
    zone_profile_flags: u32,

    // The first and second vertices, as their index in the zone's vertices plus one. 0 means there is no vertex.
    vertices_a_and_b: u32,

    // u16: The third vertex, the same way
    // --------
    // u16: The shape's group in its zone. A run of shapes with the same group is one surface.
    vertex_c_and_group: u32,

    // How far the flat core extends out from the hull of the vertices
    inner_radius: f32,

    // How far the surface falls from the edge of the core to the ground
    outer_radius: f32,

    // Height of the shape as a ratio of its outer radius
    steepness: f32,

    // Width of the soft edge where the shape meets the ground and other shapes
    blend: f32,
}
@group(1) @binding(1) var<storage, read> shapes: array<ShapePacked>;

// Positions of the vertices shapes are made from, in zone order
@group(1) @binding(2) var<storage, read> shape_vertices: array<vec2<f32>>;

const PROFILE_DOME = 1u;
const SHAPE_PENDING = 1u;

// Largest slope of a dome, which would otherwise be vertical at its edge. Must match MAX_DOME_SLOPE in shape.rs.
const MAX_DOME_SLOPE = 3.0;

// How far out a dome's circle goes, as a fraction of its outer radius, before it reaches MAX_DOME_SLOPE. Must match dome_cap_t in shape.rs.
fn dome_cap_t() -> f32 {
    return MAX_DOME_SLOPE / sqrt(1.0 + MAX_DOME_SLOPE * MAX_DOME_SLOPE);
}

struct Shape {
    // Amount of vertices, from 1 to 3
    vertex_count: u32,

    // Positions of the vertices. Missing ones are at the first vertex.
    a: vec2<f32>,
    b: vec2<f32>,
    c: vec2<f32>,

    profile: u32,
    pending: bool,
    group: u32,
    inner_radius: f32,
    outer_radius: f32,
    steepness: f32,
    blend: f32,
}

// Get a shape, looking up its vertices from the start of its zone's vertices
fn get_shape(shape_id: u32, vertex_start: u32) -> Shape {
    let shape = shapes[shape_id];
    let slot_a = unpack_fst(shape.vertices_a_and_b);
    let slot_b = unpack_snd(shape.vertices_a_and_b);
    let slot_c = unpack_fst(shape.vertex_c_and_group);

    let a = shape_vertices[vertex_start + slot_a - 1u];
    var b = a;
    var c = a;
    var vertex_count = 1u;
    if (slot_b != 0u) { b = shape_vertices[vertex_start + slot_b - 1u]; vertex_count = 2u; }
    if (slot_c != 0u) { c = shape_vertices[vertex_start + slot_c - 1u]; vertex_count = 3u; }

    return Shape(
        vertex_count,
        a, b, c,
        (shape.zone_profile_flags >> 16u) & 0xFFu,
        ((shape.zone_profile_flags >> 24u) & SHAPE_PENDING) != 0u,
        unpack_snd(shape.vertex_c_and_group),
        shape.inner_radius,
        shape.outer_radius,
        shape.steepness,
        shape.blend,
    );
}

// How far from the hull a shape affects normals. Must match ShapeParams::reach in shape.rs.
fn shape_reach(shape: Shape) -> f32 {
    // Where the profile meets the ground, then past that until the foot is a blend width below it
    var ground = shape.outer_radius;
    var end_slope = 1.0;
    if (shape.profile == PROFILE_DOME) {
        let t = dome_cap_t();
        ground = shape.outer_radius * (t + sqrt(1.0 - t * t) / MAX_DOME_SLOPE);
        end_slope = MAX_DOME_SLOPE;
    }
    return shape.inner_radius + ground + shape.blend / (shape.steepness * end_slope);
}

/***********************************/
/*         SHAPE DISTANCES         */
/***********************************/

// A distance and the direction it is measured in
struct Distance {
    distance: f32,
    direction: vec2<f32>,
}

// Get the distance from a point to a position, and the direction from the point to the position, or zero if they are the same
fn away_from(point: vec2<f32>, pos: vec2<f32>) -> Distance {
    let offset = pos - point;
    let distance = length(offset);
    if (distance > 1e-6) {
        return Distance(distance, offset / distance);
    }
    return Distance(distance, vec2<f32>(0.0));
}

// Get the closest point to a position on a line segment
fn closest_on_segment(start: vec2<f32>, end: vec2<f32>, pos: vec2<f32>) -> vec2<f32> {
    let segment = end - start;
    let length_squared = dot(segment, segment);
    if (length_squared <= 0.0) {
        return start;
    }
    let t = clamp(dot(pos - start, segment) / length_squared, 0.0, 1.0);
    return start + segment * t;
}

// 2D cross product
fn cross2(a: vec2<f32>, b: vec2<f32>) -> f32 {
    return a.x * b.y - a.y * b.x;
}

// Get the signed distance from a position to the hull of a shape's vertices, and the direction away from the hull.
// Inside a triangle the distance to its edges is negative, and the direction is zero. Must match hull_distance in shape.rs.
fn hull_distance(shape: Shape, pos: vec2<f32>) -> Distance {
    if (shape.vertex_count == 1u) {
        return away_from(shape.a, pos);
    }
    if (shape.vertex_count == 2u) {
        return away_from(closest_on_segment(shape.a, shape.b, pos), pos);
    }

    // Distance to the closest edge
    var closest = away_from(closest_on_segment(shape.a, shape.b, pos), pos);
    let to_bc = away_from(closest_on_segment(shape.b, shape.c, pos), pos);
    if (to_bc.distance < closest.distance) { closest = to_bc; }
    let to_ca = away_from(closest_on_segment(shape.c, shape.a, pos), pos);
    if (to_ca.distance < closest.distance) { closest = to_ca; }

    // Inside if the position is on the same side of every edge, whichever way the triangle winds
    let side_ab = cross2(shape.b - shape.a, pos - shape.a);
    let side_bc = cross2(shape.c - shape.b, pos - shape.b);
    let side_ca = cross2(shape.a - shape.c, pos - shape.c);
    let inside = (side_ab >= 0.0 && side_bc >= 0.0 && side_ca >= 0.0) || (side_ab <= 0.0 && side_bc <= 0.0 && side_ca <= 0.0);
    if (inside) {
        return Distance(-closest.distance, vec2<f32>(0.0));
    }
    return closest;
}

// Get the distance from a position to the edge of a shape's flat core, and the direction away from the core.
// Both are zero inside the core. Must match core_distance in evaluate.rs.
fn core_distance(shape: Shape, pos: vec2<f32>) -> Distance {
    let hull = hull_distance(shape, pos);
    let distance = max(hull.distance - shape.inner_radius, 0.0);
    if (distance > 0.0) {
        return Distance(distance, hull.direction);
    }
    return Distance(0.0, vec2<f32>(0.0));
}

/***********************************/
/*         SHAPE BLENDING          */
/***********************************/

// A height and its gradient
struct Height {
    height: f32,
    gradient: vec2<f32>,
}

// Get the height of a surface a distance away from its core, and its gradient, both in world pixels with +y down.
// The height is 0 past the surface's reach, where it meets the ground. Must match surface_height in evaluate.rs.
fn surface_height(shape: Shape, core: Distance) -> Height {
    // Height and slope of the profile, which continue below the ground past the outer radius
    let radius = shape.outer_radius;
    let steepness = shape.steepness;
    let blend = shape.blend;
    let distance = core.distance;
    let t = distance / radius;

    // Soft shoulder: within a blend width of the core the cone rounds over, so its slope starts at 0 at the core's edge.
    // Past that it's the straight cone, so its foot and reach don't change.
    var ramp = distance;
    var ramp_slope = 1.0;
    if (distance < blend) {
        ramp = 0.5 * blend + distance * distance / (2.0 * blend);
        ramp_slope = distance / blend;
    }
    var height = steepness * (radius - ramp);
    var slope = -steepness * ramp_slope;

    // A dome is a quarter circle until its slope reaches the cap, then a straight line at the capped slope
    if (shape.profile == PROFILE_DOME) {
        let cap_t = dome_cap_t();
        if (t < cap_t) {
            let root = sqrt(1.0 - t * t);
            height = steepness * radius * root;
            slope = -steepness * t / root;
        } else {
            height = steepness * radius * sqrt(1.0 - cap_t * cap_t) - steepness * MAX_DOME_SLOPE * (distance - cap_t * radius);
            slope = -steepness * MAX_DOME_SLOPE;
        }
    }

    // Soft foot: a smooth maximum with the ground, which is exactly the ground once the height is a blend width below it
    let mix = clamp(0.5 + 0.5 * height / blend, 0.0, 1.0);
    return Height(mix * height + blend * mix * (1.0 - mix), core.direction * (mix * slope));
}

// The union of several surfaces, with each surface's own blend width. Must match Union in evaluate.rs.
// Each weight is exp(height / blend) - 1, scaled down by exp(max_exponent) so it can't overflow.
struct Union {
    max_exponent: f32,
    weight_sum: f32,
    gradient_sum: vec2<f32>,
}

// Add a surface to the union, from its shape and the distance from its core
fn union_add(previous: Union, shape: Shape, core: Distance) -> Union {
    let surface = surface_height(shape, core);
    if (surface.height <= 0.0) {
        return previous;
    }

    var result = previous;
    let exponent = surface.height / shape.blend;
    if (exponent > result.max_exponent) {
        let scale = exp(result.max_exponent - exponent);
        result.weight_sum *= scale;
        result.gradient_sum *= scale;
        result.max_exponent = exponent;
    }
    let weight = exp(exponent - result.max_exponent) - exp(-result.max_exponent);
    result.weight_sum += weight;
    result.gradient_sum += surface.gradient * weight;
    return result;
}

// Get the gradient of the surface a zone's shapes make at a position.
// A run of shapes with the same group is one surface: its cores join exactly, and it uses the first shape's params.
// The surfaces then blend into their union. Must match shape_gradient in evaluate.rs.
fn shape_gradient(zone: Zone, pos: vec2<f32>) -> vec2<f32> {
    var surfaces = Union(0.0, 0.0, vec2<f32>(0.0));

    // The group being joined, its first shape, and the closest of its cores so far
    var has_group = false;
    var group_shape: Shape;
    var group_closest = Distance(0.0, vec2<f32>(0.0));

    for (var i = 0u; i < zone.shape_count; i += 1u) {
        let shape = get_shape(zone.shape_start + i, zone.vertex_start);
        if (shape.pending) { continue; }

        // A different group finishes the one being joined
        let joining = has_group && shape.group == group_shape.group;
        if (has_group && !joining) {
            surfaces = union_add(surfaces, group_shape, group_closest);
        }

        // Shapes this far away can't change the height. A group still starts here, so its params are the first shape's.
        let reach = shape_reach(shape);
        let lower = min(min(shape.a, shape.b), shape.c) - reach;
        let upper = max(max(shape.a, shape.b), shape.c) + reach;
        var core = Distance(1e30, vec2<f32>(0.0));
        if (all(pos >= lower) && all(pos <= upper)) {
            core = core_distance(shape, pos);
        }

        if (joining) {
            if (core.distance < group_closest.distance) { group_closest = core; }
        } else {
            has_group = true;
            group_shape = shape;
            group_closest = core;
        }
    }
    if (has_group) {
        surfaces = union_add(surfaces, group_shape, group_closest);
    }

    if (surfaces.weight_sum <= 0.0) {
        return vec2<f32>(0.0);
    }
    return surfaces.gradient_sum / surfaces.weight_sum;
}

// Rotate a normal by the shortest rotation that takes +z to a base normal. Must match rotate_from_up in evaluate.rs.
fn rotate_from_up(normal: vec3<f32>, base_in: vec3<f32>) -> vec3<f32> {
    if (length(base_in) < 1e-6) {
        return normal;
    }
    let base = normalize(base_in);

    // The rotation is undefined for a base pointing straight back, which normal maps can't have anyway
    let x = base.x;
    let y = base.y;
    let z = max(base.z, -0.999);
    let a = 1.0 / (1.0 + z);
    return normalize(vec3<f32>(
        normal.x * (1.0 - x * x * a) - normal.y * x * y * a + normal.z * x,
        -normal.x * x * y * a + normal.y * (1.0 - y * y * a) + normal.z * y,
        -normal.x * x - normal.y * y + normal.z * z,
    ));
}

/***********************************/
/*        HELPER FUNCTIONS         */
/***********************************/

// Get the normal for a pixel in a zone: the surface its shapes make, turned onto the zone's base normal.
// Must match ObjectBuffers::zone_normal_at in evaluate.rs.
fn evaluate_zone(
    pxl: vec2<u32>,
    zone_id: u32,
) -> vec3<f32> {
    let zone = unpack_zone(zones[zone_id]);

    // Pixel p covers p..p+1, so evaluate at its center
    let gradient = shape_gradient(zone, vec2<f32>(pxl) + 0.5);

    // The normal map has +y up, but world space has +y down
    let normal = normalize(vec3<f32>(-gradient.x, gradient.y, 1.0));
    return rotate_from_up(normal, zone.normal);
}

/***********************************/
/*          MAIN FUNCTION          */
/***********************************/

// The base normal map's normal at a pixel, ignoring alpha, or straight out past its edge
fn base_normal(pxl: vec2<u32>) -> vec3<f32> {
    let in_size = textureDimensions(normal);
    if (pxl.x < in_size.x && pxl.y < in_size.y) {
        return normalize(textureLoad(normal, pxl, 0).xyz * 2 - 1);
    }
    return vec3<f32>(0, 0, 1);
}

@compute @workgroup_size(16, 16, 1)
fn main(
    @builtin(global_invocation_id) id: vec3<u32>
) {
    // Output texture should be the size of the assignment texture, which should be the size of the spritesheet
    let size = textureDimensions(assign);
    if (id.x >= size.x || id.y >= size.y) {
        return;
    }

    // Only regenerate pixels in the stale region
    let pxl = vec2<f32>(id.xy);
    if (any(pxl < params.stale_min) || any(pxl >= params.stale_max)) {
        return;
    }

    var norm = vec3<f32>(0, 0, 1);

    // Get information about the assignment
    let assignment = textureLoad(assign, id.xy, 0);
    let z1 = assignment.r;
    let z2 = assignment.g;
    let z1_weight = f32(assignment.b) / 65535.0;

    let flags = assignment.a;
    let has_zone1 = (flags & 0x0001) > 0;
    let has_zone2 = (flags & 0x0002) > 0;
    let zone2_is_base = (flags & 0x0004) > 0;

    // If we have a zone 1, evaluate zones
    if (has_zone1) {
        // Get the first one
        norm = evaluate_zone(id.xy, z1);

        // If we have a second zone, lerp between them according to z1_weight.
        // The second can be the part of the pixel outside every zone, which gets the base normal map.
        if (has_zone2) {
            var norm2 = vec3<f32>(0, 0, 1);
            if (zone2_is_base) {
                norm2 = base_normal(id.xy);
            } else {
                norm2 = evaluate_zone(id.xy, z2);
            }

            // Non-spherical linear interpolation.
            // If this causes noticeable artifacts, swap to slower spherical linear interpolation.
            norm = normalize(norm2 + (norm - norm2) * z1_weight);
        }
    }

    // Otherwise, use the base normal map
    else {
        norm = base_normal(id.xy);
    }

    // Convert to color and write to the output texture
    textureStore(output, id.xy, vec4<f32>((norm + 1) * 0.5, 1.0));
}