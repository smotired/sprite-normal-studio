mod generator;
mod interaction;
mod objects;
mod assignment;

use wgpu::{Device, Queue, Texture};

use crate::generator::Generator;
use crate::assignment::ZoneAssigner;

pub use crate::interaction::input::{Input, Axis};
pub use crate::interaction::viewport::ViewportDataUniform;
pub use crate::interaction::tools::EditorTool;
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

    /// The current cursor position in world space.
    cursor_pos: vector::Vec2,

    /// The tool we currently have selected
    tool: EditorTool,

    /// The ID of the zone we currently have selected
    selected_zone: Option<u16>,

    /// The ID of the point we currently have selected
    selected_point: Option<u16>,

    /// Whether we are currently dragging a handle of the selected point. If true, we are dragging the right handle.
    dragging_handle: Option<bool>,
}

impl Controller {
    /// Set up the controller and all its child objects
    pub fn new(device: &Device) -> Self {
        let mut objects = ObjectBuffers::new(device);
        let objects_buffers = objects.get_buffers(device);

        Self {
            assigner: ZoneAssigner::new(device, objects_buffers.clone()),
            generator: Generator::new(device, objects_buffers.clone()),
            normals_stale: false,
            camera: Default::default(),
            light: Default::default(),
            overlay_state: Default::default(),
            objects,
            cursor_pos: Default::default(),
            tool: EditorTool::Zone,
            selected_zone: None,
            selected_point: None,
            dragging_handle: None,
        }
    }
    
    /// When sprite or normal map are loaded from files, update child objects.
    pub fn set_inputs(&mut self, device: &Device, sprite: &Texture, normal: &Texture)
    {
        // Reset selection
        self.selected_zone = None;
        self.selected_point = None;
        self.tool = EditorTool::Zone;

        // Regenerate the normal map's output texture
        let object_buffers = self.objects.get_buffers(device);
        self.assigner.set_inputs(device, sprite, object_buffers.clone());
        self.generator.set_inputs(device, normal, self.assigner.output(), object_buffers.clone());
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
            let zone_ignore = if let EditorTool::Pen = self.tool { self.selected_zone } else { None }; // If we are creating a zone, ignore it, otherwise don't ignore.
            self.assigner.assign_zones(device, queue, objects_buffers.clone(), zone_ignore);
            self.generator.generate_normals(device, queue, objects_buffers.clone());
            self.normals_stale = false;
        }
    }
}