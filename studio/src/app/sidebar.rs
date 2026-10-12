use eframe::egui;
use std::path::{Path, PathBuf};

use crate::app::{file, StudioApp};

impl StudioApp {
    /// Render the left sidebar panel. Must be shown before the central panel.
    pub(super) fn show_sidebar(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("sidebar")
            .exact_size(240.0)
            .resizable(false)
            .show(ui, |ui| {
                ui.heading("Sprite Normal Studio");

                self.show_file_selectors(ui);
            });
    }

    /// Spritesheet and normal map file selectors
    fn show_file_selectors(&mut self, ui: &mut egui::Ui) {
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
