mod controls;
mod utils;
mod zone;
mod point;
mod pen;

use controller::{Controller, EditorToolKind};
use eframe::egui;
use std::path::{Path, PathBuf};

use crate::app::{file, StudioApp};

/// A section of the sidebar with options for one editor tool.
/// Each one keeps its own state, which is kept when switching between tools.
trait ToolSidebar {
    fn ui(&mut self, ui: &mut egui::Ui, controller: &mut Controller);
}

/// The sidebar sections for every editor tool
#[derive(Default)]
pub(super) struct ToolSidebars {
    zone: zone::ZoneSidebar,
    point: point::PointSidebar,
    pen: pen::PenSidebar,
}

impl ToolSidebars {
    /// Get the sidebar section for a tool
    fn get(&mut self, tool: EditorToolKind) -> &mut dyn ToolSidebar {
        match tool {
            EditorToolKind::Zone  => &mut self.zone,
            EditorToolKind::Point => &mut self.point,
            EditorToolKind::Pen   => &mut self.pen,
        }
    }
}

impl StudioApp {
    /// Render the left sidebar panel. Must be shown before the central panel.
    pub(super) fn show_sidebar(&mut self, ui: &mut egui::Ui) {
        // The padding goes on the contents instead of the panel, so the scrollbar is at the edge of the panel
        let margin = egui::Margin::symmetric(8, 2);
        egui::Panel::left("sidebar")
            .exact_size(240.0)
            .resizable(false)
            .frame(egui::Frame::side_top_panel(ui.style()).inner_margin(0))
            .show(ui, |ui| {
                // The heading stays at the top while the rest scrolls
                egui::Frame::new().inner_margin(margin).show(ui, |ui| {
                    ui.heading("Sprite Normal Studio");
                });

                egui::ScrollArea::vertical()
                    .auto_shrink([false, false])
                    .show(ui, |ui| {
                        egui::Frame::new().inner_margin(margin).show(ui, |ui| {
                            self.show_file_selectors(ui);
                            ui.separator();

                            // Options for the selected tool
                            self.tool_sidebars.get(self.controller.selected_tool()).ui(ui, &mut self.controller);
                        });
                    });
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
                    file::loader::FLAT_NORMAL_COLOR,
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
