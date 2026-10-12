use eframe::egui::{ CentralPanel, Frame, Response, Ui, Widget, Image, TextureId, load::SizedTexture, Sense };
use eframe::egui_wgpu::RenderState;
use eframe::wgpu::FilterMode;
use rendering::Renderer;
use controller::Controller;

use crate::app::StudioApp;

/// Widget for the main editor window
pub struct Editor<'a> {
    renderer: &'a mut Renderer,
    render_state: &'a RenderState,
    viewport_texture_id: TextureId,
    controller: &'a mut Controller,
}

impl<'a> Editor<'a> {
    pub fn new(renderer: &'a mut Renderer, render_state: &'a RenderState, viewport_texture_id: TextureId, controller: &'a mut Controller) -> Self {
        Self { renderer, render_state, viewport_texture_id, controller }
    }
}

impl Widget for Editor<'_> {
    fn ui(self, ui: &mut Ui) -> Response {
        let points = ui.available_size();
        let ppp = ui.ctx().pixels_per_point();
        let px = studio_math::Vec2::new((points.x * ppp).max(16.0), (points.y * ppp).max(16.0));

        // Render directly to the EGUI texture
        let view = self.renderer.render(&self.render_state.device, &self.render_state.queue, self.controller, px, ppp);
        self.render_state.renderer.write().update_egui_texture_from_wgpu_texture(
            &self.render_state.device,
            view,
            FilterMode::Nearest,
            self.viewport_texture_id,
        );

        // Display the texture image in as much space as possible
        ui.add(Image::new(SizedTexture::new(self.viewport_texture_id, points)).sense(Sense::click_and_drag()))
    }
}
impl StudioApp {
    /// Render the main editor in the remaining central space. Must be shown after all side panels.
    /// Returns the response for the editor image itself.
    pub(super) fn show_editor(&mut self, ui: &mut Ui) -> Response {
        CentralPanel::default().frame(Frame::NONE).show(ui, |ui| {
            ui.add(Editor::new(
                &mut self.renderer,
                &self.render_state,
                self.viewport_texture_id,
                &mut self.controller,
            ))
        }).inner
    }
}
