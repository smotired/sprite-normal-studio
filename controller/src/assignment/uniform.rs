use wgpu::{Device, util::DeviceExt};

/// Formats data used for the actual rendering process.
#[repr(C)] // Needed for Rust to pass to shaders correctly
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)] // Needed to store into a buffer below
pub struct ZoneAssignerUniform {
    // Amount of zones
    pub zone_count: u32,

    // Amount of control points
    pub point_count: u32,

    // Zone to ignore
    pub zone_ignore: u32,
}

impl ZoneAssignerUniform {
    pub fn new((zone_count, point_count): (usize, usize), zone_ignore: Option<u16>) -> Self {
        Self {
            zone_count: zone_count as u32,
            point_count: point_count as u32,
            zone_ignore: if let Some(id) = zone_ignore { id as u32 } else { 65536 },
        }
    }

    /// Create a buffer that can be used to store this uniform data.
    pub fn buffer(device: &Device) -> wgpu::Buffer {
        device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("Zone Assigner Uniform Buffer"),
            contents: bytemuck::cast_slice(&[Self::default()]),
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        })
    }

    /// Get the bytes for writing to a buffer
    pub fn bytes(&self) -> &[u8] { bytemuck::bytes_of(self) }
}

impl Default for ZoneAssignerUniform {
    fn default() -> Self {
        Self {
            zone_count: 0,
            point_count: 0,
            zone_ignore: 65536,
        }
    }
}