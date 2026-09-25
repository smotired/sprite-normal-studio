use wgpu::{BindGroup, Buffer, ComputePipeline, Device, Texture, TextureView};
use egui::{TextureId};

use crate::screen::view::CameraUniform;

mod view;

pub struct ScreenRenderer {
    pipeline: ComputePipeline,
    camera_buffer: Buffer,
    size: (usize, usize),
    texture: Texture,
    view: TextureView,
    texture_id: TextureId,
    bind_group: BindGroup,
}

impl ScreenRenderer {
    /// Create and initialize a device with the buffers and bind groups
    pub fn new(rs: &egui_wgpu::RenderState) -> Self {
        // Load the shader
        let shader = rs.device.create_shader_module(wgpu::include_wgsl!("screen/helloworld.wgsl"));

        // Create the pipeline for the shader
        let pipeline = rs.device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Introduction Compute Pipeline"),
            layout: None,
            module: &shader,
            entry_point: None,
            compilation_options: Default::default(),
            cache: Default::default(),
        });

        let size: (usize, usize) = (256, 256);

        // Create the buffer for the camera uniform
        let camera_buffer = CameraUniform::buffer(&rs.device);
        
        // Create texture data
        let (texture, view, bind_group) = Self::create_texture(&rs.device, &pipeline, &camera_buffer, size);

        // Get the initial ID
        let texture_id = rs.renderer.write().register_native_texture(&rs.device, &view, wgpu::FilterMode::Nearest);

        Self {
            pipeline,
            camera_buffer,
            size,
            texture,
            view,
            texture_id,
            bind_group,
        }
    }
    
    fn create_texture(device: &Device, pipeline: &ComputePipeline, camera_buffer: &Buffer, (width, height): (usize, usize)) -> (Texture, TextureView, BindGroup) {
        // Create a new texture and texture view when resizing
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("output"),
            size: wgpu::Extent3d { width: width as u32, height: height as u32, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
        let view = texture.create_view(&Default::default());

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: camera_buffer.as_entire_binding(),
                },
            ]
        });

        (texture, view, bind_group)
    }

    pub fn render(&mut self, rs: &egui_wgpu::RenderState, (width, height): (usize, usize)) -> TextureId {
        // Ensure we aren't rendering too small
        let width = width.max(16);
        let height = height.max(16);

        // Recreate texture if needed
        if (width, height) != self.size {
            let (texture, view, bind_group) = Self::create_texture(&rs.device, &self.pipeline, &self.camera_buffer, (width, height));
            rs.renderer.write().update_egui_texture_from_wgpu_texture(
                &rs.device, &view, wgpu::FilterMode::Nearest, self.texture_id,
            );
            (self.texture, self.view, self.bind_group, self.size) = (texture, view, bind_group, (width, height));
        }

        // Create a command encoder
        let mut encoder = rs.device.create_command_encoder(&Default::default());

        // Set up work groups
        {
            // We specified 16x16x16 work groups in the shader
            let blocks_x = width.div_ceil(16) as u32;
            let blocks_y = height.div_ceil(16) as u32;

            // Set up the render pass and dispatch work groups
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.dispatch_workgroups(blocks_x, blocks_y, 1);
        }

        // Write uniforms
        let camera = CameraUniform::new(63);
        rs.queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera));

        // Submit workload
        rs.queue.submit([encoder.finish()]);

        // Return the texture ID
        self.texture_id
    }
}