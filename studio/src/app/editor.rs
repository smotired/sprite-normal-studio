use eframe::egui::{
    Response, Ui, Widget,
};
use rendering::Renderer;

/// Widget for the main editor
pub struct Editor<'a> {
    device: &'a mut Renderer,
}

impl<'a> Editor<'a> {
    pub fn new(device: &'a mut Renderer) -> Self {
        Self { device }
    }
}

impl Widget for Editor<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let ctx = ui.ctx();
        let avail = ui.available_size() * ctx.pixels_per_point();
        let size = (avail.x.clamp(1.0, 256.0) as usize, avail.y.clamp(1.0, 256.0) as usize);
        let tex = &self.device.render(size, ctx);
        ui.image((tex.id(), tex.size_vec2()))
    }
}