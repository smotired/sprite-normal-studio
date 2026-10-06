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