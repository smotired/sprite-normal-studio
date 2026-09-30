use eframe::egui::{ Response, Ui, Widget, Image, TextureId, load::SizedTexture, Sense };
use eframe::egui_wgpu::RenderState;
use eframe::wgpu::FilterMode;
use rendering::Renderer;
use controller::ViewportDataUniform;

/// Widget for the main editor window
pub struct Editor<'a> {
    renderer: &'a mut Renderer,
    render_state: &'a RenderState,
    viewport_texture_id: TextureId,
    uniform: ViewportDataUniform,
}

impl<'a> Editor<'a> {
    pub fn new(renderer: &'a mut Renderer, render_state: &'a RenderState, viewport_texture_id: TextureId, uniform: ViewportDataUniform) -> Self {
        Self { renderer, render_state, viewport_texture_id, uniform }
    }
}

impl Widget for Editor<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let points = ui.available_size();
        let ppp = ui.ctx().pixels_per_point();
        let px = vector::V2::new((points.x * ppp).max(16.0), (points.y * ppp).max(16.0));

        // Render directly to the EGUI texture
        let view = self.renderer.render(&self.render_state.device, &self.render_state.queue, self.uniform, px);
        self.render_state.renderer.write().update_egui_texture_from_wgpu_texture(
            &self.render_state.device,
            &view,
            FilterMode::Nearest,
            self.viewport_texture_id,
        );

        // Display the texture image in as much space as possible
        ui.add(Image::new(SizedTexture::new(self.viewport_texture_id, points)).sense(Sense::click_and_drag()))
    }
}