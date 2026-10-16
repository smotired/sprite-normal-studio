mod uniform;

use studio_math::Vec2;
use wgpu::{BindGroup, Buffer, ComputePipeline, Device, Queue, Texture};

use crate::{Bounds, BufferStates, Controller};
pub use uniform::GeneratorUniform;

/// The Generator is in charge of generating the normal map from the
/// input texture, spritesheet (for size), zones, and shapes.
pub struct Generator {
    /// The Compute pipeline that generates our working normal map.
    pipeline: ComputePipeline,

    /// The output texture for our normal map.
    output: Texture,

    /// Uniform buffer for this pipeline
    uniform_buffer: Buffer,

    /// Binds our normal map shader to the WGPU device
    bind_group: Option<BindGroup>,

    /// Binds our compute shader to the WGPU device.
    /// Must be recreated whenever object buffers are recreated/resized.
    objects_bind_group: BindGroup,

    /// Current size of the spritesheet.
    size: Vec2,

    /// Last sizes of the zones, shapes and shape vertices buffers, used to know if we need to rebind the buffers.
    object_buffer_sizes: [usize; 3],
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
        let object_buffer_sizes = used_sizes(&object_buffers);
        let objects_bind_group = create_objects_bind_group(device, &pipeline, object_buffers);

        // Create the output texture
        let size = Vec2::new(16.0, 16.0);
        let output = create_output(device, size);

        // Create the uniform buffer
        let uniform_buffer = GeneratorUniform::buffer(device);

        Self { pipeline, output, uniform_buffer, bind_group: None, objects_bind_group, size, object_buffer_sizes }
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
                wgpu::BindGroupEntry { binding: 3, resource: self.uniform_buffer.as_entire_binding() },
            ],
        }));

        // Recreate objects bind group if needed
        let object_buffer_sizes = used_sizes(&object_buffers);
        if object_buffer_sizes != self.object_buffer_sizes {
            self.objects_bind_group = create_objects_bind_group(device, &self.pipeline, object_buffers);
            self.object_buffer_sizes = object_buffer_sizes;
        }
    }

    /// Recompute working normal map within the stale region, which should be corrected and clamped to the texture.
    /// Should be run whenever the input textures, shapes, or zones change.
    /// Does not need to run every frame or when camera or light is changed.
    pub fn generate_normals(&mut self, device: &Device, queue: &Queue, object_buffers: BufferStates, stale: Bounds) {
        // Skip if we haven't set up the input textures yet
        let Some(bind_group) = &self.bind_group else { return };

        // Recreate objects bind group if needed
        let object_buffer_sizes = used_sizes(&object_buffers);
        if object_buffer_sizes != self.object_buffer_sizes {
            self.objects_bind_group = create_objects_bind_group(device, &self.pipeline, object_buffers);
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

        // Write uniforms
        let uniform = GeneratorUniform::new(stale);
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
        label: Some("Working normal map"),
        size: wgpu::Extent3d { width: size.x as u32, height: size.y as u32, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        // Storage for the working pass to write it, texture binding for viewport renderer to read it, copy source for exporting it.
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}

/// Bytes per row of an RGBA8 texture when copied to a buffer, which must be padded to a multiple of COPY_BYTES_PER_ROW_ALIGNMENT
fn padded_bytes_per_row(width: u32) -> u32 {
    (width * 4).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT)
}

/// Get the sizes of the object buffers the generator uses
fn used_sizes(object_buffers: &BufferStates) -> [usize; 3] {
    [object_buffers.zones.1, object_buffers.shapes.1, object_buffers.shape_vertices.1]
}

/// Create an objects bind group
fn create_objects_bind_group(device: &Device, pipeline: &ComputePipeline, object_buffers: BufferStates) -> BindGroup {
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("Normals generator objects bind group"),
        layout: &pipeline.get_bind_group_layout(1),
        entries: &[
            wgpu::BindGroupEntry { binding: 0, resource: object_buffers.zones.0.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 1, resource: object_buffers.shapes.0.as_entire_binding() },
            wgpu::BindGroupEntry { binding: 2, resource: object_buffers.shape_vertices.0.as_entire_binding() },
        ],
    })
}

impl Controller {
    /// Get a reference to the working normal map texture
    pub fn output(&self) -> &Texture { self.generator.output() }

    /// Read the generated normal map back from the GPU, after regenerating anything stale.
    /// Returns its width, height, and RGBA8 pixels in rows with no padding. Blocks until the GPU is done.
    pub fn read_output(&mut self, device: &Device, queue: &Queue) -> anyhow::Result<(u32, u32, Vec<u8>)> {
        self.update(device, queue);

        let output = self.generator.output();
        let (width, height) = (output.width(), output.height());
        let padded_row = padded_bytes_per_row(width);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Normal map readback buffer"),
            size: (padded_row * height) as u64,
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // Copy the texture into the buffer
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            output.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout { offset: 0, bytes_per_row: Some(padded_row), rows_per_image: Some(height) },
            },
            output.size(),
        );
        queue.submit([encoder.finish()]);

        // Wait for the copy, then take each row without its padding
        let slice = buffer.slice(..);
        let (sender, receiver) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| { let _ = sender.send(result); });
        device.poll(wgpu::PollType::wait_indefinitely())?;
        receiver.recv()??;

        let mut pixels = Vec::with_capacity((width * height * 4) as usize);
        {
            let mapped = slice.get_mapped_range()?;
            for row in mapped.chunks(padded_row as usize) {
                pixels.extend_from_slice(&row[..(width * 4) as usize]);
            }
        }
        buffer.unmap();

        Ok((width, height, pixels))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Rows are padded up to the copy alignment, unless they already fill it
    #[test]
    fn padded_rows() {
        assert_eq!(padded_bytes_per_row(1), 256);
        assert_eq!(padded_bytes_per_row(64), 256);
        assert_eq!(padded_bytes_per_row(65), 512);
    }

    /// The shader parses and validates
    #[test]
    fn shader_validates() {
        crate::validate_wgsl(include_str!("shaders/generator.wgsl"));
    }
}