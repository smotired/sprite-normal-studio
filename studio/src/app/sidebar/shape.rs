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
        let zone = match controller.tool_query(SpecialQuery::ActiveZone) {
            Some(SpecialValue::Zone(zone)) => zone,
            _ => None,
        };
        let shapes = controller.selected_shapes();
        match (in_shape_tool, zone, shapes.len()) {
            (false, Some(zone_id), _) => ui.label(format!("Zone {zone_id}: drawing new shapes.")),
            (false, None, _) => ui.label("Drawing new shapes."),
            (true, None, _) => ui.label("Click a zone to edit its shapes."),
            (true, Some(zone_id), 0) => ui.label(format!("Zone {zone_id}: no shapes selected. Editing values for new shapes.")),
            (true, Some(zone_id), 1) => ui.label(format!("Zone {zone_id}: 1 shape selected.")),
            (true, Some(zone_id), count) => ui.label(format!("Zone {zone_id}: {count} shapes selected.")),
        };
        ui.add_space(ui.spacing().item_spacing.y);

        show_params(ui, controller);
        ui.add_space(ui.spacing().item_spacing.y);

        let button_size = egui::vec2(ui.available_width(), 0.0);
        if in_shape_tool {
            for (label, enabled, action) in [
                ("Merge vertices", controller.can_merge_shape_vertices(), SpecialAction::SelectionMerged),
                ("Detach vertices", controller.can_detach_shape_vertices(), SpecialAction::SelectionSeparated),
                ("Group shapes", controller.can_group_shapes(), SpecialAction::ShapesGrouped),
                ("Ungroup shapes", controller.can_ungroup_shapes(), SpecialAction::ShapesUngrouped),
            ] {
                if ui.add_enabled(enabled, egui::Button::new(label).min_size(button_size)).clicked() {
                    controller.handle_action(Action::Special(action));
                }
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

/// A row of the params grid: its label, the value the selected shapes share if they do, the value to start from if they don't,
/// its drag speed, and the change it makes
type ParamRow = (&'static str, Option<f32>, f32, f64, fn(f32) -> ShapeParamChange);

/// Show controls for the params of the selected shapes, or of new shapes if none are selected
fn show_params(ui: &mut egui::Ui, controller: &mut Controller) {
    let params = controller.selected_shape_params();

    // Profile, as buttons like the point tool's handle modes
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        let size = egui::vec2(ui.available_width() / 2.0, 20.0);
        // Neither is selected if the selected shapes have different profiles
        for (option, label) in [(ShapeProfile::Cone, "Cone"), (ShapeProfile::Dome, "Dome")] {
            let button = egui::Button::selectable(params.profile == Some(option), label).corner_radius(0);
            if ui.add_sized(size, button).clicked() && params.profile != Some(option) {
                controller.handle_action(Action::Special(SpecialAction::ShapeParamSet(ShapeParamChange::Profile(option), false)));
            }
        }
    });

    egui::Grid::new("shape_params").num_columns(2).show(ui, |ui| {
        let first = params.first;
        let rows: [ParamRow; 4] = [
            ("Inner radius", params.inner_radius, first.inner_radius, 0.1, ShapeParamChange::InnerRadius),
            ("Outer radius", params.outer_radius, first.outer_radius, 0.1, ShapeParamChange::OuterRadius),
            ("Steepness", params.steepness, first.steepness, 0.01, ShapeParamChange::Steepness),
            ("Blend", params.blend, first.blend, 0.05, ShapeParamChange::Blend),
        ];
        for (label, shared, fallback, speed, change) in rows {
            ui.label(label);
            let value = shared.unwrap_or(fallback);
            let mut edited = value;
            let mut drag = egui::DragValue::new(&mut edited).speed(speed).range(0.0..=f32::MAX).max_decimals(2);
            if shared.is_none() {
                drag = drag.custom_formatter(|_, _| "mixed".to_owned());
            }
            let response = ui.add(drag);
            if response.changed() && edited != value {
                // Only the first change in a drag starts something new to undo
                let continuing_drag = response.dragged() && !response.drag_started();
                controller.handle_action(Action::Special(SpecialAction::ShapeParamSet(change(edited), continuing_drag)));
            }
            ui.end_row();
        }
    });
}
