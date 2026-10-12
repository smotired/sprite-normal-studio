mod file;
mod editor;
mod input;
mod sidebar;
mod status;
mod toolbar;

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

    /// If the dialog with the full tool error text is open
    show_error_dialog: bool,

    /// Sidebar sections for each editor tool
    tool_sidebars: sidebar::ToolSidebars,
}

/// Default initializer for application state
impl StudioApp {
    pub fn new(render_state: eframe::egui_wgpu::RenderState) -> Self {
        let mut controller = Controller::new(&render_state.device);

        let renderer = Renderer::new(&render_state.device, controller.selection_buffer(), controller.object_buffers(&render_state.device));
        let viewport_texture_id = render_state.renderer.write()
            .register_native_texture(&render_state.device, renderer.view(), FilterMode::Nearest);

        Self {
            sprite_path: SpriteFileSelection::new("spritesheet".to_owned(), &render_state, None),
            normal_path: SpriteFileSelection::new("normal_map".to_owned(), &render_state, None),
            controller,
            renderer,
            render_state,
            viewport_texture_id,
            show_error_dialog: false,
            tool_sidebars: Default::default(),
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
        // Side panels must be shown before the central panel, left to right, then the status bar below the workspace
        self.show_sidebar(ui);
        let toolbar_response = self.show_toolbar(ui);
        let status_response = self.show_status_bar(ui);

        // The rest of the space will now be taken up by the editor
        let full_rect = ui.max_rect();
        let viewport_rect = egui::Rect::from_min_max(
            egui::Pos2::new(toolbar_response.rect.max.x, full_rect.min.y),
            egui::Pos2::new(full_rect.max.x, status_response.rect.min.y),
        );

        // Handle app-level input events, unless the error dialog is open and should get them instead (i.e. escape to close it)
        if !self.show_error_dialog {
            self.process_app_input(ui, viewport_rect);
        }

        // Call the controller's update method to regenerate any textures as needed
        self.controller.update(&self.render_state.device, &self.render_state.queue);

        // Render the main editor component and handle input events on the image itself
        let response = self.show_editor(ui);
        self.process_editor_input(ui, response);

        self.show_error_dialog(ui);
    }
}