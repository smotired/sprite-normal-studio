use studio_math::Vec2;
use wgpu::{Device, util::DeviceExt};

use crate::{interaction::{camera::Camera, light::Light, overlay::OverlayState, tools::EditorTool}};

/// Formats data used for the actual rendering process.
#[repr(C)] // Needed for Rust to pass to shaders correctly
#[derive(Copy, Clone, Default, bytemuck::Pod, bytemuck::Zeroable)] // Needed to store into a buffer below
pub struct ViewportDataUniform {
    // Pixel the camera is centered on, from top left, ignoring scale
    pub camera_pos: Vec2,

    // Inverted scale of the camera. 1/2 means each sprite pixel takes up 2 screen pixels each direction.
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
}

impl ViewportDataUniform {
    pub fn new(
        light: &Light,
        camera: &Camera,
        overlay: &OverlayState,
        cursor: Vec2,
        (zone_count, point_count): (usize, usize),
        tool: &dyn EditorTool,
    ) -> Self {
        // Calculate selection box
        let (selection_box_start, selection_box_end) = {
            if let Some((start, end)) = tool.selection_box() {
                // Correct box positions
                let start_x = start.x.min(end.x);
                let start_y = start.y.min(end.y);
                let end_x = start.x.max(end.x);
                let end_y = start.y.max(end.y);
                (Vec2::new(start_x, start_y), Vec2::new(end_x, end_y))
            } else { (Vec2::ZERO, Vec2::ZERO) }
        };

        Self {
            camera_pos: camera.position,
            inv_scale: camera.inv_scale(),
            overlay_flags: overlay.get_flags(tool),
            light_pos: light.position(),
            light_height: light.height(),
            light_color: light.packed_color(),
            cursor_pos: cursor,
            zone_count: zone_count as u32,
            point_count: point_count as u32,
            selection_box_start,
            selection_box_end,
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

    use super::*;

    /// The uniform should be filled from each source
    #[test]
    fn new() {
        let mut camera = Camera::default();
        camera.reset_position(Vec2::new(10.0, 20.0));
        camera.apply_scale(1, None);
        let tool = EditorToolZone::init();
        let overlay = OverlayState::default();

        let uniform = ViewportDataUniform::new(&Light::default(), &camera, &overlay, Vec2::new(5.0, 6.0), (3, 12), tool.as_ref());
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
    }

    /// Bytes should cover the whole struct
    #[test]
    fn bytes() {
        let uniform = ViewportDataUniform { zone_count: 1, ..Default::default() };
        assert_eq!(uniform.bytes().len(), std::mem::size_of::<ViewportDataUniform>());
        assert_eq!(std::mem::size_of::<ViewportDataUniform>() % 16, 0); // uniform buffers want 16 byte alignment
    }
}
