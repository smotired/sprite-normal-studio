mod file;
mod editor;

use std::path::{Path, PathBuf};
use eframe::egui;
use rendering::Renderer;

/// Defines application state
pub struct StudioApp {
    /// Path to the current sprite file which is not modified
    sprite_path: Option<PathBuf>,
    /// Path to the current normal map file which is not modified
    normal_path: Option<PathBuf>,

    /// Renderer device
    device: Renderer,
}

/// Default initializer for application state
impl Default for StudioApp {
    fn default() -> Self {
        Self {
            sprite_path: None,
            normal_path: None,
            device: Renderer::new(),
        }
    }
}

/// Root app UI
impl eframe::App for StudioApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::Panel::left("sidebar")
            .exact_size(300.0)
            .resizable(false)
            .show(ui, |ui| {
                ui.heading("Sprite Normal Studio");
                
                ui.add(
                    file::SpriteFileSelect::new(
                        &mut self.sprite_path,
                        "Sprite file path",
                    )
                    .on_select(|path| {
                        self.normal_path = Some(generate_normal_map_filename(path.as_path()));
                    })
                );
                
                ui.add(file::SpriteFileSelect::new(&mut self.normal_path, "Normal map file path"));
            });

        egui::CentralPanel::default().frame(egui::Frame::NONE).show(ui, |ui| {
            ui.add(editor::Editor::new(&mut self.device));
        });
    }
}

/// Generate a normal map filename by appending _normal to the file name before the PNG
fn generate_normal_map_filename(sprite_path: &Path) -> PathBuf {
    let stem = sprite_path.file_stem().unwrap();
    let new_filename = stem.to_str().unwrap().to_owned() + "_normal.png";

    sprite_path.with_file_name(std::ffi::OsStr::new(&new_filename))
}