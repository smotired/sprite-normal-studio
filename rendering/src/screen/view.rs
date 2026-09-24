use wgpu::{Device, util::DeviceExt};


// Needed for Rust to store data correctly for shaders
#[repr(C)]
// Needed for storing into a buffer
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    blue: u32, // we just need something to pass to the buffer
}

impl CameraUniform {
    pub fn new(blue: u32) -> Self {
        Self {
            blue,
        }
    }

    pub fn buffer(device: &Device) -> wgpu::Buffer {
        // Create initial contents
        let camera = Self::new(0);

        // Create and return the buffer
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }
}