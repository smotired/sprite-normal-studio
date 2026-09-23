use eframe::egui::{
    Response, Ui, Widget,
};
use rendering::{device::RendererDevice};

/// Widget for the main editor
pub struct Editor<'a> {
    device: &'a mut RendererDevice,
}

impl<'a> Editor<'a> {
    pub fn new(device: &'a mut RendererDevice) -> Self {
        Self { device }
    }
}

impl Widget for Editor<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        // TODO: Find size somehow
        let tex = &self.device.render((240, 240), ui.ctx());
        ui.image((tex.id(), tex.size_vec2()))
    }
}