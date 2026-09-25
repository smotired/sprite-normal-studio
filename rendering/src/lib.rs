mod viewport;

use wgpu::{BindGroup, Buffer, ComputePipeline, Device, TextureView};
use egui::{TextureId};

use crate::viewport::ViewportDataUniform;

/// Contains information about the viewport texture.
struct ViewportTexture {
    /// The current size of the viewport, in pixels.
    size: (usize, usize),

    /// The actual viewer for the viewport.
    view: TextureView,

    /// The ID of the viewport texture used for egui.
    id: TextureId,
}

/// References for the objects that have to do with WGPU rendering
struct RenderControl {
    /// The compute pipeline we use to run the shader that generates the viewport image.
    pipeline: ComputePipeline,

    /// The uniform buffer we use to send viewport data to the GPU.
    uniform_buffer: Buffer,

    /// Bind group for sending the textures and uniforms to the GPU.
    bind_group: BindGroup,
}

/// Primary struct for rendering the viewport.
pub struct Renderer {
    /// References to the controller objects.
    control: RenderControl,

    /// Information about the texture used for the viewport.
    texture: ViewportTexture,
}

impl Renderer {
    /// Use egui's render state to initialize our renderer.
    /// Creates our compute pipeline, texture, and buffers.
    pub fn new(rs: &egui_wgpu::RenderState) -> Self {
        // Load the shader
        let shader = rs.device.create_shader_module(wgpu::include_wgsl!("viewport.wgsl"));

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

        // Create the buffer for the viewport uniform
        let uniform_buffer = ViewportDataUniform::buffer(&rs.device);
        
        // Create texture data
        let (view, bind_group) = Self::create_texture(&rs.device, &pipeline, &uniform_buffer, size);

        // Get the initial ID
        let texture_id = rs.renderer.write().register_native_texture(&rs.device, &view, wgpu::FilterMode::Nearest);

        // Set up
        let control = RenderControl { pipeline, uniform_buffer, bind_group };
        let texture = ViewportTexture { size, view, id: texture_id };
        Self { control, texture }
    }
    
    /// Create a texture, view, and bindgroup for a pipeline. Should be called when the target render size changes.
    fn create_texture(device: &Device, pipeline: &ComputePipeline, uniform_buffer: &Buffer, (width, height): (usize, usize)) -> (TextureView, BindGroup) {
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

        // Must recreate the bind group.
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
                    resource: uniform_buffer.as_entire_binding(),
                },
            ]
        });

        (view, bind_group)
    }

    /// Render the editor UI. Runs the shader and updates the texture, and returns the texture ID for use in egui.
    pub fn render(&mut self, rs: &egui_wgpu::RenderState, (width, height): (usize, usize)) -> TextureId {
        // Ensure we aren't rendering too small
        let width = width.max(16);
        let height = height.max(16);

        // Recreate texture if needed
        if (width, height) != self.texture.size {
            let (view, bind_group) = Self::create_texture(
                &rs.device,
                &self.control.pipeline,
                &self.control.uniform_buffer,
                (width, height)
            );

            rs.renderer.write().update_egui_texture_from_wgpu_texture(
                &rs.device, &view, wgpu::FilterMode::Nearest, self.texture.id,
            );

            self.texture.view = view;
            self.control.bind_group = bind_group;
            self.texture.size = (width, height);
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
            pass.set_pipeline(&self.control.pipeline);
            pass.set_bind_group(0, &self.control.bind_group, &[]);
            pass.dispatch_workgroups(blocks_x, blocks_y, 1);
        }

        // Write uniforms
        let camera = ViewportDataUniform::new(63);
        rs.queue.write_buffer(&self.control.uniform_buffer, 0, bytemuck::bytes_of(&camera));

        // Submit workload
        rs.queue.submit([encoder.finish()]);

        // Return the texture ID
        self.texture.id
    }
}