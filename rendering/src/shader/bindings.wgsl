/* bindings.wgsl
 * Contains buffer bindings and their structs for the viewport.
 * Group 0: Output texture, camera/light uniform, spritesheet, and
 *          normal map texture.
 * Group 1: Zone, path, and shape buffers.
 *******************************************************************/

/***********************************/
/*  TEXTURES AND CAMERA UNIFRORM   */
/***********************************/

// The output image as a storage texture which allows pixel-level write access.
@group(0) @binding(0) var output: texture_storage_2d<rgba8unorm, write>;

// Uniform struct
struct Params {
    // Position of the camera center ignoring scale
    camera_pos: vec2<f32>,
    // Inverse scale of pixels in the camera. If one spritesheet pixel is 2 points onscreen, this is 0.5.
    // Overlay sizes are in points, so they are corrected by this alone.
    inv_scale: f32,

    // Extra flags for which parts of the overlay are shown.
    // In order of increasing magnitude:
    // 00: Normal Map on (default true, replaces spritesheet if lighting is off)
    // 01: Lighting on (default true, shades flat if normal map is off)
    // 02: Light Button On (default true)
    // 03: Light Button Halo On (default false, true when dragging light)
    // 04: Zone paths enabled
    // 05: If the currently selected zone path should have its points drawn
    // 06: If a path is being created. It is always the last zone, and should not be closed.
    // 07: If the latest point of the path being created is its first point, so the ghost path closes to it
    // 08: If the transform box around the selection should be drawn
    // 09: If shapes are drawn. Only set along with 04.
    // 10: If a shape tool is selected, so the active zone's shape vertices get knobs
    // 11: If a path of shapes is being drawn, so a ghost line joins its latest vertex to the cursor
    // 12: If the selected group of shapes has radius knobs, which only happens along with 10
    // 13: If the lines between the sprites the spritesheet is sliced into are drawn
    // 14: 
    // 15: 
    // 16: 
    // 17: 
    // 18: 
    // 19: 
    // 20: 
    // 21: 
    // 22: 
    // 23: 
    // 24: 
    // 25: 
    // 26: 
    // 27: 
    // 28: 
    // 29: 
    // 30: 
    // 31: 
    overlay_flags: u32,

    // Position of the point light, assuming each pixel is one unit.
    // Technically passed in as a Vec2 and another f32 but this is fine. Keeping it as a vec3 is useful for shading.
    light_pos: vec3<f32>,

    // Light color
    light_color: u32,

    // Current cursor position in world space
    cursor_pos: vec2<f32>,

    // Total amount of zones
    zone_count: u32,

    // Total amount of points
    point_count: u32,

    // Top left corner of the selection box. Should be 0,0 if no selection box.
    selection_box_start: vec2<f32>,

    // Bottom right corner of the selection box. Should be 0,0 if no selection box.
    selection_box_end: vec2<f32>,

    // Physical pixels per screen point. The output texture is in physical pixels, so this converts its pixels to points.
    pixels_per_point: f32,

    // Center of the transform box around the selection. Only drawn if flag 08 is set.
    transform_center: vec2<f32>,

    // Half the width and height of the transform box, before it is rotated
    transform_half_size: vec2<f32>,

    // Cosine and sine of the rotation of the transform box
    transform_rotation: vec2<f32>,

    // The zone a shape tool is editing, whose shape vertices get knobs. NO_OBJECT if there is none.
    active_zone: u32,

    // Aligns the ghost start to 8 bytes
    _padding_2: u32,

    // Latest vertex of the path of shapes being drawn. Only drawn if flag 11 is set.
    ghost_start: vec2<f32>,

    // The selected group's inner radius knob. Only drawn if flag 12 is set.
    inner_knob: vec2<f32>,

    // The selected group's outer radius knob. Only drawn if flag 12 is set.
    outer_knob: vec2<f32>,

    // Width and height of each sprite the spritesheet is sliced into. Lines between them are drawn if flag 13 is set.
    sprite_size: vec2<f32>,

