use eframe::egui;

use crate::app::{recent, StudioApp};
use crate::app::workspace::WorkspaceCommand;

impl StudioApp {
    /// Render the menu bar across the top of the window. Must be shown before every other panel so it spans the whole window.
    /// Returns the workspace command that was clicked, if any.
    pub(super) fn show_menu_bar(&mut self, ui: &mut egui::Ui) -> Option<WorkspaceCommand> {
        let mut command = None;
        let mut open_recent = None;
        let has_project = self.project.is_some();
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| {
                    command_button(ui, "New…", WorkspaceCommand::New, has_project, &mut command);
                    command_button(ui, "Open…", WorkspaceCommand::Open, has_project, &mut command);

                    // Recently opened or saved files, opened directly
                    let recent = &self.recent.paths()[..self.recent.paths().len().min(recent::MENU_LIMIT)];
                    ui.add_enabled_ui(!recent.is_empty(), |ui| {
                        ui.menu_button("Recent Files", |ui| {
                            for path in recent {
                                let name = path.file_name().unwrap_or(path.as_os_str()).to_string_lossy();
                                if ui.button(name).on_hover_text(path.display().to_string()).clicked() {
                                    open_recent = Some(path.clone());
                                }
                            }
                        });
                    });

                    command_button(ui, "Save", WorkspaceCommand::Save, has_project, &mut command);
                    command_button(ui, "Save As…", WorkspaceCommand::SaveAs, has_project, &mut command);
                    command_button(ui, "Close", WorkspaceCommand::Close, has_project, &mut command);
                });
            });
        });

        // Open after the menu is shown, since the menu borrows the list
        if let Some(path) = open_recent {
            self.open_recent(&ui.ctx().clone(), &path);
        }
        command
    }
}

/// A menu button for a workspace command, showing its shortcut. Disabled if it needs a project and there isn't one.
fn command_button(ui: &mut egui::Ui, label: &str, item: WorkspaceCommand, has_project: bool, command: &mut Option<WorkspaceCommand>) {
    let shortcut = ui.ctx().format_shortcut(&item.shortcut());
    let enabled = has_project || item.works_without_project();
    if ui.add_enabled(enabled, egui::Button::new(label).shortcut_text(shortcut)).clicked() {
        *command = Some(item);
    }
}
