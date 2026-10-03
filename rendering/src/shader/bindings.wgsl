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
    // Inverse scale of pixels in the camera. If one spritesheet pixel is 2 pixels onscreen, this is 0.5
    inv_scale: f32,

    // Extra flags for which parts of the overlay are shown.
    // In order of increasing magnitude:
    // 00: Normal Map on (default true, replaces spritesheet if lighting is off)
    // 01: Lighting on (default true, shades flat if normal map is off)
    // 02: Light Button On (default true)
    // 03: Light Button Halo On (default false, true when dragging light)
    // 04: Zone paths enabled
    // 05: If the currently selected zone path should have its points drawn
    // 06: If the currently selected zone path should not be closed (i.e. if the first and last points are not connected. only true when creating a new path)
    // 07: 
    // 08: 
    // 09: 
    // 10: 
    // 11: 
    // 12: 
    // 13: 
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

    // Selected zone index (> 65535 if none)
    selected_zone: u32,

    // Selected point index (> 65535 if none)
    selected_point: u32,
}
@group(0) @binding(1) var<uniform> params: Params;

// The input textures for the sprite and normal map
@group(0) @binding(2) var sprite: texture_2d<f32>;
@group(0) @binding(3) var normal: texture_2d<f32>;

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

    // The index of this control point in the list.
    // --------
    // The index of another control point. If this point is updated, its sibling must also be updated equivalently. Not used here.
    id_and_sibling_id: u32,
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

    // The index of the zone this control point is a part of.
    zone_id: u32,
    // Don't care about sibling ID or sync ID for now.
}

fn unpack_point(point: ControlPointPacked) -> ControlPoint {
    let mode_and_sync_mode = unpack_fst(point.mode_and_sync_mode_and_zone_id);
    let no_handles = (mode_and_sync_mode & 0x2u) > 0u;

    return ControlPoint(
        point.position,
        point.left_handle,
        point.right_handle,
        no_handles,
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