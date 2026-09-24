use wgpu::{Device, util::DeviceExt};


// Needed for Rust to store data correctly for shaders
#[repr(C)]
// Needed for storing into a buffer
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CameraUniform {
    // Width and height of the actual screen texture
    screen: [u32; 2],
    // Buffer width in pixels for scanline wrapping
    buffer_width: u32,

    _pad: u32, // pad to power of 2 bytes
}

impl CameraUniform {
    pub fn new((width, height): (usize, usize), buffer_width: usize) -> Self {
        Self {
            screen: [width as u32, height as u32],
            buffer_width: buffer_width as u32,
            _pad: 0,
        }
    }

    pub fn buffer(size: usize, device: &Device) -> wgpu::Buffer {
        // Create initial contents
        let camera = Self::new((size, size), size);

        // Create and return the buffer
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Camera Buffer"),
            contents: bytemuck::cast_slice(&[camera]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }
}