    // The point whose handles are drawn: the only selected point, or the latest point of the path being created.
    // NO_OBJECT if there is none. Only used along with flag 05.
    handles_point: u32,

    // Pads the struct to a multiple of 16 bytes
    _padding_3: u32,
}
@group(0) @binding(1) var<uniform> params: Params;

// The input textures for the sprite and normal map
@group(0) @binding(2) var sprite: texture_2d<f32>;
@group(0) @binding(3) var normal: texture_2d<f32>;

/***********************************/
/*       SELECTION / HELPERS       */
/***********************************/

// Bits for what is selected, 32 to a texel, in rows of SELECTION_WIDTH texels. Must match pack_selection in buffers.rs.
// A texture rather than a storage buffer, since only 4 storage buffers can be bound at once.
// Bits 0..65536: selected points
// Bits 65536..131072: selected shape vertices
// Bits 131072..196608: points that are selected or have a selected sibling
@group(1) @binding(0) var selection: texture_2d<u32>;

// Texels in each row of the selection texture. Must match SELECTION_WIDTH in buffers.rs.
const SELECTION_WIDTH = 256u;

// Stands in for a zone or shape ID in the uniform when there is none. Must match NO_OBJECT in viewport.rs.
const NO_OBJECT = 0xFFFFFFFFu;

// Return true if a bit of the selection texture is set
fn selection_bit(bit: u32) -> bool {
    let texel = bit / 32u;
    let selected_bits = textureLoad(selection, vec2<u32>(texel % SELECTION_WIDTH, texel / SELECTION_WIDTH), 0).r;
    return (selected_bits & (1u << (bit % 32u))) != 0u;
}

// Return true if the point with the ID is selected
fn is_selected(point_id: u32) -> bool {
    return selection_bit(point_id);
}

// Return true if the shape vertex with the ID is selected
fn is_vertex_selected(vertex_id: u32) -> bool {
    return selection_bit(65536u + vertex_id);
}

// Returns true if a point or any of its siblings is selected. The CPU follows the sibling rings when packing the selection.
fn sibling_is_selected(point_id: u32) -> bool {
    return selection_bit(131072u + point_id);
}

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
@group(1) @binding(1) var<storage, read> zones: array<ZonePacked>;

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

    // Amount of shape vertices in this zone
    vertex_count: u32,

    // Top left and bottom right corners around the zone's path, including its handles
    path_min: vec2<f32>,
    path_max: vec2<f32>,

    // Top left and bottom right corners around everything the zone's shapes reach. Empty (min > max) if it has none.
    shapes_min: vec2<f32>,
    shapes_max: vec2<f32>,
}

