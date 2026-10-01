mod generator;
mod interaction;
mod objects;
mod assignment;

use wgpu::{Device, Queue, Texture};

use crate::generator::Generator;
use crate::assignment::ZoneAssigner;

pub use crate::interaction::input::{Input, Axis};
pub use crate::interaction::viewport::ViewportDataUniform;
pub use crate::objects::{ObjectBuffers, BufferStates};

/// The Controller manages our actual working pipeline.
pub struct Controller {
    // In charge of assigning pixels to zones
    assigner: ZoneAssigner,

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

    /// The points, zones, and shapes.
    objects: objects::ObjectBuffers,
}

impl Controller {
    /// Set up the controller and all its child objects
    pub fn new(device: &Device) -> Self {
        let mut objects = ObjectBuffers::new(device);
        let objects_buffers = objects.get_buffers(device);

        Self {
            assigner: ZoneAssigner::new(device, objects_buffers),
            generator: Generator::new(device),
            normals_stale: false,
            camera: Default::default(),
            light: Default::default(),
            overlay_state: Default::default(),
            objects,
        }
    }
    
    /// When sprite or normal map are loaded from files, update child objects.
    pub fn set_inputs(&mut self, device: &Device, sprite: &Texture, normal: &Texture)
    {
        // Regenerate the normal map's output texture
        self.assigner.set_inputs(device, sprite, self.objects.get_buffers(device));
        self.generator.set_inputs(device, normal, self.assigner.output());
        self.normals_stale = true;

        // Recenter the camera and the light by adding a fake input (maybe not a good idea but icbatgetslftmoas </3)
        self.handle_input(Input::Recenter);
    }

    /// Runs every frame of the GUI. Handle rerendering as needed.
    pub fn update(&mut self, device: &Device, queue: &Queue) {
        // Write shapes. Should be fine to do this here because we are writing to the same buffers.
        let objects_buffers = self.objects.get_buffers(device);
        self.write_object_buffers(queue);

        if self.normals_stale {
            let zone_ignore = None; // TODO
            self.assigner.assign_zones(device, queue, objects_buffers, zone_ignore);
            self.generator.generate_normals(device, queue);
            self.normals_stale = false;
        }
    }
}