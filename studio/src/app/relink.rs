use std::path::Path;

use eframe::egui;

use crate::app::file::{self, loader::SpriteFileSelection};
use crate::app::new_project::WIDTH;
use crate::app::workspace::{show_result, PendingOpen, NORMAL_LABEL, SPRITE_LABEL};
use crate::app::StudioApp;

/// State of the Missing Files dialog, shown when a workspace file's spritesheet or normal map can't be loaded.
/// The user picks where they are now, and the workspace opens with the new paths. Nothing is saved until the user saves.
pub(super) struct RelinkDialog {
    pending: PendingOpen,
    sprite: Option<SpriteFileSelection>,
    /// The base normal map, or None for a flat one
    normal: Option<SpriteFileSelection>,
    /// If the workspace file has a base normal map that couldn't be loaded, and nothing has replaced it yet
    normal_missing: bool,
    error: Option<String>,
}

/// What the user did with the dialog this frame
enum DialogAction {
    None,
    Open,
    Cancel,
}

impl RelinkDialog {
    /// Create the dialog for a workspace file, with whichever images did load
    pub(super) fn new(pending: PendingOpen, sprite: Option<SpriteFileSelection>, normal: Option<SpriteFileSelection>) -> Self {
        let normal_missing = pending.normal_path.is_some() && normal.is_none();
        Self { pending, sprite, normal, normal_missing, error: None }
    }

    /// Render the dialog's contents. Selections replaced by new picks are added to `retired`.
    fn ui(&mut self, ui: &mut egui::Ui, rs: &eframe::egui_wgpu::RenderState, retired: &mut Vec<SpriteFileSelection>) -> DialogAction {
        ui.set_width(WIDTH);
        ui.heading("Missing Files");
        ui.add(egui::Label::new("Some files used in the workspace couldn't be loaded. Pick where they are now.").wrap());
        ui.add_space(8.0);

        // Spritesheet
        let result = file::pick_image(ui, "Spritesheet", true, &mut self.sprite, SPRITE_LABEL, rs, retired);
        self.handle_pick(result);
        if self.sprite.is_none() {
            not_found(ui, &self.pending.sprite_path);
        }
        file::preview(ui, self.sprite.as_ref(), "None selected");
        ui.add_space(8.0);

        // Base normal map. A missing one can be replaced, or dropped to start from a flat normal map instead.
        let result = file::pick_image(ui, "Base Normal Map", true, &mut self.normal, NORMAL_LABEL, rs, retired);
        if result == Ok(true) { self.normal_missing = false; }
        self.handle_pick(result);
        if self.normal_missing && let Some(normal_path) = &self.pending.normal_path {
            not_found(ui, normal_path);
        }
        file::preview(ui, self.normal.as_ref(), if self.normal_missing { "None selected" } else { file::FLAT_NORMAL_TEXT });
        if self.normal_missing {
            let flat = file::right_aligned_button(ui, "Create flat normal map")
                .on_hover_text("Start from a flat normal map instead, with no base normal map file");
            if flat.clicked() {
                self.normal_missing = false;
            }
        }

        if let Some(error) = &self.error {
            ui.add_space(8.0);
            ui.add(egui::Label::new(egui::RichText::new(format!("⚠ {error}")).color(egui::Color32::YELLOW)).wrap());
        }

        ui.add_space(8.0);
        ui.separator();

        // Buttons, on the right, added right to left so Open is last
        let can_open = self.sprite.is_some() && !self.normal_missing;
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let open = ui.add_enabled(can_open, egui::Button::new("Open"));
            let cancel = ui.button("Cancel");
            if open.clicked() {
                DialogAction::Open
            } else if cancel.clicked() {
                DialogAction::Cancel
            } else {
                DialogAction::None
            }
        }).inner
    }

    /// Update the error after picking a file
    fn handle_pick(&mut self, result: Result<bool, String>) {
        match result {
            Ok(true) => self.error = None,
            Ok(false) => {},
            Err(e) => self.error = Some(e),
        }
    }
}

/// Say where a file was expected
fn not_found(ui: &mut egui::Ui, path: &Path) {
    ui.add(egui::Label::new(
        egui::RichText::new(format!("Not found: {}", path.display())).color(egui::Color32::YELLOW).small()
    ).wrap());
}

impl StudioApp {
    /// Render the Missing Files dialog, if it is open, and finish opening the workspace when asked
    pub(super) fn show_relink_dialog(&mut self, ui: &mut egui::Ui) {
        let Some(dialog) = &mut self.relink_dialog else { return; };

        let modal = egui::Modal::new(egui::Id::new("relink_dialog")).show(ui.ctx(), |ui| {
            dialog.ui(ui, &self.render_state, &mut self.retired_selections)
        });

        match modal.inner {
            DialogAction::Open => {
                let Some(dialog) = self.relink_dialog.take() else { return; };
                match dialog.sprite {
                    Some(sprite) => show_result(Ok(self.finish_open(ui.ctx(), dialog.pending, sprite, dialog.normal))),
                    None => self.retired_selections.extend(dialog.normal),
                }
            },
            DialogAction::Cancel => self.close_relink_dialog(),
            DialogAction::None if modal.should_close() => self.close_relink_dialog(),
            DialogAction::None => {},
        }
    }

    /// Close the Missing Files dialog without opening the workspace, freeing any images it loaded
    fn close_relink_dialog(&mut self) {
        if let Some(dialog) = self.relink_dialog.take() {
            self.retired_selections.extend(dialog.sprite);
            self.retired_selections.extend(dialog.normal);
        }
    }
}
