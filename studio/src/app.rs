mod file;
mod editor;
mod input;
mod render;

use controller::Controller;
use eframe::egui;
use eframe::wgpu::FilterMode;
use rendering::Renderer;

use crate::app::file::loader::SpriteFileSelection;

/// Defines application state
pub struct StudioApp {
    /// Path to the current sprite file which is not modified
    sprite_path: SpriteFileSelection,
    /// Path to the current normal map file which is not modified
    normal_path: SpriteFileSelection,

    /// Controller object
    controller: Controller,

    /// Renderer device
    renderer: Renderer,
    /// Render state
    render_state: eframe::egui_wgpu::RenderState,
    /// Renderer viewport texture ID
    viewport_texture_id: egui::TextureId,
}

/// Default initializer for application state
impl StudioApp {
    pub fn new(render_state: eframe::egui_wgpu::RenderState) -> Self {
        let renderer = Renderer::new(&render_state.device);
        let viewport_texture_id = render_state.renderer.write()
            .register_native_texture(&render_state.device, renderer.view(), FilterMode::Nearest);

        Self {
            sprite_path: SpriteFileSelection::new("spritesheet".to_owned(), &render_state, None),
            normal_path: SpriteFileSelection::new("normal_map".to_owned(), &render_state, None),
            controller: Controller::new(&render_state.device),
            renderer,
            render_state,
            viewport_texture_id,
        }
    }

    fn update_input_textures(&mut self) {
        self.controller.set_inputs(&self.render_state.device, self.sprite_path.texture(), self.normal_path.texture());
        self.renderer.set_inputs(&self.render_state.device, self.sprite_path.texture(), self.controller.output());
    }
}

/// Root app UI
impl eframe::App for StudioApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        // Handle input events
        ui.input_mut(|i| {
            for event in self.handle_input(i) {
                self.controller.handle_input(event);
            }
        });

        // Call the controller's update method to regenerate any textures as needed
        let viewport_state = self.controller.update(&self.render_state.device, &self.render_state.queue);

        // Render the application
        self.render_app(ui, viewport_state);
    }
}