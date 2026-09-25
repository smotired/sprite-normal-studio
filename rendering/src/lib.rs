mod screen;

// TODO: May make this crate just for rendering the screen and have another crate for compute shaders for the actual like images

use crate::screen::ScreenRenderer;

// Contains methods for initializing a renderer with a device
pub struct Renderer {
    screen: ScreenRenderer,
}

impl Renderer {
    pub fn new(render_state: &egui_wgpu::RenderState) -> Renderer {
        // Create renderers
        let screen = ScreenRenderer::new(render_state);

        // Return final struct
        Renderer {
            screen
        }
    }

    pub fn render(&mut self, render_state: &egui_wgpu::RenderState, size: (usize, usize)) -> egui::TextureId {
        // Render the editor
        self.screen.render(render_state, size)
    }
}