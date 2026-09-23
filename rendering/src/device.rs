use egui::{Context, TextureHandle};
use pollster::FutureExt;
use wgpu::{Device, Instance, Queue};

use crate::normals::NormalsRenderer;

// Contains methods for initializing and using a device
pub struct RendererDevice {
    device: Device,
    queue: Queue,
    normals: NormalsRenderer,
    pub texture: Option<TextureHandle>,
}

impl RendererDevice {
    pub fn new() -> RendererDevice {
        // Create the WGPU instance and adapter as a handle to the GPU
        let instance = Instance::default();
        let adapter = instance.request_adapter(&Default::default()).block_on().unwrap();

        // Create the device and queue
        let (device, queue) = adapter.request_device(&Default::default()).block_on().unwrap();

        // Create renderers
        let normals = NormalsRenderer::new(&device);

        // Return final struct
        RendererDevice {
            device,
            queue,
            normals,
            texture: None,
        }
    }

    pub fn render(self: &mut Self, size: (usize, usize), ctx: &Context) -> &TextureHandle {
        // Render the editor
        let image = self.normals.render(size, &self.device, &self.queue).expect("Rendering failed");

        // Create or update the texture
        match &mut self.texture {
            Some(tex) => tex.set(image, egui::TextureOptions::NEAREST),
            None => {
                self.texture = Some(ctx.load_texture("normals", image, egui::TextureOptions::NEAREST));
            }
        };

        self.texture.as_ref().unwrap()
    }
}