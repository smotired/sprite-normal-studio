mod generator;
mod interaction;

use wgpu::{Device, Queue};

use crate::generator::Generator;

pub use crate::interaction::input::{Input, Axis};
pub use crate::interaction::viewport::ViewportDataUniform;

/// The Controller manages our actual working pipeline.
pub struct Controller {
    // In charge of generating normal maps when we change the controller
    generator: Generator,

    // If we need to regenerate the normal map on the next render.
    normals_stale: bool,

    // Current state of the camera
    camera: interaction::Camera,

    // Current state of the light
    light: interaction::Light,

    // Current state of the overlay and viewport
    overlay_state: interaction::OverlayState,
}

impl Controller {
    /// Set up the controller and all its child objects
    pub fn new(device: &Device) -> Self {
        Self {
            generator: Generator::new(device),
            normals_stale: false,
            camera: Default::default(),
            light: Default::default(),
            overlay_state: Default::default(),
        }
    }

    /// Runs every frame of the GUI. Handle rerendering as needed.
    /// Returns the status of the viewport as a tuple. Camera, light(s) later.
    #[must_use = "Use return values when rendering the viewport."]
    pub fn update(&mut self, device: &Device, queue: &Queue) -> ViewportDataUniform
    {
        if self.normals_stale {
            self.generator.generate_normals(device, queue);
            self.normals_stale = false;
        }

        self.uniform()
    }
}