use std::path::Path;

use eframe::egui;

use crate::app::file::{self, loader::{SpriteFileSelection, FLAT_NORMAL_COLOR}};
use crate::app::new_project::WIDTH;
use crate::app::workspace::{show_result, PendingOpen, NORMAL_LABEL, SPRITE_LABEL};
use crate::app::StudioApp;

/// State of the Missing Files dialog, shown when a workspace file's spritesheet or normal map can't be loaded.
/// The user picks where they are now, and the workspace opens with the new paths. Nothing is saved until the user saves.
pub(super) struct RelinkDialog {
    pending: PendingOpen,
    sprite: Option<SpriteFileSelection>,
    normal: Option<SpriteFileSelection>,
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
        Self { pending, sprite, normal, error: None }
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
        file::preview(ui, self.sprite.as_ref());
        ui.add_space(8.0);

        // Base normal map, which can also start flat at its saved path, if the spritesheet is loaded to give its size
        let result = file::pick_image(ui, "Base Normal Map", true, &mut self.normal, NORMAL_LABEL, rs, retired);
        self.handle_pick(result);
        let missing = self.normal.is_none();
        if missing {
            not_found(ui, &self.pending.normal_path);
        }
        file::preview(ui, self.normal.as_ref());
        if missing {
            // In a one-row horizontal so the right-to-left layout doesn't take the rest of the dialog's height
            let flat = ui.horizontal(|ui| {
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.add_enabled(self.sprite.is_some(), egui::Button::new("Create flat normal map"))
                        .on_hover_text("Start with a flat normal map, which is saved to the original path")
                        .on_disabled_hover_text("Pick the spritesheet first")
                }).inner
            }).inner;
            if flat.clicked() && let Some(sprite) = &self.sprite {
                self.normal = Some(SpriteFileSelection::open_or_fill(
                    NORMAL_LABEL, self.pending.normal_path.clone(), rs, sprite.size(), FLAT_NORMAL_COLOR,
                ));
            }
        }

        if let Some(error) = &self.error {
            ui.add_space(8.0);
            ui.add(egui::Label::new(egui::RichText::new(format!("⚠ {error}")).color(egui::Color32::YELLOW)).wrap());
        }

        ui.add_space(8.0);
        ui.separator();

        // Buttons, on the right, added right to left so Open is last
        let can_open = self.sprite.is_some() && self.normal.is_some();
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
                if let (Some(sprite), Some(normal)) = (dialog.sprite, dialog.normal) {
                    show_result(Ok(self.finish_open(ui.ctx(), dialog.pending, sprite, normal)));
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
