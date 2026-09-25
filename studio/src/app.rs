mod file;
mod editor;

use std::path::{Path, PathBuf};
use eframe::egui;
use rendering::Renderer;

use crate::app::file::loader::SpriteFileSelection;

/// Defines application state
pub struct StudioApp {
    /// Path to the current sprite file which is not modified
    sprite_path: SpriteFileSelection,
    /// Path to the current normal map file which is not modified
    normal_path: SpriteFileSelection,

    /// Renderer device
    renderer: Renderer,
    /// Render state
    render_state: eframe::egui_wgpu::RenderState,
}

/// Default initializer for application state
impl StudioApp {
    pub fn new(render_state: eframe::egui_wgpu::RenderState) -> Self {
        Self {
            sprite_path: SpriteFileSelection::new("spritesheet".to_owned(), &render_state, None),
            normal_path: SpriteFileSelection::new("normal_map".to_owned(), &render_state, None),
            renderer: Renderer::new(&render_state),
            render_state,
        }
    }
}

/// Root app UI
impl eframe::App for StudioApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::left("sidebar")
            .exact_size(240.0)
            .resizable(false)
            .show(ui, |ui| {
                ui.heading("Sprite Normal Studio");
                
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
                    })
                );
                
                ui.add(
                    file::SpriteFileSelect::new(
                        &mut self.normal_path,
                        "Normal Map",
                        &self.render_state,
                    )
                );
            });

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            ui.add(editor::Editor::new(&mut self.renderer, &self.render_state));
        });
    }
}

/// Generate a normal map filename by appending _normal to the file name before the PNG
fn generate_normal_map_filename(sprite_path: &Path) -> PathBuf {
    let stem = sprite_path.file_stem().unwrap();
    let new_filename = stem.to_str().unwrap().to_owned() + "_normal.png";

    sprite_path.with_file_name(std::ffi::OsStr::new(&new_filename))
}