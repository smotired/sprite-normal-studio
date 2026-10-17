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
    /// Get everything that should be saved to a workspace file
    pub fn save_scene(&self) -> SavedScene {
        let slice_size = (self.sprite_size != self.sheet_size()).then_some(self.sprite_size);
        SavedScene { objects: self.objects.save(), slice_size, animations: self.animations.clone() }
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
        self.status = Status::Ok;
        self.history = History::default();
        self.normals_stale = Some((Vec2::ZERO, self.assigner.size()));
    }
}
