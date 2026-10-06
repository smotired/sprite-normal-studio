use studio_math::Vec2;

mod camera;
mod light;
mod overlay;
pub mod viewport;
pub mod input;
pub mod tools;

pub use camera::Camera;
pub use light::Light;
pub use overlay::OverlayState;

use crate::Controller;

impl Controller {
    // Camera methods
    pub fn screen_to_world(&self, screen: Vec2, viewport_size: Vec2) -> Vec2 {
        self.camera.screen_to_world(screen, viewport_size)
    }

    // Get output
    pub fn uniform(&self) -> viewport::ViewportDataUniform {
        viewport::ViewportDataUniform::new(
            &self.light,
            &self.camera,
            &self.overlay_state,
            self.cursor_pos,
            self.objects.object_counts(),
            self.tool.as_ref(),
        )
    }
}