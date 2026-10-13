use serde::{Deserialize, Serialize};
use studio_math::Vec2;

use crate::{Controller, Status};
use crate::interaction::history::History;
use crate::interaction::tools::{EditorTool, EditorToolZone};
use crate::objects::{LoadedObjects, SavedObjects};

/// Everything in the controller that is saved to a workspace file: the objects, but not the light, the camera or the view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedScene {
    #[serde(flatten)]
    pub objects: SavedObjects,
}

impl Controller {
    /// Get everything that should be saved to a workspace file
    pub fn save_scene(&self) -> SavedScene {
        SavedScene { objects: self.objects.save() }
    }

    /// Replace the objects with ones loaded from a workspace file, and reset the light.
    /// The objects should already be validated, so this can't fail. Should be called before set_inputs, so the light is centered on the spritesheet.
    pub fn load_scene(&mut self, objects: LoadedObjects) {
        self.objects.load(objects);
        self.light = Default::default();

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

        self.tool = EditorToolZone::init();
        self.status = Status::Ok;
        self.history = History::default();
        self.normals_stale = Some((Vec2::ZERO, self.assigner.size()));
    }
}
