use controller::{Action, Controller, EditorToolKind, ShapeParamChange, ShapeProfile, SpecialAction, SpecialQuery, SpecialValue};
use eframe::egui;

use super::ToolSidebar;

/// Sidebar options for the shape tool and the shape pen.
/// The params apply to the selected shapes, or to new shapes if none are selected.
#[derive(Default)]
pub struct ShapeSidebar;

impl ToolSidebar for ShapeSidebar {
    fn ui(&mut self, ui: &mut egui::Ui, controller: &mut Controller) {
        let in_shape_tool = controller.selected_tool() == EditorToolKind::Shape;
        let has_zone = matches!(controller.tool_query(SpecialQuery::ActiveZone), Some(SpecialValue::Zone(Some(_))));
        let shapes = controller.selected_shapes();
        match (in_shape_tool, shapes.len()) {
            (false, _) => ui.label("Drawing new shapes."),
            (true, _) if !has_zone => ui.label("Click a zone to edit its shapes."),
            (true, 0) => ui.label("No shapes selected. Editing values for new shapes."),
            (true, 1) => ui.label("1 shape selected."),
            (true, count) => ui.label(format!("{count} shapes selected.")),
        };
        ui.add_space(ui.spacing().item_spacing.y);

        show_params(ui, controller);
        ui.add_space(ui.spacing().item_spacing.y);

        let button_size = egui::vec2(ui.available_width(), 0.0);
        if in_shape_tool {
            let merge = egui::Button::new("Merge vertices").min_size(button_size);
            if ui.add_enabled(controller.can_merge_shape_vertices(), merge).clicked() {
                controller.handle_action(Action::Special(SpecialAction::ShapeVerticesMerged));
            }

            let delete = egui::Button::new(if shapes.len() == 1 { "Delete shape" } else { "Delete shapes" }).min_size(button_size);
            if ui.add_enabled(!controller.selected_shape_vertices().is_empty(), delete).clicked() {
                controller.handle_action(Action::Delete);
            }
        } else {
            let mut checked = matches!(controller.tool_query(SpecialQuery::StayActive), Some(SpecialValue::Flag(true)));
            if ui.checkbox(&mut checked, "Stay in Shape Pen after finishing").clicked() {
                controller.handle_action(Action::Special(SpecialAction::ToggleStayActive));
            }
        }
    }
}

/// A row of the params grid: its label, current value, drag speed, and the change it makes
type ParamRow = (&'static str, f32, f64, fn(f32) -> ShapeParamChange);

/// Show controls for the params of the selected shapes, or of new shapes if none are selected
fn show_params(ui: &mut egui::Ui, controller: &mut Controller) {
    let params = controller.selected_shape_params();

    // Profile, as buttons like the point tool's handle modes
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let size = egui::vec2(ui.available_width() / 2.0, 20.0);
        for (option, label) in [(ShapeProfile::Cone, "Cone"), (ShapeProfile::Dome, "Dome")] {
            let button = egui::Button::selectable(params.profile == option, label).corner_radius(0);
            if ui.add_sized(size, button).clicked() && params.profile != option {
                controller.handle_action(Action::Special(SpecialAction::ShapeParamSet(ShapeParamChange::Profile(option), false)));
            }
        }
    });

    egui::Grid::new("shape_params").num_columns(2).show(ui, |ui| {
        let rows: [ParamRow; 4] = [
            ("Inner radius", params.inner_radius, 0.1, ShapeParamChange::InnerRadius),
            ("Outer radius", params.outer_radius, 0.1, ShapeParamChange::OuterRadius),
            ("Steepness", params.steepness, 0.01, ShapeParamChange::Steepness),
            ("Blend", params.blend, 0.05, ShapeParamChange::Blend),
        ];
        for (label, value, speed, change) in rows {
            ui.label(label);
            let mut edited = value;
            let response = ui.add(egui::DragValue::new(&mut edited).speed(speed).range(0.0..=f32::MAX).max_decimals(2));
            if response.changed() && edited != value {
                // Only the first change in a drag starts something new to undo
                let continuing_drag = response.dragged() && !response.drag_started();
                controller.handle_action(Action::Special(SpecialAction::ShapeParamSet(change(edited), continuing_drag)));
            }
            ui.end_row();
        }
    });
}
