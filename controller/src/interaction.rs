use studio_math::Vec2;

mod camera;
mod light;
mod overlay;
pub mod viewport;
pub mod action;
pub mod tools;
pub mod history;

pub use camera::Camera;
pub use light::Light;
pub use overlay::OverlayState;

use crate::Controller;

impl Controller {
    // Camera methods
    pub fn screen_to_world(&self, screen: Vec2, viewport_size: Vec2) -> Vec2 {
        self.camera.screen_to_world(screen, viewport_size)
    }

    /// Highlight a rect in world space like a selection box, or stop highlighting with None
    pub fn set_highlight(&mut self, rect: Option<crate::Bounds>) {
        self.highlight = rect;
    }

    // Get output
    pub fn uniform(&self) -> viewport::ViewportDataUniform {
        let mut uniform = viewport::ViewportDataUniform::new(
            &self.light,
            &self.camera,
            &self.overlay_state,
            self.cursor_pos,
            &self.objects,
            self.tool.as_ref(),
            self.tool.transform_box(&self.objects),
        );
        uniform.sprite_size = Vec2::new(self.sprite_size.0 as f32, self.sprite_size.1 as f32);

        // The tool's own selection box takes priority over a highlight
        if let Some((min, max)) = self.highlight && self.tool.selection_box().is_none() {
            uniform.selection_box_start = min;
            uniform.selection_box_end = max;
        }
        uniform
    }
}