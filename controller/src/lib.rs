mod generator;
mod interaction;

use vector::Vec2;
use wgpu::{Device, Queue, Texture};

use crate::generator::Generator;
use crate::interaction::Interaction;

pub use crate::interaction::input::{Input, Axis};
pub use crate::interaction::viewport::ViewportDataUniform;

/// The Controller manages our actual working pipeline.
pub struct Controller {
    // In charge of generating normal maps when we change the controller
    generator: Generator,

    // If we need to regenerate the normal map on the next render.
    normals_stale: bool,

    // Handles our current interaction
    interaction: Interaction,
}

impl Controller {
    /// Set up the controller and all its child objects
    pub fn new(device: &Device) -> Self {
        Self {
            generator: Generator::new(device),
            normals_stale: false,
            interaction: Default::default(),
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

        // Recenter the camera and the light by adding a fake input (maybe not a good idea but icbatgetslftmoas </3)
        self.interaction.handle_input(Input::Recenter, Vec2::from(self.output().size()));
    }

    // Convert viewport pixel position to world/spritesheet space.
    pub fn screen_to_world(&self, screen: Vec2, viewport_size: Vec2) -> Vec2 {
        self.interaction.screen_to_world(screen, viewport_size)
    }
    
    // Forward input interactions
    pub fn handle_input(&mut self, input: Input) {
        self.interaction.handle_input(input, Vec2::from(self.output().size()));
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

        self.interaction.output()
    }
}