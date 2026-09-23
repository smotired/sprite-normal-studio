use eframe::egui::{
    Response, Ui, Widget,
};
use rendering::get_normal_image;

/// Widget for the main editor
pub struct Editor {

}

impl Editor {
    pub fn new() -> Self {
        Self { }
    }
}

impl Widget for Editor {
    fn ui(self, ui: &mut Ui) -> Response {
        get_normal_image();
        ui.label("vulkan image goes here")
    }
}