use wgpu::{Device, util::DeviceExt};

use crate::interaction::{camera::Camera, light::Light, overlay::OverlayState};

/// Formats data used for the actual rendering process.
#[repr(C)] // Needed for Rust to pass to shaders correctly
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)] // Needed to store into a buffer below
pub struct ViewportDataUniform {
    // Pixel the camera is centered on, from top left, ignoring scale
    pub camera_pos: [f32; 2],
    // Inverted scale of the camera. 1/2 means each sprite pixel takes up 2 screen pixels each direction.
    // We never need the non inverted version, but this is used frequently to normalize stuff that doesn't depend on the camera scale like the overlay.
    pub inv_scale: f32,

    // Flags for the overlay. See shader/bindings.wgsl for full documentation of each flag.
    pub overlay_flags: u32,

    // Position of the point light, assuming each pixel is one unit
    pub light_pos: [f32; 3],
    // Light color
    pub light_color: u32,
}

impl ViewportDataUniform {
    pub fn new(light: &Light, camera: &Camera, overlay: &OverlayState) -> Self {
        Self {
            camera_pos: [ camera.position.x, camera.position.y ],
            inv_scale: camera.inv_scale(),
            overlay_flags: overlay.get_flags(),
            light_pos: light.pos_arr(),
            light_color: light.packed_color(),
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

impl Default for ViewportDataUniform {
    fn default() -> Self {
        Self {
            camera_pos: Default::default(),
            inv_scale: Default::default(),
            overlay_flags: Default::default(),
            light_pos: Default::default(),
            light_color: Default::default(),
        }
    }
}