mod generator;
mod interaction;
mod objects;
mod assignment;

use wgpu::{Buffer, Device, Queue, Texture};

use crate::generator::Generator;
use crate::assignment::ZoneAssigner;

pub use crate::interaction::input::{Input, InputModifiers, Axis};
use crate::interaction::tools::{EditorTool, EditorToolZone};
pub use crate::interaction::viewport::ViewportDataUniform;
pub use crate::interaction::tools::EditorToolKind;
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
    cursor_pos: studio_math::Vec2,

    /// The tool we currently have selected
    tool: Box<dyn EditorTool>,

    /// Selection buffer which gets contents from the selected tool
    selection_buffer: Buffer,
}

impl Controller {
    /// Set up the controller and all its child objects
    pub fn new(device: &Device) -> Self {
        let mut objects = ObjectBuffers::new(device);
        let objects_buffers = objects.get_buffers(device);
    
        let selection_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Points Selection Buffer"),
            size: 8192, // 65536 max points packed into 8 values per byte
            mapped_at_creation: false,
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_DST,
        });

        Self {
            assigner: ZoneAssigner::new(device, objects_buffers.clone()),
            generator: Generator::new(device, objects_buffers.clone()),
            normals_stale: false,
            camera: Default::default(),
            light: Default::default(),
            overlay_state: Default::default(),
            objects,
            cursor_pos: Default::default(),
            tool: EditorToolZone::init(),
            selection_buffer,
        }
    }
    
    /// When sprite or normal map are loaded from files, update child objects.
    pub fn set_inputs(&mut self, device: &Device, sprite: &Texture, normal: &Texture)
    {
        // Reset selection
        self.tool = EditorToolZone::init();

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
            self.assigner.assign_zones(device, queue, objects_buffers.clone(), self.objects.object_counts());
            self.generator.generate_normals(device, queue, objects_buffers.clone());
            self.normals_stale = false;
        }
    }

    pub fn selection_buffer(&self) -> Buffer { self.selection_buffer.clone() }
}