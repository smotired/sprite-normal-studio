mod generator;
mod interaction;
mod objects;
mod assignment;
mod save;
mod status;

use studio_math::Vec2;
use studio_math::bounds::{bounds_clamp, bounds_correct};
use wgpu::{Device, Queue, Texture};

use crate::generator::Generator;
use crate::assignment::ZoneAssigner;

pub use crate::interaction::action::{Action, InputModifiers, Axis};
use crate::interaction::tools::{EditorTool, EditorToolZone};
use crate::interaction::history::History;
pub use crate::interaction::viewport::ViewportDataUniform;
pub use crate::interaction::tools::{EditorToolKind, EditorChange, PastePlacement, SpecialAction, SpecialQuery, SpecialValue};
pub use crate::objects::{ObjectBuffers, ObjectDiff, BufferStates, ControlPointMode, Bounds, SavedObjects, LoadedObjects};
pub use crate::save::SavedScene;
pub use crate::status::Status;
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

    /// Texture the selected tool's selection is packed into for the viewport
    selection: Texture,

    /// What the status bar shows: the result of the latest tool action, or feedback from the app
    status: Status,

    /// The changes that can be undone and redone
    history: History,
}

impl Controller {
    /// Set up the controller and all its child objects
    pub fn new(device: &Device) -> Self {
        let mut objects = ObjectBuffers::new(device);
        let objects_buffers = objects.get_buffers(device);
    
        // A texture rather than a storage buffer, since only 4 storage buffers can be bound at once
        let selection = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Selection texture"),
            size: wgpu::Extent3d { width: objects::SELECTION_WIDTH, height: objects::SELECTION_HEIGHT, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R32Uint,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
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
            selection,
            status: Status::Ok,
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

    /// Get the texture the selection is packed into for the viewport. See pack_selection in buffers.rs.
    pub fn selection_texture(&self) -> Texture { self.selection.clone() }
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

/// Parse and validate a shader, and check it fits in the device's limits, panicking with its errors. Nothing else checks shaders without a GPU.
#[cfg(test)]
pub(crate) fn validate_wgsl(source: &str) {
    let module = naga::front::wgsl::parse_str(source).unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
    naga::valid::Validator::new(naga::valid::ValidationFlags::all(), naga::valid::Capabilities::default())
        .validate(&module)
        .unwrap_or_else(|e| panic!("{}", e.emit_to_string(source)));
    check_binding_limits(&module);
}

/// Panic if a shader binds more storage buffers or storage textures than the device allows.
/// The app asks for wgpu's downlevel limits, so it runs on as many devices as possible.
#[cfg(test)]
pub(crate) fn check_binding_limits(module: &naga::Module) {
    let limits = wgpu::Limits::downlevel_defaults();
    let storage_buffers = module.global_variables.iter()
        .filter(|(_, global)| matches!(global.space, naga::AddressSpace::Storage { .. }))
        .count();
    let storage_textures = module.global_variables.iter()
        .filter(|(_, global)| matches!(module.types[global.ty].inner, naga::TypeInner::Image { class: naga::ImageClass::Storage { .. }, .. }))
        .count();
    assert!(storage_buffers <= limits.max_storage_buffers_per_shader_stage as usize,
        "{storage_buffers} storage buffers, but only {} are allowed", limits.max_storage_buffers_per_shader_stage);
    assert!(storage_textures <= limits.max_storage_textures_per_shader_stage as usize,
        "{storage_textures} storage textures, but only {} are allowed", limits.max_storage_textures_per_shader_stage);
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
