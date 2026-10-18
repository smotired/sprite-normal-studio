use serde::{Deserialize, Serialize};
use studio_math::Vec2;

use crate::{Animation, Controller, Status};
use crate::interaction::history::History;
use crate::interaction::tools::{EditorTool, EditorToolZone};
use crate::objects::{LoadedObjects, SavedObjects};

/// Everything in the controller that is saved to a workspace file: the objects, the sprite size and the animations,
/// but not the light, the camera, the view or the grid size.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedScene {
    #[serde(flatten)]
    pub objects: SavedObjects,

    /// Width and height of each sprite the spritesheet is sliced into, or null if the whole spritesheet is one sprite
    #[serde(default)]
    pub slice_size: Option<(u32, u32)>,

    /// The animations, in the order they are listed
    #[serde(default)]
    pub animations: Vec<Animation>,
}

impl Controller {
    /// Get everything that should be saved to a workspace file.
    /// A path the pen is still creating isn't a finished zone, so it's left out. It's always the last zone.
    pub fn save_scene(&self) -> SavedScene {
        let slice_size = (self.sprite_size != self.sheet_size()).then_some(self.sprite_size);
        let objects = match self.objects.zone_count().checked_sub(1) {
            // Copying the other zones drops the creating path's points from any sibling groups it branched into
            Some(creating) if self.tool.creating_path().is_some() => self.objects.copy_zones(&(0..creating).collect::<Vec<_>>())
                .expect("every zone before the last exists"),
            _ => self.objects.save(),
        };
        SavedScene { objects, slice_size, animations: self.animations.clone() }
    }

    /// Replace the objects, sprite size and animations with ones loaded from a workspace file, and reset the light.
    /// The objects should already be validated, so this can't fail. Should be called before set_inputs, so the light is centered on the spritesheet
    /// and the sprite size is fit to it.
    pub fn load_scene(&mut self, objects: LoadedObjects, slice_size: Option<(u32, u32)>, animations: Vec<Animation>) {
        self.objects.load(objects);
        self.light = Default::default();
        self.sprite_size = slice_size.unwrap_or((0, 0));
        self.animations = animations;

        // Nothing is selected in the new objects, old errors don't apply to them, and old changes can't be undone into them
        self.tool = EditorToolZone::init();
        self.tool_dragging = false;
        self.revision += 1;
        self.status = Status::Ok;
        self.history = History::default();

        // Regenerate the whole normal map
        self.normals_stale = Some((Vec2::ZERO, self.assigner.size()));
    }

    /// Replace the objects and light with an empty scene for a new project.
    /// Should be called before set_inputs, so the new light is centered on the new spritesheet.
    pub fn new_scene(&mut self) {
        let empty = SavedObjects::default().validate().expect("an empty scene is always valid");
        self.objects.load(empty);
        self.light = Default::default();
        self.sprite_size = (0, 0);
        self.animations = Vec::new();

        self.tool = EditorToolZone::init();
        self.tool_dragging = false;
        self.revision += 1;
        self.status = Status::Ok;
        self.history = History::default();
        self.normals_stale = Some((Vec2::ZERO, self.assigner.size()));
    }
}

#[cfg(test)]
mod tests {
    use studio_math::Vec2;

    use crate::{Action, Controller, EditorToolKind, InputModifiers};
    use crate::objects::test_utils::add_square;

    /// A controller on a device with no GPU behind it, with a square at (0, 0) to (10, 10)
    fn controller() -> Controller {
        let (device, _) = wgpu::Device::noop(&Default::default());
        let mut controller = Controller::new(&device);
        add_square(&mut controller.objects, 10.0);
        controller
    }

    /// Click with the pen at each position
    fn pen_clicks(controller: &mut Controller, positions: &[(f32, f32)]) {
        controller.handle_action(Action::ToolSelected(EditorToolKind::Pen));
        for &position in positions {
            controller.handle_action(Action::MouseClicked(Vec2::from(position), InputModifiers::none()));
        }
    }

    /// A path the pen is still creating isn't saved, and what is saved loads
    #[test]
    fn creating_path_not_saved() {
        let mut controller = controller();
        let before = controller.save_scene();
        pen_clicks(&mut controller, &[(50.0, 50.0), (60.0, 50.0), (60.0, 60.0)]);
        assert!(controller.tool.creating_path().is_some());
        assert_eq!(controller.objects.zone_count(), 2);
        assert_eq!(controller.save_scene(), before);
    }

    /// A path branching off another zone is left out along with its place in that zone's sibling groups
    #[test]
    fn creating_branch_not_saved() {
        let mut controller = controller();
        let before = controller.save_scene();
        pen_clicks(&mut controller, &[(10.0, 0.0), (20.0, -10.0)]);
        assert!(controller.tool.creating_path().is_some());
        assert!(!controller.objects.get_siblings(1).is_empty());

        let saved = controller.save_scene();
        assert_eq!(saved, before);
        assert!(saved.objects.validate().is_ok());
    }
}
