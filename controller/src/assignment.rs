mod uniform;

use studio_math::Vec2;
use wgpu::{BindGroup, Buffer, ComputePipeline, Device, Queue, Texture};

use crate::{BufferStates, Controller};
pub use uniform::ZoneAssignerUniform;

/// The ZoneAssigner is in charge of assigning pixels to the zones they fall into.
pub struct ZoneAssigner {
    /// The compute pipeline that assigns pixels to zones.
    pipeline: ComputePipeline,

    /// The output texture for zone assignments.
    output: Texture,

    /// Uniform buffer for this pipeline
    uniform_buffer: Buffer,

    /// Binds the output texture and uniforms to the pipeline.
    /// Only needs to be recreated when a new spritesheet is imported (because size changes).
    output_bind_group: Option<BindGroup>,

    /// Binds our compute shader to the WGPU device.
    /// Must be recreated whenever object buffers are recreated/resized.
    objects_bind_group: BindGroup,

    /// Current size of the spritesheet.
    size: Vec2,

    /// Last size of the object buffer, used to know if we need to rebind the buffers.
    object_buffer_sizes: (usize, usize),
}

impl ZoneAssigner {
    /// Set up the generator and shader pipeline
    pub fn new(device: &Device, object_buffers: BufferStates) -> Self {
        // Load the shader and create the pipeline
        let shader = device.create_shader_module(wgpu::include_wgsl!("shaders/assignment.wgsl"));
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Zone Assignment Pipeline"),
            layout: None,
            module: &shader,
            entry_point: None,
            compilation_options: Default::default(),
            cache: Default::default(),
        });

        // Create object bind group
        let (zones_buffer, points_buffer, _) = object_buffers;
        let objects_bind_group = create_objects_bind_group(device, &pipeline, zones_buffer.0, points_buffer.0);
        let object_buffer_sizes = (zones_buffer.1, points_buffer.1);

        // Create the output texture
        let size = Vec2::new(16.0, 16.0);
        let output = create_output(device, size);

        // Create the uniform buffer
        let uniform_buffer = ZoneAssignerUniform::buffer(device);

        Self { pipeline, output, uniform_buffer, output_bind_group: None, objects_bind_group, size, object_buffer_sizes }
    }

    /// Should be called whenever sprite file selection changes
    /// Recreate output texture and bind group
    pub fn set_inputs(&mut self, device: &Device, sprite: &Texture, object_buffers: BufferStates) {
        // Recreate output texture if size mismatches
        let size = Vec2::from(sprite.size());
        if size != self.size {
            self.output = create_output(device, size);
            self.size = size;
        }

        // Create texture view
        let output_view = self.output.create_view(&Default::default());

        // Recreate the texture bind group
        self.output_bind_group = Some(device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Assignment bind group"),
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&output_view) },
                wgpu::BindGroupEntry { binding: 1, resource: self.uniform_buffer.as_entire_binding() },
            ],
        }));

        // Recreate objects bind group if needed
        let (zones_buffer, points_buffer, _) = object_buffers;
        let object_buffer_sizes = (zones_buffer.1, points_buffer.1);
        if object_buffer_sizes != self.object_buffer_sizes {
            self.objects_bind_group = create_objects_bind_group(device, &self.pipeline, zones_buffer.0, points_buffer.0);
            self.object_buffer_sizes = object_buffer_sizes;
        }
    }

    /// Reassign textures to zones.
    /// Should be run whenever the zones change.
    /// TODO: Give it a bounding box, and only update assignments within that bounding box.
    ///       The bounding box should be the bounding box of the zone's old path and its new path
    pub fn assign_zones(&mut self, device: &Device, queue: &Queue, object_buffers: BufferStates, object_counts: (usize, usize)) {
        // Skip if we haven't set up the texture bind group yet
        let Some(output_bind_group) = &self.output_bind_group else { return };

        // Recreate objects bind group if needed
        let (zones_buffer, points_buffer, _) = object_buffers;
        let object_buffer_sizes = (zones_buffer.1, points_buffer.1);
        if object_buffer_sizes != self.object_buffer_sizes {
            self.objects_bind_group = create_objects_bind_group(device, &self.pipeline, zones_buffer.0, points_buffer.0);
            self.object_buffer_sizes = object_buffer_sizes;
        }

        // Create encoder
        let mut encoder = device.create_command_encoder(&Default::default());
        {
            // Set up the compute shader pass with the texture bind group
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, output_bind_group, &[]);
            pass.set_bind_group(1, &self.objects_bind_group, &[]);
            pass.dispatch_workgroups((self.size.x as u32).div_ceil(16), (self.size.y as u32).div_ceil(16), 1);
        }

        // Write uniforms
        let uniform = ZoneAssignerUniform::new(object_counts);
        queue.write_buffer(&self.uniform_buffer, 0, uniform.bytes());

        // Submit the command
        queue.submit([encoder.finish()]);
    }

    /// Return a clone of the working normal map.
    pub fn output(&self) -> &Texture { &self.output }
}

/// Create a new texture for the output when needed
fn create_output(device: &Device, size: Vec2) -> Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Zone assignment map"),
        size: wgpu::Extent3d { width: size.x as u32, height: size.y as u32, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Uint,
        // Storage for the working pass to write it, texture binding for viewport renderer to read it.
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING,
        view_formats: &[],
    })
}

/// Create an objects bind group
fn create_objects_bind_group(device: &Device, pipeline: &ComputePipeline, zones_buffer: Buffer, points_buffer: Buffer) -> BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Assignment objects bind group"),
        layout: &pipeline.get_bind_group_layout(1),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: zones_buffer.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: points_buffer.as_entire_binding() },
        ],
    })
}

impl Controller {
    /// Get a reference to the assignment texture
    pub fn assignment(&self) -> &Texture { self.assigner.output() }
}