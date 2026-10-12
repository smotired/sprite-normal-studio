use controller::{Action, Controller, SpecialAction, SpecialQuery, SpecialValue};
use eframe::egui;

use super::{ToolSidebar, controls::NormalPicker};

/// Sidebar options for the pen tool
#[derive(Default)]
pub struct PenSidebar {
    /// Picker for the normal vector that new zones get
    normal_picker: NormalPicker,
}

impl ToolSidebar for PenSidebar {
    fn ui(&mut self, ui: &mut egui::Ui, controller: &mut Controller) {
        ui.label("Adding new zones.");
        let normal = match controller.tool_query(SpecialQuery::Normal) {
            Some(SpecialValue::Normal(normal)) => Some(normal),
            _ => None,
        };
        if let Some((normal, continuing_drag)) = self.normal_picker.show(ui, normal) {
            controller.handle_action(Action::Special(SpecialAction::NormalSet(normal, continuing_drag)));
        }
        ui.add_space(ui.spacing().item_spacing.y);

        show_toggle(ui, controller, "Inherit normal when branching", SpecialQuery::InheritNormal, SpecialAction::ToggleInherit);
        show_toggle(ui, controller, "Stay in Pen after finishing", SpecialQuery::StayActive, SpecialAction::ToggleStayActive);
    }
}

/// Show a checkbox for one of the pen's flags, which sends its toggle action when clicked
fn show_toggle(ui: &mut egui::Ui, controller: &mut Controller, label: &str, query: SpecialQuery, toggle: SpecialAction) {
    let mut checked = matches!(controller.tool_query(query), Some(SpecialValue::Flag(true)));
    if ui.checkbox(&mut checked, label).clicked() {
        controller.handle_action(Action::Special(toggle));
    }
}
