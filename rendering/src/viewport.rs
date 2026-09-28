use wgpu::{Device, util::DeviceExt};

/// Formats data used for the actual rendering process.
#[repr(C)] // Needed for Rust to pass to shaders correctly
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)] // Needed to store into a buffer below
pub struct ViewportDataUniform {
    // The grayscale background color for the sprite
    background: u32,
}

impl ViewportDataUniform {
    /// Create viewport data from some parameters
    pub fn new() -> Self {
        Self {
            // Use a default gray background
            background: 63,
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