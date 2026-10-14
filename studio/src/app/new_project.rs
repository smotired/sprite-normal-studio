use std::path::Path;

use controller::Controller;
use eframe::egui;

use crate::app::file::{self, loader::SpriteFileSelection};
use crate::app::workspace::{build_workspace_file, EXTENSION, NORMAL_LABEL, SPRITE_LABEL};
use crate::app::{Project, StudioApp};

/// Width of the dialog's contents
pub(super) const WIDTH: f32 = 360.0;

/// Characters that can't be in the project's name, because it becomes the workspace file's name
const INVALID_NAME_CHARS: &[char] = &['<', '>', ':', '"', '/', '\\', '|', '?', '*'];

/// State of the New Project dialog. The images are loaded as soon as they are picked, so they can be previewed,
/// and are moved into the new project when it is created.
#[derive(Default)]
pub(super) struct NewProjectDialog {
    name: String,
    sprite: Option<SpriteFileSelection>,
    normal: Option<SpriteFileSelection>,
    error: Option<String>,
}

/// What the user did with the dialog this frame
enum DialogAction {
    None,
    Create,
    Cancel,
}

impl NewProjectDialog {
    /// Render the dialog's contents. Selections replaced by new picks are added to `retired`.
    fn ui(&mut self, ui: &mut egui::Ui, rs: &eframe::egui_wgpu::RenderState, retired: &mut Vec<SpriteFileSelection>) -> DialogAction {
        ui.set_width(WIDTH);
        ui.heading("New Project");
        ui.add_space(8.0);

        // Name
        ui.horizontal(|ui| {
            let label = ui.label("Name");
            ui.add(egui::TextEdit::singleline(&mut self.name).hint_text("Project name").desired_width(f32::INFINITY))
                .labelled_by(label.id);
        });
        ui.add_space(8.0);

        // Spritesheet, which also names the project if it has no name yet
        match file::pick_image(ui, "Spritesheet", true, &mut self.sprite, SPRITE_LABEL, rs, retired) {
            Ok(true) => {
                let sprite = self.sprite.as_ref().unwrap();
                if self.name.trim().is_empty() {
                    self.name = sprite.path().file_stem().unwrap_or_default().to_string_lossy().into_owned();
                }
                self.error = None;
            },
            Ok(false) => {},
            Err(e) => self.error = Some(e),
        }
        file::preview(ui, self.sprite.as_ref(), "None selected");
        ui.add_space(8.0);

        // Base normal map, which is optional. Without one, the normal map starts flat.
        match file::pick_image(ui, "Base Normal Map", true, &mut self.normal, NORMAL_LABEL, rs, retired) {
            Ok(true) => self.error = None,
            Ok(false) => {},
            Err(e) => self.error = Some(e),
        }
        file::preview(ui, self.normal.as_ref(), file::FLAT_NORMAL_TEXT);
        if self.normal.is_some() && file::right_aligned_button(ui, "Use flat normal map").clicked() {
            retired.extend(self.normal.take());
        }

        if let Some(error) = &self.error {
            ui.add_space(8.0);
            ui.add(egui::Label::new(egui::RichText::new(format!("⚠ {error}")).color(egui::Color32::YELLOW)).wrap());
        }

        ui.add_space(8.0);
        ui.separator();

        // Buttons, on the right, added right to left so Create is last
        let can_create = !self.name.trim().is_empty() && self.sprite.is_some();
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            let create = ui.add_enabled(can_create, egui::Button::new("Create"));
            let cancel = ui.button("Cancel");
            if create.clicked() {
                DialogAction::Create
            } else if cancel.clicked() {
                DialogAction::Cancel
            } else {
                DialogAction::None
            }
        }).inner
    }
}

impl StudioApp {
    /// Render the New Project dialog, if it is open, and create the project when asked
    pub(super) fn show_new_project_dialog(&mut self, ui: &mut egui::Ui) {
        let Some(dialog) = &mut self.new_project_dialog else { return; };

        let modal = egui::Modal::new(egui::Id::new("new_project_dialog")).show(ui.ctx(), |ui| {
            dialog.ui(ui, &self.render_state, &mut self.retired_selections)
        });

        match modal.inner {
            DialogAction::Create => self.create_project(ui.ctx()),
            DialogAction::Cancel => self.close_new_project_dialog(),
            DialogAction::None if modal.should_close() => self.close_new_project_dialog(),
            DialogAction::None => {},
        }
    }

    /// Close the New Project dialog, freeing any images it loaded
    fn close_new_project_dialog(&mut self) {
        if let Some(dialog) = self.new_project_dialog.take() {
            self.retired_selections.extend(dialog.sprite);
            self.retired_selections.extend(dialog.normal);
        }
    }

    /// Create the project from the New Project dialog in a new tab. It isn't saved until the user saves it.
    fn create_project(&mut self, ctx: &egui::Context) {
        let Some(dialog) = &mut self.new_project_dialog else { return; };
        let name = dialog.name.trim();
        if name.contains(INVALID_NAME_CHARS) {
            dialog.error = Some(format!("The name can't contain any of {}", INVALID_NAME_CHARS.iter().collect::<String>()));
            return;
        }
        let Some(dialog) = self.new_project_dialog.take() else { return; };
        let Some(sprite) = dialog.sprite else {
            self.retired_selections.extend(dialog.normal);
            return;
        };
        let normal = dialog.normal;

        // The workspace file goes next to the spritesheet by default
        let folder = sprite.path().parent().unwrap_or(Path::new(""));
        let path = folder.join(format!("{}.{EXTENSION}", dialog.name.trim()));

        let mut controller = Controller::new(&self.render_state.device);
        controller.new_scene();
        let clean = build_workspace_file(&sprite, normal.as_ref(), None, controller.save_scene(), &path);
        self.add_project(ctx, Project { sprite, normal, path, saved: false, export_path: None, clean, dirty: true, controller });

    }
}
