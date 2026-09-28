use wgpu::{Device, util::DeviceExt};

/// Formats data used for the actual rendering process.
#[repr(C)] // Needed for Rust to pass to shaders correctly
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)] // Needed to store into a buffer below
pub struct ViewportDataUniform {
    // The ambient light color (nothing should be in total darkness)
    ambient_light: u32,

    // pad because light_pos needs to be aligned
    _pad0: [u32; 3],

    // Position of the point light, assuming each pixel is one unit
    light_pos: [f32; 3],
    // Light color
    light_color: u32,
}

impl ViewportDataUniform {
    /// Create viewport data from some parameters
    pub fn new() -> Self {
        // Pack 20% ambient light into the integer we expect
        let ambient_value = (255 as f32 * 0.25) as u8;
        let ambient_light = pack_color(ambient_value, ambient_value, ambient_value, 255);

        // Do the same for a white light
        let light_value = (255 as f32 * 0.8) as u8;
        let light_color = pack_color(light_value, light_value, light_value, 255);

        Self {
            ambient_light,

            _pad0: [0, 0, 0],

            light_pos: [ 200.0, 100.0, 200.0 ], // static position for now
            light_color,
        }
    }

    /// Create a buffer that can be used to store this uniform data.
    pub fn buffer(device: &Device) -> wgpu::Buffer {
        // Create initial contents
        let data = Self::new();

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