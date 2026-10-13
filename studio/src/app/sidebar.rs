mod controls;
mod utils;
mod zone;
mod point;
mod pen;

use controller::{Controller, EditorToolKind};
use eframe::egui;

use crate::app::{file, workspace, StudioApp};

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
                            // Options for the selected tool
                            self.tool_sidebars.get(self.controller.selected_tool()).ui(ui, &mut self.controller);
                        });
                    });
            });
    }

    /// Pick a new spritesheet for the open project. The base normal map is also reset to the one next to it,
    /// or a flat one if there isn't one. Shows an error if the picked file can't be loaded.
    pub(super) fn load_spritesheet(&mut self) {
        let Some(project) = &mut self.project else { return; };
        let Some(path) = file::pick_png(Some(project.sprite.path())) else { return; };

        match project.sprite.try_select(path.clone(), &self.render_state) {
            Ok(size) => {
                project.normal.select_or_fill(
                    file::generate_normal_map_filename(&path),
                    &self.render_state,
                    size,
                    file::loader::FLAT_NORMAL_COLOR,
                );
                self.update_input_textures();
            },
            Err(e) => workspace::show_message(rfd::MessageLevel::Error, "Error", &format!("Could not load {}: {e:#}", path.display())),
        }
    }

    /// Pick a new base normal map for the open project. Shows an error if the picked file can't be loaded.
    pub(super) fn load_normal_map(&mut self) {
        let Some(project) = &mut self.project else { return; };
        let Some(path) = file::pick_png(Some(project.normal.path())) else { return; };

        match project.normal.try_select(path.clone(), &self.render_state) {
            Ok(_) => self.update_input_textures(),
            Err(e) => workspace::show_message(rfd::MessageLevel::Error, "Error", &format!("Could not load {}: {e:#}", path.display())),
        }
    }
}
