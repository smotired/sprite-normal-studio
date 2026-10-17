use studio_math::Vec2;
use studio_math::bounds::bounds_correct;
use wgpu::{Device, util::DeviceExt};

use crate::{ObjectBuffers, SpecialQuery, SpecialValue};
use crate::{interaction::{camera::Camera, light::Light, overlay::OverlayState, tools::{EditorTool, TransformBox}}};

/// Formats data used for the actual rendering process.
#[repr(C)] // Needed for Rust to pass to shaders correctly
#[derive(Copy, Clone, Default, bytemuck::Pod, bytemuck::Zeroable)] // Needed to store into a buffer below
pub struct ViewportDataUniform {
    // Pixel the camera is centered on, from top left, ignoring scale
    pub camera_pos: Vec2,

    // Inverted scale of the camera. 1/2 means each sprite pixel takes up 2 screen points each direction.
    // We never need the non inverted version, but this is used frequently to normalize stuff that doesn't depend on the camera scale like the overlay.
    pub inv_scale: f32,

    // Flags for the overlay. See shader/bindings.wgsl for full documentation of each flag.
    pub overlay_flags: u32,

    // Position of the point light, assuming each pixel is one unit
    pub light_pos: Vec2,

    // Height of the light in the same units as position. Affects attenuation.
    pub light_height: f32,

    // Light color
    pub light_color: u32,
    
    // Current world space position of cursor,
    pub cursor_pos: Vec2,

    // Amount of zones
    pub zone_count: u32,

    // Amount of control points
    pub point_count: u32,

    // Top left corner of the selection box. Should be 0,0 if no selection box.
    pub selection_box_start: Vec2,

    // Bottom right corner of the selection box. Should be 0,0 if no selection box.
    pub selection_box_end: Vec2,

    // Physical pixels per screen point. The viewport texture is rendered in physical pixels, but everything else is in points.
    // Set by the renderer, since the controller doesn't know about the display.
    pub pixels_per_point: f32,

    // Aligns the transform box to 8 bytes, like the shader expects
    _padding: f32,

    // Center of the transform box around the selection. Only drawn if overlay flag 08 is set.
    pub transform_center: Vec2,

    // Half the width and height of the transform box, before it is rotated
    pub transform_half_size: Vec2,

    // Cosine and sine of the rotation of the transform box
    pub transform_rotation: Vec2,

    // The zone a shape tool is editing, whose shape vertices get knobs. NO_OBJECT if there is none.
    pub active_zone: u32,

    // Aligns the ghost start to 8 bytes, like the shader expects
    _padding_2: u32,

    // Latest vertex of the path of shapes being drawn, which a ghost line joins to the cursor. Only drawn if overlay flag 11 is set.
    pub ghost_start: Vec2,

    // The selected group's inner radius knob. Only drawn if overlay flag 12 is set.
    pub inner_knob: Vec2,

    // The selected group's outer radius knob. Only drawn if overlay flag 12 is set.
    pub outer_knob: Vec2,

    // Width and height of each sprite the spritesheet is sliced into. Lines between them are drawn if overlay flag 13 is set.
    // Set by the controller, since it isn't part of the overlay state.
    pub sprite_size: Vec2,

    // Pads the struct to a multiple of 16 bytes, like the shader expects
    _padding_3: Vec2,
}

/// Stands in for a zone or shape ID in the uniform when there is none. IDs only go up to 65535.
pub const NO_OBJECT: u32 = 0xFFFF_FFFF;

impl ViewportDataUniform {
    pub fn new(
        light: &Light,
        camera: &Camera,
        overlay: &OverlayState,
        cursor: Vec2,
        objects: &ObjectBuffers,
        tool: &dyn EditorTool,
        transform_box: Option<TransformBox>,
    ) -> Self {
        let (zone_count, point_count) = objects.object_counts();

        // Calculate selection box
        let (selection_box_start, selection_box_end) = {
            if let Some((start, end)) = tool.selection_box() {
                bounds_correct(start, end)
            } else { (Vec2::ZERO, Vec2::ZERO) }
        };

        // Draw the transform box along with the zone paths
        let mut overlay_flags = overlay.get_flags(tool);
        let transform_box = transform_box.filter(|_| overlay_flags & (1 << 4) != 0);
        if transform_box.is_some() { overlay_flags |= 1 << 8; }
        let TransformBox { center, half_size, angle } = transform_box.unwrap_or(TransformBox { center: Vec2::ZERO, half_size: Vec2::ZERO, angle: 0.0 });
        let (sin, cos) = angle.sin_cos();

        // Radius knobs are drawn with the shape tools' vertex knobs, when exactly one whole group of shapes is selected
        let knobs = objects.shapes_with_all_vertices(tool.shape_selection()).ok()
            .and_then(|shape_ids| objects.radius_knobs(&shape_ids, camera.inv_scale()).ok().flatten())
            .filter(|_| overlay_flags & (1 << 10) != 0);
        if knobs.is_some() { overlay_flags |= 1 << 12; }

        Self {
            camera_pos: camera.position,
            inv_scale: camera.inv_scale(),
            overlay_flags,
            light_pos: light.position(),
            light_height: light.height(),
            light_color: light.packed_color(),
            cursor_pos: cursor,
            zone_count: zone_count as u32,
            point_count: point_count as u32,
            selection_box_start,
            selection_box_end,
            pixels_per_point: 1.0,
            _padding: 0.0,
            transform_center: center,
            transform_half_size: half_size,
            transform_rotation: Vec2::new(cos, sin),
            active_zone: match tool.handle_special_query(SpecialQuery::ActiveZone) {
                Some(SpecialValue::Zone(Some(zone_id))) => zone_id as u32,
                _ => NO_OBJECT,
            },
            _padding_2: 0,
            ghost_start: match tool.handle_special_query(SpecialQuery::ShapeGhost) {
                Some(SpecialValue::Position(position)) => position,
                _ => Vec2::ZERO,
            },
            inner_knob: knobs.map_or(Vec2::ZERO, |knobs| knobs.inner),
            outer_knob: knobs.map_or(Vec2::ZERO, |knobs| knobs.outer),
            sprite_size: Vec2::ZERO,
            _padding_3: Vec2::ZERO,
        }
    }

