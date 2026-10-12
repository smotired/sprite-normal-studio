use eframe::egui;

use crate::app::StudioApp;

impl StudioApp {
    /// Render the one-line status bar below the workspace. Must be shown after the side panels so it only spans the workspace.
    /// Shows a checkmark if the last tool action succeeded, or its error if it failed. Clicking an error opens the full text.
    pub(super) fn show_status_bar(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let response = egui::Panel::bottom("status")
            .resizable(false)
            .show(ui, |ui| {
                match self.controller.tool_error() {
                    None => {
                        ui.label(egui::RichText::new("✔").color(egui::Color32::GREEN));
                    },
                    Some(error) => {
                        ui.add(
                            egui::Label::new(egui::RichText::new(format!("⚠ {error}")).monospace().color(egui::Color32::YELLOW))
                                .truncate()
                                .selectable(false)
                        );
                    },
                }
            })
            .response;

        // Open the full error text when the bar is clicked, in case it was truncated
        if self.controller.tool_error().is_some() {
            let response = response.interact(egui::Sense::click()).on_hover_cursor(egui::CursorIcon::PointingHand);
            if response.clicked() {
                self.show_error_dialog = true;
            }
            return response;
        }
        response
    }

    /// Render the dialog with the full text of the tool error, if it is open
    pub(super) fn show_error_dialog(&mut self, ui: &mut egui::Ui) {
        if !self.show_error_dialog { return; }
        let Some(error) = self.controller.tool_error() else {
            self.show_error_dialog = false;
            return;
        };

        let modal = egui::Modal::new(egui::Id::new("tool_error_dialog")).show(ui.ctx(), |ui| {
            ui.set_max_width(400.0);
            ui.heading("⚠ Tool Error");
            ui.add(egui::Label::new(egui::RichText::new(error).monospace()).wrap());
            ui.button("OK").clicked()
        });

        if modal.inner || modal.should_close() {
            self.show_error_dialog = false;
        }
    }
}
