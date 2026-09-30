use controller::ViewportState;
use wgpu::{Device, util::DeviceExt};

/// Formats data used for the actual rendering process.
#[repr(C)] // Needed for Rust to pass to shaders correctly
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)] // Needed to store into a buffer below
pub struct ViewportDataUniform {
    // Pixel the camera is centered on, from top left, ignoring scale
    camera_pos: [f32; 2],
    // Inverted scale of the camera. 1/2 means each sprite pixel takes up 2 screen pixels each direction.
    // We never need the non inverted version, but this is used frequently to normalize stuff that doesn't depend on the camera scale like the overlay.
    inv_scale: f32,

    // Flags for the overlay. See shader/bindings.wgsl for full documentation of each flag.
    overlay_flags: u32,

    // Position of the point light, assuming each pixel is one unit
    light_pos: [f32; 3],
    // Light color
    light_color: u32,
}

impl ViewportDataUniform {
    /// Create viewport data from some parameters
    pub fn new((camera, light, overlay_flags): ViewportState) -> Self {
        let (light_pos, light_color) = light.info_for_shader();

        Self {
            camera_pos: camera.position,
            inv_scale: 1.0 / camera.scale as f32,

            overlay_flags,
            light_pos,
            light_color
        }
    }

    /// Create a buffer that can be used to store this uniform data.
    pub fn buffer(device: &Device) -> wgpu::Buffer {
        // Create initial contents
        let data = Self::new(Default::default());

        // Create and return the buffer
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[data]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }
}