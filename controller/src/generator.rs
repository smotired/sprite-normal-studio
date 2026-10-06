use studio_math::Vec2;
use wgpu::{BindGroup, Buffer, ComputePipeline, Device, Queue, Texture};

use crate::{BufferStates, Controller};

/// The Generator is in charge of generating the normal map from the
/// input texture, spritesheet (for size), zones, and shapes.
pub struct Generator {
    /// The Compute pipeline that generates our working normal map.
    pipeline: ComputePipeline,

    /// The output texture for our normal map.
    output: Texture,

    /// Binds our normal map shader to the WGPU device
    bind_group: Option<BindGroup>,

    /// Binds our compute shader to the WGPU device.
    /// Must be recreated whenever object buffers are recreated/resized.
    objects_bind_group: BindGroup,

    /// Current size of the spritesheet.
    size: Vec2,

    /// Last size of the zones buffer, used to know if we need to rebind the buffers.
    object_buffer_sizes: (usize, ()),
}

impl Generator {
    /// Set up the generator and shader pipeline
    pub fn new(device: &Device, object_buffers: BufferStates) -> Self {
        // Load the shader and create the pipeline
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/generator.wgsl"));
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Normal Map Generation Pipeline"),
            layout: None,
            module: &shader,
            entry_point: None,
            compilation_options: Default::default(),
            cache: Default::default(),
        });

        // Create object bind group
        let (zones_buffer, _, _) = object_buffers;
        let objects_bind_group = create_objects_bind_group(device, &pipeline, zones_buffer.0);
        let object_buffer_sizes = (zones_buffer.1, ());

        // Create the output texture
        let size = Vec2::new(16.0, 16.0);
        let output = create_output(device, size);

        Self { pipeline, output, bind_group: None, objects_bind_group, size, object_buffer_sizes }
    }

    /// Should be called whenever normal map or assignment textures are regenerated
    /// Recreate bind group for the pipeline
    pub fn set_inputs(&mut self, device: &Device, normal: &Texture, assign: &Texture, object_buffers: BufferStates) {
        // Recreate output texture if size mismatches
        let size = Vec2::from(assign.size());
        if size != self.size {
            self.output = create_output(device, size);
            self.size = size;
        }

        // Create texture views
        let normal_view = normal.create_view(&Default::default());
        let assign_view = assign.create_view(&Default::default());
        let output_view = self.output.create_view(&Default::default());

        // Recreate the texture bind group
        self.bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Controller texture bind group"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&normal_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&assign_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::TextureView(&output_view) },
            ],
        }));

        // Recreate objects bind group if needed
        let (zones_buffer, _, _) = object_buffers;
        let object_buffer_sizes = (zones_buffer.1, ());
        if object_buffer_sizes != self.object_buffer_sizes {
            self.objects_bind_group = create_objects_bind_group(device, &self.pipeline, zones_buffer.0);
            self.object_buffer_sizes = object_buffer_sizes;
        }
    }

    /// Recompute working normal map.
    /// Should be run whenever the input textures, shapes, or zones change.
    /// Does not need to run every frame or when camera or light is changed.
    pub fn generate_normals(&mut self, device: &Device, queue: &Queue, object_buffers: BufferStates) {
        // Skip if we haven't set up the input textures yet
        let Some(bind_group) = &self.bind_group else { return };

        // Recreate objects bind group if needed
        let (zones_buffer, _, _) = object_buffers;
        let object_buffer_sizes = (zones_buffer.1, ());
        if object_buffer_sizes != self.object_buffer_sizes {
            self.objects_bind_group = create_objects_bind_group(device, &self.pipeline, zones_buffer.0);
            self.object_buffer_sizes = object_buffer_sizes;
        }

        // Create encoder
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            // Set up the compute shader pass with the texture bind group
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, bind_group, &[]);
            pass.set_bind_group(1, &self.objects_bind_group, &[]);
            pass.dispatch_workgroups((self.size.x as u32).div_ceil(16), (self.size.y as u32).div_ceil(16), 1);
        }

        // Submit the command
        queue.submit([encoder.finish()]);
    }

    /// Return a clone of the working normal map.
    pub fn output(&self) -> &Texture { &self.output }
}

/// Create a new texture for the output when needed
fn create_output(device: &Device, size: Vec2) -> Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Working normal map"),
        size: wgpu::Extent3d { width: size.x as u32, height: size.y as u32, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        // Storage for the working pass to write it, texture binding for viewport renderer to read it.
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

/// Create an objects bind group
fn create_objects_bind_group(device: &Device, pipeline: &ComputePipeline, zones_buffer: Buffer) -> BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Normals generator objects bind group"),
        layout: &pipeline.get_bind_group_layout(1),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: zones_buffer.as_entire_binding() },
        ],
    })
}

impl Controller {
    /// Get a reference to the working normal map texture
    pub fn output(&self) -> &Texture { self.generator.output() }
}