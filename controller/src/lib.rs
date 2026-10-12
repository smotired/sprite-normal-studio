mod generator;
mod interaction;
mod objects;
mod assignment;

use studio_math::Vec2;
use studio_math::bounds::{bounds_clamp, bounds_correct};
use wgpu::{Buffer, Device, Queue, Texture};

use crate::generator::Generator;
use crate::assignment::ZoneAssigner;

pub use crate::interaction::action::{Action, InputModifiers, Axis};
use crate::interaction::tools::{EditorTool, EditorToolZone};
use crate::interaction::history::History;
pub use crate::interaction::viewport::ViewportDataUniform;
pub use crate::interaction::tools::{EditorToolKind, EditorChange, SpecialAction, SpecialQuery, SpecialValue};
pub use crate::objects::{ObjectBuffers, ObjectDiff, BufferStates, ControlPointMode, Bounds};
use crate::objects::merge_bounds;

/// The Controller manages our actual working pipeline.
pub struct Controller {
    // In charge of assigning pixels to zones
    assigner: ZoneAssigner,

    // In charge of generating normal maps when we change the controller
    generator: Generator,

    // Bounding box of the region of the normal map to regenerate on the next render, if any.
    normals_stale: Option<Bounds>,

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

    /// The error from the latest tool action, if it failed
    tool_error: Option<String>,

    /// The changes that can be undone and redone
    history: History,
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
            normals_stale: None,
            camera: Default::default(),
            light: Default::default(),
            overlay_state: Default::default(),
            objects,
            cursor_pos: Default::default(),
            tool: EditorToolZone::init(),
            selection_buffer,
            tool_error: None,
            history: History::default(),
        }
    }
    
    /// When sprite or normal map are loaded from files, update child objects.
    pub fn set_inputs(&mut self, device: &Device, sprite: &Texture, normal: &Texture)
    {
        // Reset selection, finishing any changes in progress so they can still be undone
        self.history.close(&self.objects, self.tool.as_ref());
        self.tool = EditorToolZone::init();

        // Regenerate the normal map's output texture
        let object_buffers = self.objects.get_buffers(device);
        self.assigner.set_inputs(device, sprite, object_buffers.clone());
        self.generator.set_inputs(device, normal, self.assigner.output(), object_buffers.clone());
        self.normals_stale = Some((Vec2::ZERO, self.assigner.size()));

        // Recenter the camera and the light by adding a fake input (maybe not a good idea but icbatgetslftmoas </3)
        self.handle_action(Action::Recenter);
    }

    /// Runs every frame of the GUI. Handle rerendering as needed.
    pub fn update(&mut self, device: &Device, queue: &Queue) {
        // Write shapes. Should be fine to do this here because we are writing to the same buffers.
        let objects_buffers = self.objects.get_buffers(device);
        self.write_object_buffers(queue);

        let stale = self.normals_stale.take();
        if let Some(stale) = self.correct_stale_normals(stale) {
            self.assigner.assign_zones(device, queue, objects_buffers.clone(), self.objects.object_counts(), stale);
            self.generator.generate_normals(device, queue, objects_buffers.clone(), stale);
        }
    }

    /// Add a region to the part of the normal map to regenerate on the next render
    fn mark_normals_stale(&mut self, bounds: Option<Bounds>) {
        self.normals_stale = merge_bounds(self.normals_stale, bounds);
    }

    /// Clamp a stale region to the texture, and correct it to the top left and bottom right corners of the pixels it touches.
    /// Returns None if there is no region or none of it is in the texture.
    fn correct_stale_normals(&self, stale: Option<Bounds>) -> Option<Bounds> {
        correct_stale_bounds(stale, self.assigner.size())
    }

    pub fn selection_buffer(&self) -> Buffer { self.selection_buffer.clone() }
}

/// Clamp a stale region to a texture of the given size, and correct it to the top left and bottom right corners of the pixels it touches.
/// Pixel p covers p..p+1, so the region is rounded outwards to whole pixels.
/// Returns None if there is no region or none of it is in the texture.
fn correct_stale_bounds(stale: Option<Bounds>, size: Vec2) -> Option<Bounds> {
    let (min, max) = stale?;
    let (min, max) = bounds_correct(min, max);
    let (min, max) = bounds_clamp(min.floor(), max.ceil(), Vec2::ZERO, size);
    if max.x <= min.x || max.y <= min.y { return None; }
    Some((min, max))
}

#[cfg(test)]
mod tests {
    use super::*;

    const SIZE: Vec2 = Vec2 { x: 100.0, y: 50.0 };

    /// No region stays no region
    #[test]
    fn stale_none() {
        assert_eq!(correct_stale_bounds(None, SIZE), None);
    }

    /// Flipped corners become the top left and bottom right
    #[test]
    fn stale_flipped() {
        let stale = Some((Vec2::new(30.0, 5.0), Vec2::new(10.0, 20.0)));
        assert_eq!(correct_stale_bounds(stale, SIZE), Some((Vec2::new(10.0, 5.0), Vec2::new(30.0, 20.0))));
    }

    /// Fractional corners are rounded out to cover every pixel they touch
    #[test]
    fn stale_rounds_out() {
        let stale = Some((Vec2::new(10.2, 5.7), Vec2::new(29.1, 20.5)));
        assert_eq!(correct_stale_bounds(stale, SIZE), Some((Vec2::new(10.0, 5.0), Vec2::new(30.0, 21.0))));
    }

    /// Regions partly outside the texture are clamped to it
    #[test]
    fn stale_clamped() {
        let stale = Some((Vec2::new(-20.0, 10.0), Vec2::new(150.0, 80.0)));
        assert_eq!(correct_stale_bounds(stale, SIZE), Some((Vec2::new(0.0, 10.0), Vec2::new(100.0, 50.0))));
    }

    /// Regions outside the texture, or with no area, are nothing
    #[test]
    fn stale_outside() {
        assert_eq!(correct_stale_bounds(Some((Vec2::new(-20.0, 10.0), Vec2::new(-5.0, 20.0))), SIZE), None);
        assert_eq!(correct_stale_bounds(Some((Vec2::new(120.0, 10.0), Vec2::new(150.0, 20.0))), SIZE), None);
        assert_eq!(correct_stale_bounds(Some((Vec2::new(10.0, 10.0), Vec2::new(10.0, 20.0))), SIZE), None);
    }
}