    /// Create a buffer that can be used to store this uniform data.
    pub fn buffer(device: &Device) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[Self::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }

    /// Get the bytes for writing to a buffer
    pub fn bytes(&self) -> &[u8] { bytemuck::bytes_of(self) }
}


#[cfg(test)]
mod tests {
    use crate::interaction::tools::EditorToolZone;
    use crate::objects::test_utils::add_square;

    use super::*;

    /// The uniform should be filled from each source
    #[test]
    fn new() {
        let mut camera = Camera::default();
        camera.reset_position(Vec2::new(10.0, 20.0));
        camera.apply_scale(1, None);
        let tool = EditorToolZone::init();
        let overlay = OverlayState::default();
        let mut objects = ObjectBuffers::headless();
        for size in [10.0, 20.0, 30.0] { add_square(&mut objects, size); }

        let uniform = ViewportDataUniform::new(&Light::default(), &camera, &overlay, Vec2::new(5.0, 6.0), &objects, tool.as_ref(), None);
        assert_eq!(uniform.camera_pos, Vec2::new(10.0, 20.0));
        assert_eq!(uniform.inv_scale, 0.5);
        assert_eq!(uniform.overlay_flags, overlay.get_flags(tool.as_ref()));
        assert_eq!(uniform.light_pos, Vec2::new(100.0, 100.0));
        assert_eq!(uniform.light_height, 200.0);
        assert_eq!(uniform.light_color, 0x00FFFFFF);
        assert_eq!(uniform.cursor_pos, Vec2::new(5.0, 6.0));
        assert_eq!((uniform.zone_count, uniform.point_count), (3, 12));
        assert_eq!(uniform.selection_box_start, Vec2::new(0.0, 0.0));
        assert_eq!(uniform.selection_box_end, Vec2::new(0.0, 0.0));
        assert_eq!(uniform.pixels_per_point, 1.0);
        assert_eq!(uniform.overlay_flags & (1 << 8), 0);
        assert_eq!(uniform.active_zone, NO_OBJECT);
        assert_eq!(uniform.overlay_flags & (1 << 12), 0);
    }

    /// The transform box is passed along and drawn, but only if the overlay is on
    #[test]
    fn transform_box() {
        let camera = Camera::default();
        let tool = EditorToolZone::init();
        let objects = ObjectBuffers::headless();
        let transform_box = TransformBox { center: Vec2::new(5.0, 6.0), half_size: Vec2::new(2.0, 3.0), angle: std::f32::consts::FRAC_PI_2 };

        let mut overlay = OverlayState::default();
        let uniform = ViewportDataUniform::new(&Light::default(), &camera, &overlay, Vec2::ZERO, &objects, tool.as_ref(), Some(transform_box));
        assert_ne!(uniform.overlay_flags & (1 << 8), 0);
        assert_eq!(uniform.transform_center, Vec2::new(5.0, 6.0));
        assert_eq!(uniform.transform_half_size, Vec2::new(2.0, 3.0));
        assert!(uniform.transform_rotation.distance(Vec2::new(0.0, 1.0)) < 1e-6);

        overlay.toggle_overlay();
        let uniform = ViewportDataUniform::new(&Light::default(), &camera, &overlay, Vec2::ZERO, &objects, tool.as_ref(), Some(transform_box));
        assert_eq!(uniform.overlay_flags & (1 << 8), 0);
    }

    /// A whole selected group's radius knobs are passed along in the shape tool
    #[test]
    fn radius_knobs() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 100.0);
        let vertex = objects.add_shape_vertex(0, Vec2::new(20.0, 20.0)).unwrap();
        objects.create_shape(0, &[vertex], crate::objects::ShapeParams::default(), 0).unwrap();
        let mut tool = crate::interaction::tools::EditorToolShape::select(&[0], crate::interaction::tools::test_utils::state(&objects));
        tool.handle_click(crate::interaction::tools::test_utils::state(&objects), Vec2::new(20.0, 20.0), Default::default()).unwrap();

        let uniform = ViewportDataUniform::new(&Light::default(), &Camera::default(), &OverlayState::default(), Vec2::ZERO, &objects, tool.as_ref(), None);
        assert_ne!(uniform.overlay_flags & (1 << 12), 0);
        assert_eq!(uniform.outer_knob, Vec2::new(30.0, 20.0));
        assert_eq!(uniform.inner_knob, Vec2::new(20.0, 30.0));

        // Not in other tools, which don't draw shape knobs
        let zone_tool = EditorToolZone::init();
        let uniform = ViewportDataUniform::new(&Light::default(), &Camera::default(), &OverlayState::default(), Vec2::ZERO, &objects, zone_tool.as_ref(), None);
        assert_eq!(uniform.overlay_flags & (1 << 12), 0);
    }

    /// Bytes should cover the whole struct
    #[test]
    fn bytes() {
        let uniform = ViewportDataUniform { zone_count: 1, ..Default::default() };
        assert_eq!(uniform.bytes().len(), std::mem::size_of::<ViewportDataUniform>());

        // Uniform structs in WGSL are padded to 16 bytes
        assert_eq!(std::mem::size_of::<ViewportDataUniform>() % 16, 0);
    }
}
