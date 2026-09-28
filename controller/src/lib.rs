use wgpu::{BindGroup, ComputePipeline, Device, Queue, Texture};

/// The Controller manages our actual working pipeline.
pub struct Controller {
    /// The Compute pipeline that generates our working normal map.
    pipeline: ComputePipeline,

    /// The output texture for our normal map.
    output: Texture,

    /// Binds our normal map shader to the WGPU device
    bind_group: Option<BindGroup>,

    /// Current size of the spritesheet.
    size: (u32, u32),
}

impl Controller {
    /// Create a new instance of the controller and normal map pipeline
    pub fn new(device: &Device) -> Self {
        // Load the shader and create the pipeline
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/create_map.wgsl"));
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Normal Map Generation Pipeline"),
            layout: None,
            module: &shader,
            entry_point: None,
            compilation_options: Default::default(),
            cache: Default::default(),
        });

        // Create the output texture
        let size = (16, 16);
        let output = create_output(device, size);
        Self { pipeline, output, bind_group: None, size }
    }

    /// Should be called whenever sprite or normal map file selection changes
    /// Recreate bind group for the pipeline
    pub fn set_inputs(&mut self, device: &Device, sprite: &Texture, normal: &Texture) {
        // Recreate output texture if size mismatches
        let size = (sprite.width(), sprite.height());
        if size != self.size {
            self.output = create_output(device, size);
            self.size = size;
        }

        // Create texture views
        let sprite_view = sprite.create_view(&Default::default());
        let normal_view = normal.create_view(&Default::default());
        let output_view = self.output.create_view(&Default::default());

        // Recreate the texture bind group
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Controller texture bind group"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&sprite_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&normal_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&output_view) },
            ],
        }));
    }

    /// Recompute working normal map
    pub fn generate_normals(&self, device: &Device, queue: &Queue) {
        // Skip if we haven't set up the input textures yet
        let Some(bind_group) = &self.bind_group else { return };

        // Create encoder
        let mut encoder = device.create_command_encoder(&Default::default());
        
        {
            // Set up the compute shader pass with the texture bind group
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.dispatch_workgroups(self.size.0.div_ceil(16), self.size.1.div_ceil(16), 1);
        }

        // Submit the command
        queue.submit([encoder.finish()]);
    }

    /// Return a clone of the working normal map.
    pub fn output(&self) -> &Texture { &self.output }
}

/// Create a new texture for the output when needed
fn create_output(device: &Device, (width, height): (u32, u32)) -> Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Working normal map"),
        size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        // Storage for the working pass to write it, texture binding for viewport renderer to read it.
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}