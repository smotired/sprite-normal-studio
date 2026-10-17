use studio_math::Vec2;
use wgpu::{BindGroup, Buffer, ComputePipeline, Device, Queue, Texture, TextureView};

use crate::{create_texture, create_textures_bind_group};

/// Data for drawing one sprite in the animation preview. See shader/preview.wgsl for what each field does.
#[repr(C)]
#[derive(Copy, Clone, Debug, Default, PartialEq, bytemuck::Pod, bytemuck::Zeroable)]
pub struct PreviewUniform {
    /// Top left pixel of the sprite in the spritesheet
    pub sprite_min: Vec2,

    /// Size of the sprite in pixels
    pub sprite_size: Vec2,

    /// Position of the light relative to the top left of the sprite
    pub light_offset: Vec2,

    /// Sprite pixels per screen point
    pub inv_scale: f32,

    /// Physical pixels per screen point. Set by the renderer.
    pub pixels_per_point: f32,

    /// Height of the light, in sprite pixels
    pub light_height: f32,

    /// Light color, packed as 0x00BBGGRR
    pub light_color: u32,

    /// 00: normal map, 01: lighting, 02: dragging the light
    pub flags: u32,

    /// Pads the struct to a multiple of 16 bytes, like the shader expects
    pub _padding: u32,
}

/// The scale the preview starts at, which fits the whole sprite in the view, zoomed in or out by powers of 2.
/// Returns sprite pixels per screen point.
pub fn preview_inv_scale(sprite_size: Vec2, view_size: Vec2, zoom: i32) -> f32 {
    let fit = (sprite_size.x / view_size.x.max(1.0)).max(sprite_size.y / view_size.y.max(1.0));
    fit * 2_f32.powi(-zoom)
}

/// Renders the animation preview, one lit sprite at a time
pub struct PreviewRenderer {
    pipeline: ComputePipeline,
    uniform_buffer: Buffer,
    bind_group: BindGroup,
    sprite_view: TextureView,
    normal_view: TextureView,
    output: TextureView,
    size: Vec2,
}

impl PreviewRenderer {
    pub fn new(device: &Device) -> Self {
        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("preview.wgsl"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader/preview.wgsl").into()),
        });
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Preview Rendering Pipeline"),
            layout: None,
            module: &shader,
            entry_point: None,
            compilation_options: Default::default(),
            cache: Default::default(),
        });
        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Preview Uniform Buffer"),
            size: std::mem::size_of::<PreviewUniform>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let size = Vec2::square(64.0);
        let output = create_texture(device, size);
        let sprite_view = create_texture(device, Vec2::square(16.0));
        let normal_view = create_texture(device, Vec2::square(16.0));
        let bind_group = create_textures_bind_group(device, &pipeline, &output, &sprite_view, &normal_view, &uniform_buffer);
        Self { pipeline, uniform_buffer, bind_group, sprite_view, normal_view, output, size }
    }

    /// The texture the preview is drawn to
    pub fn view(&self) -> &TextureView { &self.output }

    /// Set the spritesheet and the working normal map
    pub fn set_inputs(&mut self, device: &Device, sprite: &Texture, normal: &Texture) {
        self.sprite_view = sprite.create_view(&Default::default());
        self.normal_view = normal.create_view(&Default::default());
        self.rebind(device);
    }

    /// Draw a sprite at a size in physical pixels, and return the texture it was drawn to
    pub fn render(&mut self, device: &Device, queue: &Queue, uniform: &PreviewUniform, size: Vec2, pixels_per_point: f32) -> &TextureView {
        let size = Vec2::new(size.x.round().max(16.0), size.y.round().max(16.0));
        if size != self.size {
            self.size = size;
            self.output = create_texture(device, size);
            self.rebind(device);
        }

        let uniform = PreviewUniform { pixels_per_point, ..*uniform };
        queue.write_buffer(&self.uniform_buffer, 0, bytemuck::bytes_of(&uniform));

        let mut encoder = device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups((size.x as u32).div_ceil(16), (size.y as u32).div_ceil(16), 1);
        }
        queue.submit([encoder.finish()]);
        &self.output
    }

    /// Recreate the bind group after the output or input textures change
    fn rebind(&mut self, device: &Device) {
        self.bind_group = create_textures_bind_group(device, &self.pipeline, &self.output, &self.sprite_view, &self.normal_view, &self.uniform_buffer);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The preview shader parses, validates, and fits in the device's limits
    #[test]
    fn shader_validates() {
        crate::tests::validate(include_str!("shader/preview.wgsl"));
    }

    /// Uniform structs in WGSL are padded to 16 bytes
    #[test]
    fn uniform_size() {
        assert_eq!(std::mem::size_of::<PreviewUniform>() % 16, 0);
    }

    /// The whole sprite fits along its longer side, then zooming doubles or halves it
    #[test]
    fn fit_and_zoom() {
        let view = Vec2::new(100.0, 100.0);
        assert_eq!(preview_inv_scale(Vec2::new(50.0, 25.0), view, 0), 0.5);
        assert_eq!(preview_inv_scale(Vec2::new(50.0, 200.0), view, 0), 2.0);
        assert_eq!(preview_inv_scale(Vec2::new(50.0, 25.0), view, 1), 0.25);
        assert_eq!(preview_inv_scale(Vec2::new(50.0, 25.0), view, -2), 2.0);
    }
}
