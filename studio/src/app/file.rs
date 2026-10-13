pub mod loader;

use std::path::{Path, PathBuf};
use eframe::egui::{Align, Button, Layout, Response, RichText, Ui, Widget};

use loader::{SpriteFileSelection, SpriteFileSelectionDisplay};

/// Event handler type triggered when a sprite file is selected
type FileSelectEventHandler<'a> = Box<dyn FnOnce(PathBuf, (u32, u32)) + 'a>;

/// Widget for a file selector widget
pub struct SpriteFileSelect<'a> {
    /// Method to run when a new file name is selected.
    fn_select: Option<FileSelectEventHandler<'a>>,
    value: &'a mut SpriteFileSelection,
    label: &'a str,
    render_state: &'a eframe::egui_wgpu::RenderState,
}

impl<'a> SpriteFileSelect<'a> {
    /// Create a SpriteFileSelect widget
    pub fn new(value: &'a mut SpriteFileSelection, label: &'a str, render_state: &'a eframe::egui_wgpu::RenderState) -> Self {
        Self {
            fn_select: None,
            value,
            label,
            render_state,
        }
    }

    #[inline]
    /// Set the on_select event for the box
    pub fn on_select<F>(mut self, event: F) -> Self
        where F: FnOnce(PathBuf, (u32, u32)) + 'a
    {
        self.fn_select = Some(Box::new(event));
        self
    }
}

/// Main rendering method
impl Widget for SpriteFileSelect<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        ui.vertical(|ui| {
            if let Some(path) = file_picker_row(ui, self.label, true, Some(self.value.path())) {
                let size = self.value.select(path.clone(), self.render_state);
                if let Some(event) = self.fn_select { event(path, size); }
            }

            // Add the image widget
            ui.add(SpriteFileSelectionDisplay::new(self.value))
        }).response
    }
}

/// A label with a file select button on its right. Returns the PNG file that was picked, if any.
/// The file dialog starts in the folder of the current file, if there is one.
pub fn file_picker_row(ui: &mut Ui, label: &str, enabled: bool, current: Option<&Path>) -> Option<PathBuf> {
    let mut picked = None;
    ui.horizontal(|ui| {
        // Render the label text
        let label = ui.label(label);

        // Put the button on the right as if the label had flexGrow
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            let button = ui.add_enabled(enabled, Button::new("Select File"))
                .labelled_by(label.id);

            // Open file select on click
            if button.clicked() {
                let mut dialog = rfd::FileDialog::new().add_filter("PNG files", &["png"]);
                if let Some(directory) = current.and_then(Path::parent).filter(|dir| dir.is_dir()) {
                    dialog = dialog.set_directory(directory);
                }
                picked = dialog.pick_file();
            }
        });
    });
    picked
}

/// Tallest image previews are shown in dialogs
const DIALOG_PREVIEW_HEIGHT: f32 = 120.0;

/// A file picker row for a dialog that loads the picked image into `slot`. The selection it replaces is added to `retired`.
/// Returns true if a new image was loaded, or an error message if the picked file couldn't be loaded, leaving `slot` unchanged.
pub fn pick_image(
    ui: &mut Ui,
    label: &str,
    enabled: bool,
    slot: &mut Option<SpriteFileSelection>,
    texture_label: &str,
    rs: &eframe::egui_wgpu::RenderState,
    retired: &mut Vec<SpriteFileSelection>,
) -> Result<bool, String> {
    let Some(path) = file_picker_row(ui, label, enabled, slot.as_ref().map(SpriteFileSelection::path)) else { return Ok(false); };
    match SpriteFileSelection::open(texture_label, path.clone(), rs) {
        Ok(selection) => {
            retired.extend(slot.replace(selection));
            Ok(true)
        },
        Err(e) => Err(format!("Could not load {}: {e:#}", path.display())),
    }
}

/// Show a dialog's preview of a picked image and its path, or say nothing is picked
pub fn preview(ui: &mut Ui, selection: Option<&SpriteFileSelection>) {
    match selection {
        Some(selection) => {
            ui.add(SpriteFileSelectionDisplay::new(selection).max_height(DIALOG_PREVIEW_HEIGHT));
            ui.label(RichText::new(selection.path().display().to_string()).weak().small());
        },
        None => { ui.label(RichText::new("None selected").weak()); },
    }
}

/// Generate a normal map filename by appending _normal to the file name before the PNG
pub fn generate_normal_map_filename(sprite_path: &Path) -> PathBuf {
    let stem = sprite_path.file_stem().unwrap_or_default().to_string_lossy();
    sprite_path.with_file_name(format!("{stem}_normal.png"))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// _normal is added to the stem and the directory is kept
    #[test]
    fn normal_map_filename() {
        assert_eq!(
            generate_normal_map_filename(&PathBuf::from("art").join("hero.png")),
            PathBuf::from("art").join("hero_normal.png"),
        );
        assert_eq!(generate_normal_map_filename(Path::new("hero.png")), PathBuf::from("hero_normal.png"));
    }

    /// Only the final extension is replaced
    #[test]
    fn normal_map_filename_with_dots() {
        assert_eq!(generate_normal_map_filename(Path::new("hero.idle.png")), PathBuf::from("hero.idle_normal.png"));
    }
}