use std::sync::mpsc::channel;
use wgpu::{BindGroup, Buffer, ComputePipeline, Device, Queue};
use egui::ColorImage;

use crate::screen::view::CameraUniform;

mod view;

pub struct ScreenRenderer {
    pipeline: ComputePipeline,
    output_buffer: Buffer,
    temp_buffer: Buffer,
    camera_buffer: Buffer,
    buffer_size: usize,
    bind_group: BindGroup,
}

impl ScreenRenderer {
    /// Create and initialize a device with the buffers and bind groups
    pub fn new(device: &Device) -> Self {
        // Load the shader
        let shader = device.create_shader_module(wgpu::include_wgsl!("screen/helloworld.wgsl"));

        // Create the pipeline for the shader
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Introduction Compute Pipeline"),
            layout: None,
            module: &shader,
            entry_point: None,
            compilation_options: Default::default(),
            cache: Default::default(),
        });

        let size: usize = 256;

        // Create the output buffer
        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("output"),
            size: ((size * size) * 4) as u64, // Max image size = 256x256, and sized for u32 colors.
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::STORAGE, // Allow us to copy data back to a texture
            mapped_at_creation: false,
        });

        // Create a temp buffer for checking completion
        let temp_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("temp"),
            size: output_buffer.size(),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, // Allow us to read and map from the buffer
            mapped_at_creation: false,
        });

        // Create the buffer for the camera uniform
        let camera_buffer = CameraUniform::buffer(size, &device);

        // Create the bind group for the output texture and uniforms
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: output_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: camera_buffer.as_entire_binding(),
                }
            ]
        });

        Self {
            pipeline,
            output_buffer,
            temp_buffer,
            camera_buffer,
            buffer_size: size,
            bind_group,
        }
    }

    /// Recreate the buffers with the new size, on window resize.
    fn recreate_buffers(self: &mut Self, (width, height): (usize, usize), device: &Device)
    {
        // Determine the order of magnitude
        let mut size = self.buffer_size;
        let max_dim = width.max(height);
        while size < max_dim { size = size << 1 };
        while (size >> 1) >= max_dim { size = size >> 1 };

        // Recreate the buffers
        self.output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("output"),
            size: ((size * size) * 4) as u64,
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::STORAGE, // Allow us to copy data back to a texture
            mapped_at_creation: false,
        });

        self.temp_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("temp"),
            size: self.output_buffer.size(),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, // Allow us to read and map from the buffer
            mapped_at_creation: false,
        });

        // Recreate bind group
        self.bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: self.output_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: self.camera_buffer.as_entire_binding(),
                }
            ]
        });

        self.buffer_size = size;
    }

    pub fn render(self: &mut Self, (width, height): (usize, usize), device: &Device, queue: &Queue) -> anyhow::Result<(ColorImage, usize)> {
        // Ensure we aren't rendering too msmall
        let width = width.max(16);
        let height = height.max(16);

        // Resize buffers if necessary
        if {
            let max_dim = width.max(height);
            max_dim > self.buffer_size || max_dim <= (self.buffer_size >> 1)
        } {
            self.recreate_buffers((width, height), device);
        }

        // Write uniforms
        let camera = CameraUniform::new((width, height), self.buffer_size);
        queue.write_buffer(&self.camera_buffer, 0, bytemuck::bytes_of(&camera));

        // Create a command encoder
        let mut encoder = device.create_command_encoder(&Default::default());

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

        // Copy output buffer to the temp buffer
        encoder.copy_buffer_to_buffer(&self.output_buffer, 0, &self.temp_buffer, 0, self.output_buffer.size());

        // Submit the queue
        queue.submit([encoder.finish()]);

        // Wait for completion, in a block so that data goes out of scope before unmapping.
        let image = {
            // Create channel for async mapping process
            let (tx, rx) = channel();

            // We send the success or failure of our mapping via a callback
            self.temp_buffer.map_async(wgpu::MapMode::Read, .., move |result| tx.send(result).unwrap());

            // The callback we submitted to map async will only get called after the
            // device is polled or the queue submitted
            device.poll(wgpu::PollType::wait_indefinitely())?;

            // We check if the mapping was successful here
            rx.recv()??;

            // We then get the bytes that were stored in the buffer
            let output_data = self.temp_buffer.get_mapped_range(..)?;

            // Convert to an egui ColorImage
            ColorImage::from_rgba_unmultiplied([self.buffer_size, self.buffer_size], &output_data)
        };
        
        // Unmap the buffer so we can use it again
        self.temp_buffer.unmap();

        Ok((image, self.buffer_size))
    }
}