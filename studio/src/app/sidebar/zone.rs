use controller::{Action, Controller, SpecialAction};
use eframe::egui;

use super::{ToolSidebar, controls::NormalPicker};

/// Sidebar options for the zone tool
#[derive(Default)]
pub struct ZoneSidebar {
    /// Picker for the selected zones' normal vector
    normal_picker: NormalPicker,
}

impl ToolSidebar for ZoneSidebar {
    fn ui(&mut self, ui: &mut egui::Ui, controller: &mut Controller) {
        // Selection info. Everything else applies to the selected zones, so only show it if there are any.
        let zones = controller.selected_zones();
        match zones.len() {
            0 => {
                ui.label("No zones selected.");
                return;
            },
            1 => ui.label(format!("Selected zone: {}", zones[0])),
            count => ui.label(format!("{count} zones selected.")),
        };
        let plural = zones.len() > 1;

        if let Some((normal, continuing_drag)) = self.normal_picker.show(ui, controller.selected_zones_normal()) {
            controller.handle_action(Action::Special(SpecialAction::NormalSet(normal, continuing_drag)));
        }
        ui.add_space(ui.spacing().item_spacing.y);

        // Zone actions
        let button_size = egui::vec2(ui.available_width(), 0.0);
        let separate = egui::Button::new(if plural { "Separate zones" } else { "Separate zone" }).min_size(button_size);
        if ui.add_enabled(controller.can_separate_selection(), separate).clicked() {
            controller.handle_action(Action::Special(SpecialAction::SelectionSeparated));
        }

        let delete = egui::Button::new(if plural { "Delete zones" } else { "Delete zone" }).min_size(button_size);
        if ui.add(delete).clicked() {
            controller.handle_action(Action::Delete);
        }

        // TODO: Clear shapes once they exist
        ui.add(egui::Button::new("Clear shapes").min_size(button_size));
    }
}
