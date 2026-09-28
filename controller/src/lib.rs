mod generator;

use wgpu::{Device, Queue, Texture};

use crate::generator::Generator;

/// The Controller manages our actual working pipeline.
pub struct Controller {
    // In charge of generating normal maps when we change the controller
    generator: Generator,

    // If we need to regenerate the normal map on the next render.
    normals_stale: bool,
}

impl Controller {
    /// Set up the controller and all its child objects
    pub fn new(device: &Device) -> Self {
        Self {
            generator: Generator::new(device),
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
    }

    /// Runs every frame of the GUI. Handle rerendering as needed.
    pub fn update(&mut self, device: &Device, queue: &Queue)
    {
        if self.normals_stale {
            self.generator.generate_normals(device, queue);
            self.normals_stale = false;
        }
    }
}