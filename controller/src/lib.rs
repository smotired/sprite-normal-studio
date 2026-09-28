mod generator;
mod camera;
mod input;

use wgpu::{Device, Queue, Texture};

use crate::generator::Generator;
use crate::camera::Camera;
pub use crate::input::{Input, Axis};

pub type ViewportState = (Camera, ());

/// The Controller manages our actual working pipeline.
pub struct Controller {
    // In charge of generating normal maps when we change the controller
    generator: Generator,

    // Current state of the camera
    camera: Camera,

    // If we need to regenerate the normal map on the next render.
    normals_stale: bool,
}

impl Controller {
    /// Set up the controller and all its child objects
    pub fn new(device: &Device) -> Self {
        Self {
            generator: Generator::new(device),
            camera: Camera::new([8.0, 8.0], 1), // center of starting image. when image is loaded, should recenter at image center and scale 0.
            normals_stale: false,
        }
    }
    
    /// Get a reference to the working normal map texture
    pub fn output(&self) -> &Texture { self.generator.output() }
    
    /// When sprite or normal map are loaded from files, update child objects.
    pub fn set_inputs(&mut self, device: &Device, sprite: &Texture, normal: &Texture)
    {
        // Regenerate the normal map's output texture
        self.generator.set_inputs(device, sprite, normal);
        self.normals_stale = true;

        // Recenter the camera
        let size = sprite.size();
        self.camera = Camera::new([size.width as f32 * 0.5, size.height as f32 * 0.5], 1);
    }

    // Convert viewport pixel position to world/spritesheet space.
    pub fn screen_to_world(&self, screen: (f32, f32), viewport_size: (f32, f32)) -> (f32, f32) {
        self.camera.screen_to_world(screen, viewport_size)
    }

    /// Runs every frame of the GUI. Handle rerendering as needed.
    /// Returns the status of the viewport as a tuple. Camera, light(s) later.
    #[must_use = "Use return values when rendering the viewport."]
    pub fn update(&mut self, device: &Device, queue: &Queue) -> ViewportState
    {
        if self.normals_stale {
            self.generator.generate_normals(device, queue);
            self.normals_stale = false;
        }

        (self.camera, ())
    }
}