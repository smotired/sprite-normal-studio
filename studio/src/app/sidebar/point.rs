use controller::{Action, ControlPointMode, Controller};
use eframe::egui;

use super::ToolSidebar;

/// Sidebar options for the point tool
#[derive(Default)]
pub struct PointSidebar;

impl ToolSidebar for PointSidebar {
    fn ui(&mut self, ui: &mut egui::Ui, controller: &mut Controller) {
        // Selection info. Everything else applies to the selected points, so only show it if there are any.
        let points = controller.selected_points().to_vec();
        match points.len() {
            0 => {
                ui.label("No points selected.");
                return;
            },
            1 => ui.label(format!("Selected point: {}", points[0])),
            count => ui.label(format!("{count} points selected.")),
        };

        let button_size = egui::vec2(ui.available_width(), 0.0);
        let path_selected = controller.selected_path().is_some();
        if points.len() == 1 {
            let point_id = points[0];
            show_handle_mode(ui, controller, point_id);
            ui.add_space(ui.spacing().item_spacing.y);

            let cycle = egui::Button::new("Select sibling").min_size(button_size);
            if ui.add_enabled(controller.point_has_siblings(point_id), cycle).clicked() {
                controller.handle_action(Action::CycleSibling);
            }
        } else {
            // If a path is selected, it can be split
            if path_selected && ui.add(egui::Button::new("Split path").min_size(button_size)).clicked() {
                controller.handle_action(Action::PathSplit);
            }

            let merge = egui::Button::new("Merge").min_size(button_size);
            if ui.add_enabled(controller.can_merge_selection(), merge).clicked() {
                controller.handle_action(Action::PointsMerged);
            }
        }

        // Separating isn't offered for a selected path, which has its own options
        if !path_selected {
            let separate = egui::Button::new("Separate").min_size(button_size);
            if ui.add_enabled(controller.can_separate_selection(), separate).clicked() {
                controller.handle_action(Action::SelectionSeparated);
            }
        }
    }
}

/// Show the buttons for setting the handle mode of a point.
/// Points whose handles sync to different points show as broken, and can't be changed.
fn show_handle_mode(ui: &mut egui::Ui, controller: &mut Controller, point_id: u16) {
    let mode = controller.point_handle_mode(point_id);
    let shown_mode = mode.unwrap_or(ControlPointMode::Broken);

    ui.add_enabled_ui(mode.is_some(), |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let size = egui::vec2(ui.available_width() / 3.0, 20.0);
            for (option, label) in [
                (ControlPointMode::Linear, "Linear"),
                (ControlPointMode::Continuous, "Continuous"),
                (ControlPointMode::Broken, "Broken"),
            ] {
                let button = egui::Button::selectable(shown_mode == option, label).corner_radius(0);
                if ui.add_sized(size, button).clicked() && shown_mode != option {
                    controller.handle_action(Action::PointModeSet(option));
                }
            }
        });
    });
}
