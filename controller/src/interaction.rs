mod camera;
mod light;
mod overlay;
pub mod viewport;
pub mod input;

pub struct Interaction {
    // Current state of the camera
    camera: camera::Camera,

    // Current state of the light
    light: light::Light,

    // Current state of the overlay and viewport
    overlay_state: overlay::OverlayState,
}

impl Interaction {
    // Camera methods
    pub fn screen_to_world(&self, screen: (f32, f32), viewport_size: (f32, f32)) -> (f32, f32) {
        self.camera.screen_to_world(screen, viewport_size)
    }

    // Get output
    pub fn output(&self) -> viewport::ViewportDataUniform {
        viewport::ViewportDataUniform::new(&self.light, &self.camera, &self.overlay_state)
    }
}

impl Default for Interaction {
    fn default() -> Self {
        Self {
            camera: Default::default(),
            light: Default::default(),
            overlay_state: Default::default(),
        }
    }
}