/* bindings.wgsl
 * Contains buffer bindings and their structs for the viewport.
 * Group 1: Output texture, camera/light uniform, spritesheet, and
 *          normal map texture.
 * Group 2: Zone, path, and shape buffers.
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
    // 00: Normal Map on (default false, replaces spritesheet)
    // 01: Lighting on (default true, does nothing if flag 00 is set)
    // 02: Light Button On (default true)
    // 03: Light Button Halo On (default false, true when dragging light)
    // 04: Zone paths enabled
    // 05: 
    // 06: 
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
}
@group(0) @binding(1) var<uniform> params: Params;

// The input textures for the sprite and normal map
@group(0) @binding(2) var sprite: texture_2d<f32>;
@group(0) @binding(3) var normal: texture_2d<f32>;

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
    points_start_and_points_count: u32,
}
@group(1) @binding(0) var<storage, read> zones: array<ZonePacked>;

struct Zone {
    // Normal vector of this zone's face
    normal: vec3<f32>,

    // Start index of the points in this path
    points_start: u32,

    // Amount of points in this path
    points_count: u32,
}

fn unpack_zone(zone: ZonePacked) -> Zone {
    return Zone(
        zone.normal,
        zone.points_start_and_points_count & 0xFFFFu,
        zone.points_start_and_points_count >> 16u,
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
    
    // The handle mode of the control point, used for rendering and control.
    // 0 = Continuous
    // 1 = Broken
    // 2 = Linear
    // For our purposes, continuous == broken
    // --------
    // The index of the zone this control point is a part of.
    mode_and_zone_id: u32,

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
    
    // The handle mode of the control point, used for rendering and control.
    // 0 = Continuous
    // 1 = Broken
    // 2 = Linear
    // For our purposes, continuous == broken
    mode: u32,

    // The index of the zone this control point is a part of.
    zone_id: u32,

    // The index of this control point.
    id: u32,

    // Don't care about sibling ID for now.
}

fn unpack_point(point: ControlPointPacked) -> ControlPoint {
    return ControlPoint(
        point.position,
        point.left_handle,
        point.right_handle,
        point.mode_and_zone_id & 0xFFFFu,
        point.mode_and_zone_id >> 16u,
        point.id_and_sibling_id & 0xFFFFu,
    );
}