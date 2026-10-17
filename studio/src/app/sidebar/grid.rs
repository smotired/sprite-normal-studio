use controller::{Controller, GRID_SIZES};
use eframe::egui;

/// Show the grid size slider, which snaps to GRID_SIZES. The grid is what ctrl snaps to and what the arrow keys move by.
pub(super) fn show_grid_size(ui: &mut egui::Ui, controller: &mut Controller) {
    let mut index = GRID_SIZES.iter().position(|&size| size == controller.grid_size()).unwrap_or(0);
    ui.horizontal(|ui| {
        ui.label("Grid size");
        let slider = egui::Slider::new(&mut index, 0..=GRID_SIZES.len() - 1).show_value(false);
        let response = ui.add(slider);
        if response.changed() {
            controller.set_grid_size(GRID_SIZES[index]);
        }

        // The slider would keep the arrow keys, which should go back to moving the selection
        if response.drag_stopped() || response.clicked() {
            response.surrender_focus();
        }
        ui.label(format!("{} px", GRID_SIZES[index]));
    });
}

/// Show the width and height of each sprite the spritesheet is sliced into
pub(super) fn show_sprite_size(ui: &mut egui::Ui, controller: &mut Controller) {
    let (sheet_width, sheet_height) = controller.sheet_size();
    let (mut width, mut height) = controller.sprite_size();
    ui.horizontal(|ui| {
        ui.label("Sprite size");
        let changed = sprite_size_fields(ui, &mut width, &mut height, (sheet_width, sheet_height));
        if changed {
            controller.set_sprite_size((width, height));
        }
    });
}

/// Fields for a sprite's width and height, each from 1 pixel up to the spritesheet's size. Returns true if either changed.
pub(crate) fn sprite_size_fields(ui: &mut egui::Ui, width: &mut u32, height: &mut u32, (sheet_width, sheet_height): (u32, u32)) -> bool {
    let width_changed = ui.add(egui::DragValue::new(width).range(1..=sheet_width.max(1)).suffix(" px")).changed();
    ui.label("×");
    let height_changed = ui.add(egui::DragValue::new(height).range(1..=sheet_height.max(1)).suffix(" px")).changed();
    width_changed || height_changed
}
