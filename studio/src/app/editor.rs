use eframe::egui::{ Response, Ui, Widget, Image, load::SizedTexture };
use eframe::egui_wgpu::RenderState;
use rendering::Renderer;

/// Widget for the main editor
pub struct Editor<'a> {
    renderer: &'a mut Renderer,
    render_state: &'a RenderState,
}

impl<'a> Editor<'a> {
    pub fn new(renderer: &'a mut Renderer, render_state: &'a RenderState) -> Self {
        Self { renderer, render_state }
    }
}

impl Widget for Editor<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let points = ui.available_size();
        let ppp = ui.ctx().pixels_per_point();
        let px = (((points.x * ppp) as usize).max(16), ((points.y * ppp) as usize).max(16));

        let id = self.renderer.render(self.render_state, px);
        ui.add(Image::new(SizedTexture::new(id, points)))
    }
}