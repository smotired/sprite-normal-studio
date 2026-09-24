mod screen;

use egui::{Context, TextureHandle};
use pollster::FutureExt;
use wgpu::{Device, Instance, Queue};

use crate::screen::ScreenRenderer;

// Contains methods for initializing a renderer with a device
pub struct Renderer {
    device: Device,
    queue: Queue,
    screen: ScreenRenderer,
    texture: Option<TextureHandle>,
}

impl Renderer {
    pub fn new() -> Renderer {
        // Create the WGPU instance and adapter as a handle to the GPU
        let instance = Instance::default();
        let adapter = instance.request_adapter(&Default::default()).block_on().unwrap();

        // Create the device and queue
        let (device, queue) = adapter.request_device(&Default::default()).block_on().unwrap();

        // Create renderers
        let screen = ScreenRenderer::new(&device);

        // Return final struct
        Renderer {
            device,
            queue,
            screen,
            texture: None,
        }
    }

    pub fn render(self: &mut Self, size: (usize, usize), ctx: &Context) -> (&TextureHandle, usize) {
        // Render the editor
        let (image, image_size) = self.screen.render(size, &self.device, &self.queue).expect("Rendering failed");

        // Create or update the texture
        match &mut self.texture {
            Some(tex) => tex.set(image, egui::TextureOptions::NEAREST),
            None => {
                self.texture = Some(ctx.load_texture("editor", image, egui::TextureOptions::NEAREST));
            }
        };

        (self.texture.as_ref().unwrap(), image_size)
    }
}