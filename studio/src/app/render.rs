use std::path::{Path, PathBuf};

use controller::ViewportState;
use eframe::egui;

use crate::app::{StudioApp, editor, file};

impl StudioApp {
    // Render the actual application
    pub fn render_app(&mut self, ui: &mut egui::Ui, viewport_state: ViewportState) -> egui::Response {
        // Sidebar panel
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

        // Main editor component
        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            ui.add(editor::Editor::new(
                &mut self.renderer,
                &self.render_state,
                self.viewport_texture_id,
                viewport_state,
            ))
        }).inner
    }
}

/// Generate a normal map filename by appending _normal to the file name before the PNG
fn generate_normal_map_filename(sprite_path: &Path) -> PathBuf {
    let stem = sprite_path.file_stem().unwrap();
    let new_filename = stem.to_str().unwrap().to_owned() + "_normal.png";

    sprite_path.with_file_name(std::ffi::OsStr::new(&new_filename))
}