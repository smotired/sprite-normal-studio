use controller::{Controller, EditorToolKind};
use eframe::egui;

use crate::app::StudioApp;

impl StudioApp {
    /// Render the tool selection strip. Must be shown after the sidebar so it sits directly to its right.
    /// Returns the panel's response so the viewport can be placed beside it.
    pub(super) fn show_toolbar(&mut self, ui: &mut egui::Ui) -> egui::Response {
        let controller = self.projects.get_mut(self.current).map(|project| &mut project.controller);
        egui::Panel::left("tools")
            .exact_size(21.0)
            .frame(
                egui::Frame::side_top_panel(ui.style()) // or egui::Frame::default()
                    .inner_margin(egui::Margin::same(0)) // 10 pixels padding on all sides
            )
            .resizable(false)
            .show(ui, |ui| {
                ui.with_layout(egui::Layout::top_down(egui::Align::Center), |ui| {
                    // TODO: Icons

                    ui.spacing_mut().item_spacing = egui::vec2(0.0, 0.0);

                    let Some(controller) = controller else { return; };
                    tool_button(ui, controller, "Z", EditorToolKind::Zone);
                    tool_button(ui, controller, "P", EditorToolKind::Point);
                    tool_button(ui, controller, "A", EditorToolKind::Pen);
                });
            })
            .response
    }
}

/// A single square toolbar button that selects `kind` when clicked
fn tool_button(ui: &mut egui::Ui, controller: &mut Controller, label: &str, kind: EditorToolKind) {
    let selected_color = egui::Color32::DARK_GRAY;
    let default_color = egui::Color32::TRANSPARENT;

    if ui.add_sized(
        [20.0, 20.0],
        egui::Button::new(label)
            .corner_radius(0)
            .fill(if controller.selected_tool() == kind { selected_color } else { default_color })
    ).clicked() {
        controller.select_tool(kind);
    }
}
