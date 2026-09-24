use eframe::egui::{
    self, Response, Ui, Widget,
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
        let size = (avail.x as usize, avail.y as usize);
        let (tex, image_size) = self.device.render(size, ctx);
        let image_size = image_size as f32;

        ui.add(
            egui::Image::new(egui::load::SizedTexture::new(tex.id(), avail))
                .uv(egui::Rect::from_min_max(
                    egui::Pos2::ZERO,
                    egui::pos2(avail.x / image_size, avail.y / image_size),
        )))
    }
}