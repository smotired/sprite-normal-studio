use controller::Status;
use eframe::egui;

use crate::app::StudioApp;

impl StudioApp {
    /// Render the one-line status bar below the workspace. Must be shown after the side panels so it only spans the workspace.
    /// Shows a checkmark, with feedback text if there is any, or the error from the latest tool action if it failed.
    /// Clicking text opens the full text, in case it was truncated.
    pub(super) fn show_status_bar(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let status = self.project().map(|project| project.controller.status().clone()).unwrap_or_default();
        let status = &status;
        let response = egui::Panel::bottom("status")
            .resizable(false)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    match status {
                        Status::Ok => {
                            ui.label(egui::RichText::new("✔").color(egui::Color32::GREEN));
                        },
                        Status::Feedback(text) => {
                            ui.label(egui::RichText::new("✔").color(egui::Color32::GREEN));
                            ui.add(egui::Label::new(egui::RichText::new(text).weak()).truncate().selectable(false));
                        },
                        Status::Error(text) => {
                            ui.add(
                                egui::Label::new(egui::RichText::new(format!("⚠ {text}")).color(egui::Color32::YELLOW))
                                    .truncate()
                                    .selectable(false)
                            );
                        },
                    }
                });
            })
            .response;

        // Open the full text when the bar is clicked
        if *status != Status::Ok {
            let response = response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() {
                self.show_status_dialog = true;
            }
            return response;
        }
        response
    }

    /// Render the dialog with the full status text, if it is open
    pub(super) fn show_status_dialog(&mut self, ui: &mut egui::Ui) {
        if !self.show_status_dialog { return; }
        let Some(project) = self.projects.get(self.current) else { return; };
        let (heading, text) = match project.controller.status() {
            Status::Ok => {
                self.show_status_dialog = false;
                return;
            },
            Status::Feedback(text) => ("✔ Status", text),
            Status::Error(text) => ("⚠ Error", text),
        };

        let modal = egui::Modal::new(egui::Id::new("status_dialog")).show(ui.ctx(), |ui| {
            ui.set_max_width(400.0);
            ui.heading(heading);
            ui.add(egui::Label::new(text.as_str()).wrap());
            ui.button("OK").clicked()
        });

        if modal.inner || modal.should_close() {
            self.show_status_dialog = false;
        }
    }
}
