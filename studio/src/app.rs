mod file;
mod editor;
mod input;

use std::path::{Path, PathBuf};

use controller::{Controller, EditorToolKind};
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
        // Render the sidebar panel
        egui::Panel::left("sidebar")
            .exact_size(240.0)
            .resizable(false)
            .show(ui, |ui| {
                ui.heading("Sprite Normal Studio");

                let mut must_update_textures = false;
                
                // Spritesheet selector button
                ui.add(
                    file::SpriteFileSelect::new(
                        &mut self.sprite_path,
                        "Spritesheet",
                        &self.render_state,
                    )
                    .on_select(|path, size| {
                        // Also reselect the normal map file when selecting a new sprite
                        self.normal_path.select_or_fill(
                            generate_normal_map_filename(path.as_path()), 
                            &self.render_state,
                            size,
                            [128, 128, 255, 255],
                        );

                        must_update_textures = true;
                    })
                );
                
                // Normal map selector button
                ui.add(
                    file::SpriteFileSelect::new(
                        &mut self.normal_path,
                        "Normal Map",
                        &self.render_state,
                    )
                    .on_select(|_path, _size| {
                        must_update_textures = true;
                    })
                );

                // Regenerate the normal map and viewport if selections changed
                if must_update_textures {
                    self.update_input_textures();
                }
            });

        // Render the tools panel directly to the right of it
        let toolbar_response = egui::Panel::left("tools")
            .exact_size(21.0)
            .frame(
                egui::Frame::side_top_panel(ui.style()) // or egui::Frame::default()
                    .inner_margin(egui::Margin::same(0)) // 10 pixels padding on all sides
            )
            .resizable(false)
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    // TODO: Icons
                    
                    let selected_color = egui::Color32::DARK_GRAY;
                    let default_color = egui::Color32::TRANSPARENT;
                    
                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

                    if ui.add_sized(
                        [20.0, 20.0],
                        egui::Button::new("Z")
                            .corner_radius(0)
                            .fill(if self.controller.selected_tool() == EditorToolKind::Zone { selected_color } else { default_color })
                    ).clicked() {
                        self.controller.select_tool(EditorToolKind::Zone);
                    }

                    if ui.add_sized(
                        [20.0, 20.0],
                        egui::Button::new("P")
                            .corner_radius(0)
                            .fill(if self.controller.selected_tool() == EditorToolKind::Point { selected_color } else { default_color })
                    ).clicked() {
                        self.controller.select_tool(EditorToolKind::Point);
                    }

                    if ui.add_sized(
                        [20.0, 20.0],
                        egui::Button::new("A")
                            .corner_radius(0)
                            .fill(if self.controller.selected_tool() == EditorToolKind::Pen { selected_color } else { default_color })
                    ).clicked() {
                        self.controller.select_tool(EditorToolKind::Pen);
                    }
                });
            });

        // The rest of the space will now be taken up by the editor
        let full_rect = ui.max_rect();
        let viewport_rect = egui::Rect::from_min_max(
            egui::Pos2::new(toolbar_response.response.rect.max.x, full_rect.min.y),
            full_rect.max,
        );
        let ppp = ui.pixels_per_point();

        // Handle app-level input events
        ui.input_mut(|i| {
            for event in self.handle_app_input(i, (viewport_rect, ppp)) {
                self.controller.handle_input(event);
            }

            // Set world space cursor position
            let viewport_size = self.renderer.size();
            if let Some(mouse_world) = i.pointer.latest_pos()
                .filter(|pos| viewport_rect.contains(*pos))
                .map(|pos| {
                    let local = (pos - viewport_rect.min) * ppp; // egui::Vec2, points -> local origin
                    self.controller.screen_to_world(studio_math::Vec2::from(local), viewport_size)
                })
            {
                self.controller.set_cursor_pos(mouse_world);
            }
        });

        // Call the controller's update method to regenerate any textures as needed
        self.controller.update(&self.render_state.device, &self.render_state.queue);

        // Render the main editor component and get the response for the image itself
        let response = egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            ui.add(editor::Editor::new(
                &mut self.renderer,
                &self.render_state,
                self.viewport_texture_id,
                &mut self.controller,
            ))
        }).inner;

        // Handle input events on the image itself, i.e. click and drag
        for event in self.handle_editor_input(ui, response) {
            self.controller.handle_input(event);
        }
    }
}

/// Generate a normal map filename by appending _normal to the file name before the PNG
fn generate_normal_map_filename(sprite_path: &Path) -> PathBuf {
    let stem = sprite_path.file_stem().unwrap();
    let new_filename = stem.to_str().unwrap().to_owned() + "_normal.png";

    sprite_path.with_file_name(std::ffi::OsStr::new(&new_filename))
}


#[cfg(test)]
mod tests {
    use super::*;

    /// _normal is added to the stem and the directory is kept
    #[test]
    fn normal_map_filename() {
        assert_eq!(
            generate_normal_map_filename(&PathBuf::from("art").join("hero.png")),
            PathBuf::from("art").join("hero_normal.png"),
        );
        assert_eq!(generate_normal_map_filename(Path::new("hero.png")), PathBuf::from("hero_normal.png"));
    }

    /// Only the final extension is replaced
    #[test]
    fn normal_map_filename_with_dots() {
        assert_eq!(generate_normal_map_filename(Path::new("hero.idle.png")), PathBuf::from("hero.idle_normal.png"));
    }
}
