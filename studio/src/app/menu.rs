use eframe::egui;

use crate::app::StudioApp;
use crate::app::workspace::WorkspaceCommand;

impl StudioApp {
    /// Render the menu bar across the top of the window. Must be shown before every other panel so it spans the whole window.
    /// Returns the workspace command that was clicked, if any.
    pub(super) fn show_menu_bar(&mut self, ui: &mut egui::Ui) -> Option<WorkspaceCommand> {
        let mut command = None;
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    for (label, item) in [
                        ("Open…", WorkspaceCommand::Open),
                        ("Save", WorkspaceCommand::Save),
                        ("Save As…", WorkspaceCommand::SaveAs),
                    ] {
                        let shortcut = ui.ctx().format_shortcut(&item.shortcut());
                        if ui.add(egui::Button::new(label).shortcut_text(shortcut)).clicked() {
                            command = Some(item);
                        }
                    }
                });
            });
        });
        command
    }
}