fn get_zone(zone_id: u32) -> Zone {
    let zone = zones[zone_id];
    return Zone(
        zone.normal,
        unpack_fst(zone.points_start_and_point_count),
        unpack_snd(zone.points_start_and_point_count),
        unpack_fst(zone.shapes_start_and_shape_count),
        unpack_snd(zone.shapes_start_and_shape_count),
        unpack_fst(zone.vertices_start_and_vertex_count),
        unpack_snd(zone.vertices_start_and_vertex_count),
        zone.path_bounds.xy,
        zone.path_bounds.zw,
        zone.shapes_bounds.xy,
        zone.shapes_bounds.zw,
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
@group(1) @binding(2) var<storage, read> points: array<ControlPointPacked>;

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
    // a path to another path, and for drawing the handles of a selected point.
    syncs_left: bool,

    // Part of the sync mode of the control point. Used for drawing the ghost when connecting
    // a path to another path, and for drawing the handles of a selected point.
    syncs_right: bool,

    // The index of the zone this control point is a part of.
    zone_id: u32,
    
    // Don't care about sibling ID or sync IDs for now.
}

// Where a curve's feather width starts in its control point's flags. Must match FEATHER_SHIFT in point.rs.
const FEATHER_SHIFT = 2u;

// Returns true if the curve starting at a point is feathered into the zone on its other side.
// The flags are the high byte of the first u16.
fn is_feathered(point_id: u32) -> bool {
    let flags = (unpack_fst(points[point_id].mode_and_sync_mode_and_zone_id) >> 8u) & 0xFFu;
    return (flags >> FEATHER_SHIFT) != 0u;
}

fn get_point(point_id: u32) -> ControlPoint {
    let point = points[point_id];

    let mode_and_sync_mode = unpack_fst(point.mode_and_sync_mode_and_zone_id);
    let no_handles = (mode_and_sync_mode & 0x2u) > 0u;

    let left_sync_id = unpack_fst(point.left_and_right_sync_ids);
    let right_sync_id = unpack_snd(point.left_and_right_sync_ids);
    let syncs_left = left_sync_id != point_id;
    let syncs_right = right_sync_id != point_id;

    return ControlPoint(
        point.position,
        point.left_handle,
        point.right_handle,
        no_handles,
        syncs_left,
        syncs_right,
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
/*             SHAPES              */
/***********************************/

// Shape struct. Must match Shape in shape.rs and ShapePacked in generator.wgsl.
struct ShapePacked {
    // u16: The zone this shape is a part of
    // --------
    // u8: The profile of the shape. 0 = Cone, 1 = Dome
    // --------
    // u8: Flags. Bit 0 is set for shapes in a path that is still being drawn.
    zone_profile_flags: u32,

    // The first and second vertices, as their index in the zone's vertices plus one. 0 means there is no vertex.
    vertices_a_and_b: u32,

    // u16: The third vertex, the same way
    // --------
    // u16: The shape's group in its zone
    vertex_c_and_group: u32,

    // How far the flat core extends out from the hull of the vertices
    inner_radius: f32,

    // How far the surface falls from the edge of the core to the ground
    outer_radius: f32,

    // Height of the shape as a ratio of its outer radius. Unused by the overlay.
    steepness: f32,

    // Width of the soft edge where the shape meets the ground and other shapes. Unused by the overlay.
    blend: f32,
}
@group(1) @binding(3) var<storage, read> shapes: array<ShapePacked>;

// Positions of the vertices shapes are made from, in zone order
@group(1) @binding(4) var<storage, read> shape_vertices: array<vec2<f32>>;

const SHAPE_PENDING = 1u;

struct Shape {
    // Amount of vertices, from 1 to 3
    vertex_count: u32,

    // Positions of the vertices. Missing ones are at the first vertex.
    a: vec2<f32>,
    b: vec2<f32>,
    c: vec2<f32>,

    // IDs of the vertices. Missing ones are the first vertex.
    ids: vec3<u32>,

    // If the shape is in a path that is still being drawn
    pending: bool,

    inner_radius: f32,
    outer_radius: f32,
}

// Get a shape, looking up its vertices from the start of its zone's vertices
fn get_shape(shape_id: u32, vertex_start: u32) -> Shape {
    let shape = shapes[shape_id];
    let slot_a = unpack_fst(shape.vertices_a_and_b);
    let slot_b = unpack_snd(shape.vertices_a_and_b);
    let slot_c = unpack_fst(shape.vertex_c_and_group);

    var ids = vec3<u32>(vertex_start + slot_a - 1u);
    var vertex_count = 1u;
    if (slot_b != 0u) { ids.y = vertex_start + slot_b - 1u; vertex_count = 2u; }
    if (slot_c != 0u) { ids.z = vertex_start + slot_c - 1u; vertex_count = 3u; }

    return Shape(
        vertex_count,
        shape_vertices[ids.x],
        shape_vertices[ids.y],
        shape_vertices[ids.z],
        ids,
        ((shape.zone_profile_flags >> 24u) & SHAPE_PENDING) != 0u,
        shape.inner_radius,
        shape.outer_radius,
    );
}

// Returns true if every vertex of a shape is selected
fn is_shape_selected(shape: Shape) -> bool {
    return is_vertex_selected(shape.ids.x) && is_vertex_selected(shape.ids.y) && is_vertex_selected(shape.ids.z);
}
