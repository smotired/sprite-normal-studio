use studio_math::Vec2;
use wgpu::{Device, util::DeviceExt};

use crate::Bounds;

/// Formats data used for generating the normal map.
#[repr(C)] // Needed for Rust to pass to shaders correctly
#[derive(Default, Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)] // Needed to store into a buffer below
pub struct GeneratorUniform {
    // Top left corner of the region to regenerate
    pub stale_min: Vec2,

    // Bottom right corner of the region to regenerate
    pub stale_max: Vec2,
}

impl GeneratorUniform {
    pub fn new((stale_min, stale_max): Bounds) -> Self {
        Self { stale_min, stale_max }
    }

    /// Create a buffer that can be used to store this uniform data.
    pub fn buffer(device: &Device) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Generator Uniform Buffer"),
            contents: bytemuck::cast_slice(&[Self::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }

    /// Get the bytes for writing to a buffer
    pub fn bytes(&self) -> &[u8] { bytemuck::bytes_of(self) }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STALE: Bounds = (Vec2 { x: 1.0, y: 2.0 }, Vec2 { x: 30.0, y: 40.0 });

    /// The stale region should be copied into the uniform
    #[test]
    fn new() {
        let uniform = GeneratorUniform::new(STALE);
        assert_eq!((uniform.stale_min, uniform.stale_max), STALE);
    }

    /// Bytes should be the top left corner then the bottom right corner
    #[test]
    fn bytes() {
        let uniform = GeneratorUniform::new(STALE);
        let bytes = uniform.bytes();
        assert_eq!(bytes.len(), 16);
        assert_eq!(&bytes[0..8], STALE.0.bytes());
        assert_eq!(&bytes[8..16], STALE.1.bytes());
    }
}
