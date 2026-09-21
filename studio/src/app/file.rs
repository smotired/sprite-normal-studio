use std::path::PathBuf;
use eframe::egui::{
    Align, Layout, Response, Ui, Widget,
};

/// Event handler type triggered when a sprite file is selected
type FileSelectEventHandler<'a> = Box<dyn FnOnce(&PathBuf) + 'a>;

/// Widget for a file selector widget
pub struct SpriteFileSelect<'a> {
    /// Method to run when a new file name is selected.
    fn_select: Option<FileSelectEventHandler<'a>>,
    value: &'a mut Option<PathBuf>,
    label: &'a str,
}

impl<'a> SpriteFileSelect<'a> {
    /// Create a SpriteFileSelect widget
    pub fn new(value: &'a mut Option<PathBuf>, label: &'a str) -> Self {
        Self {
            fn_select: None,
            value,
            label,
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
                        *self.value = Some(path);
                    }
                });
            });

            // Add the path label
            ui.label(if let Some(path) = self.value {
                path.as_path().file_name().unwrap().to_str().unwrap().to_string()
            } else {
                format!("No {} selected.", String::from(self.label).to_lowercase())
            });
        }).response
    }
}