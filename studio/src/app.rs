mod file;
mod editor;

use std::path::{Path, PathBuf};
use eframe::egui;
use rendering::device::RendererDevice;

/// Defines application state
pub struct StudioApp {
    /// Path to the current sprite file which is not modified
    sprite_path: Option<PathBuf>,
    /// Path to the current normal map file which is not modified
    normal_path: Option<PathBuf>,

    /// Renderer device
    device: RendererDevice,
}

/// Default initializer for application state
impl Default for StudioApp {
    fn default() -> Self {
        Self {
            sprite_path: None,
            normal_path: None,
            device: RendererDevice::new(),
        }
    }
}

/// Root app UI
impl eframe::App for StudioApp {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ui, |ui| {
            ui.columns(2, |columns|{
                // Left sidebar
                columns[0].heading("Sprite Normal Studio");
                
                columns[0].add(
                    file::SpriteFileSelect::new(
                        &mut self.sprite_path,
                        "Sprite file path",
                    )
                    .on_select(|path| {
                        self.normal_path = Some(generate_normal_map_filename(path.as_path()));
                    })
                );
                
                columns[0].add(file::SpriteFileSelect::new(&mut self.normal_path, "Normal map file path"));

                // Main content editor
                columns[1].add(editor::Editor::new(&mut self.device));
            });
        });
    }
}

/// Generate a normal map filename by appending _normal to the file name before the PNG
fn generate_normal_map_filename(sprite_path: &Path) -> PathBuf {
    let stem = sprite_path.file_stem().unwrap();
    let new_filename = stem.to_str().unwrap().to_owned() + "_normal.png";

    sprite_path.with_file_name(std::ffi::OsStr::new(&new_filename))
}