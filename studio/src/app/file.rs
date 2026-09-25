pub mod loader;

use std::path::PathBuf;
use eframe::egui::{Align, Layout, Response, Ui, Widget};

use loader::SpriteFileSelection;

use crate::app::file::loader::SpriteFileSelectionDisplay;

/// Event handler type triggered when a sprite file is selected
type FileSelectEventHandler<'a> = Box<dyn FnOnce(&PathBuf) + 'a>;

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
        where F: FnOnce(&PathBuf) + 'a
    {
        self.fn_select = Some(Box::new(event));
        self
    }
}

/// Main rendering method
impl Widget for SpriteFileSelect<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        ui.vertical(|ui| {
            ui.horizontal(|ui| {
                // Render the label text
                let label = ui.label(self.label);

                // Put the button on the right as if the label had flexGrow
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    // Add the file select button
                    let button = ui.button("Select File")
                        .labelled_by(label.id);

                    // Open file select on click
                    if button.clicked()
                        && let Some(path) = rfd::FileDialog::new()
                            .add_filter("PNG files", &["png"])
                            .set_directory("/")
                            .pick_file()
                    {
                        if let Some(event) = self.fn_select { event(&path); }
                        self.value.select(path, self.render_state);
                    }
                });
            });

            // Add the image widget or default label
            ui.add(SpriteFileSelectionDisplay::new(self.value))
        }).response
    }
}