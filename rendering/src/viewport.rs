use controller::ViewportState;
use wgpu::{Device, util::DeviceExt};

/// Formats data used for the actual rendering process.
#[repr(C)] // Needed for Rust to pass to shaders correctly
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)] // Needed to store into a buffer below
pub struct ViewportDataUniform {
    // Pixel the camera is centered on, from top left, ignoring scale
    camera_pos: [i32; 2],
    // Scale of the camera. 2 means each sprite pixel takes up 2 screen pixels each direction.
    camera_scale: u32,

    // The ambient light color (nothing should be in total darkness)
    ambient_light: u32,

    // Position of the point light, assuming each pixel is one unit
    light_pos: [f32; 3],
    // Light color
    light_color: u32,
}

impl ViewportDataUniform {
    /// Create viewport data from some parameters
    pub fn new((camera, _): ViewportState) -> Self {
        // Pack 20% ambient light into the integer we expect
        let ambient_value = (255 as f32 * 0.25) as u8;
        let ambient_light = pack_color(ambient_value, ambient_value, ambient_value, 255);

        // Do the same for a white light
        let light_value = (255 as f32 * 0.8) as u8;
        let light_color = pack_color(light_value, light_value, light_value, 255);

        Self {
            camera_pos: camera.position,
            camera_scale: camera.scale,

            ambient_light,

            light_pos: [ 200.0, 100.0, 200.0 ], // static position for now
            light_color,
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

fn pack_color(r: u8, g: u8, b: u8, a: u8) -> u32 {
    ((r as u32) <<  0) |
    ((g as u32) <<  8) |
    ((b as u32) << 16) |
    ((a as u32) << 24)
}