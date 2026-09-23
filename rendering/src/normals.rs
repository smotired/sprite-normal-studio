use std::sync::mpsc::channel;
use wgpu::{BindGroup, Buffer, ComputePipeline, Device, Queue};
use egui::ColorImage;

pub struct NormalsRenderer {
    pipeline: ComputePipeline,
    output_buffer: Buffer,
    bind_group: BindGroup,
    temp_buffer: Buffer,
}

impl NormalsRenderer {
    pub fn new(device: &Device) -> Self {
        // Load the shader
        let shader = device.create_shader_module(wgpu::include_wgsl!("normals/helloworld.wgsl"));

        // Create the pipeline for the shader
        let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
            label: Some("Introduction Compute Pipeline"),
            layout: None,
            module: &shader,
            entry_point: None,
            compilation_options: Default::default(),
            cache: Default::default(),
        });

        // Create the output buffer
        let output_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("output"),
            size: (256 * 256) * 4, // Max image size = 256x256, and sized for u32 colors.
            usage: wgpu::BufferUsages::COPY_SRC | wgpu::BufferUsages::STORAGE, // Allow us to copy data back to a texture
            mapped_at_creation: false,
        });

        // Create the bind group for the output texture
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: output_buffer.as_entire_binding(),
                }
            ]
        });

        // Create a temp buffer for checking completion
        let temp_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("temp"),
            size: output_buffer.size(),
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST, // Allow us to read and map from the buffer
            mapped_at_creation: false,
        });

        Self { pipeline, output_buffer, bind_group, temp_buffer }
    }

    pub fn render(self: &Self, (width, height): (usize, usize), device: &Device, queue: &Queue) -> anyhow::Result<ColorImage> {
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
            let uncropped = ColorImage::from_rgba_unmultiplied([256, 256], &output_data);
            uncropped.region(
                &egui::Rect::from_min_max(
                    egui::Pos2::ZERO,
                    egui::Pos2::new(width as f32 - 1.0f32, height as f32 - 1.0f32)
                ),
                None
            )
        };
        
        // Unmap the buffer so we can use it again
        self.temp_buffer.unmap();

        Ok(image)
    }
}