mod controls;
mod utils;
mod zone;
mod point;
mod pen;

use controller::{Controller, EditorToolKind};
use eframe::egui;

use crate::app::{file, workspace, StudioApp};
use crate::app::file::loader::SpriteFileSelection;

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

    /// Pick a new spritesheet for the open project. Shows an error if the picked file can't be loaded.
    pub(super) fn load_spritesheet(&mut self) {
        let Some(project) = &mut self.project else { return; };
        let Some(path) = file::pick_png(Some(project.sprite.path())) else { return; };

        match project.sprite.try_select(path.clone(), &self.render_state) {
            Ok(_) => self.update_input_textures(),
            Err(e) => show_load_error(&path, e),
        }
    }

    /// Pick a new base normal map for the open project. Shows an error if the picked file can't be loaded.
    pub(super) fn load_normal_map(&mut self) {
        let Some(project) = &mut self.project else { return; };
        let current = project.normal.as_ref().map_or(project.sprite.path(), |normal| normal.path());
        let Some(path) = file::pick_png(Some(current)) else { return; };

        let result = match &mut project.normal {
            Some(normal) => normal.try_select(path.clone(), &self.render_state).map(|_| ()),
            None => SpriteFileSelection::open(workspace::NORMAL_LABEL, path.clone(), &self.render_state)
                .map(|normal| project.normal = Some(normal)),
        };
        match result {
            Ok(()) => self.update_input_textures(),
            Err(e) => show_load_error(&path, e),
        }
    }

    /// Stop using a base normal map for the open project, so the normal map starts flat
    pub(super) fn reset_base_normals(&mut self) {
        let Some(project) = &mut self.project else { return; };
        let Some(normal) = project.normal.take() else { return; };
        self.retired_selections.push(normal);
        self.update_input_textures();
    }
}

/// Show an error for an image that couldn't be loaded
fn show_load_error(path: &std::path::Path, e: anyhow::Error) {
    workspace::show_message(rfd::MessageLevel::Error, "Error", &format!("Could not load {}: {e:#}", path.display()));
}
