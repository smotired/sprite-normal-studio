use serde::{Deserialize, Serialize};
use studio_math::Vec2;

use crate::Controller;
use crate::interaction::history::History;
use crate::interaction::tools::{EditorTool, EditorToolZone};
use crate::objects::{LoadedObjects, SavedObjects};

/// Everything in the controller that is saved to a workspace file: the objects and the light, but not the camera or the view.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedScene {
    pub light: SavedLight,

    #[serde(flatten)]
    pub objects: SavedObjects,
}

/// The light, as stored in a workspace file
#[derive(Copy, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedLight {
    pub position: Vec2,
    pub height: f32,
    pub color: [u8; 3],
}

impl Controller {
    /// Get everything that should be saved to a workspace file
    pub fn save_scene(&self) -> SavedScene {
        let (r, g, b) = self.light.color();
        SavedScene {
            light: SavedLight { position: self.light.position(), height: self.light.height(), color: [r, g, b] },
            objects: self.objects.save(),
        }
    }

    /// Replace the objects and light with ones loaded from a workspace file.
    /// The objects should already be validated, so this can't fail. Should be called after set_inputs, so the light isn't recentered.
    pub fn load_scene(&mut self, light: SavedLight, objects: LoadedObjects) {
        self.objects.load(objects);
        let [r, g, b] = light.color;
        self.light.set(light.position, light.height, (r, g, b));

        // Nothing is selected in the new objects, old errors don't apply to them, and old changes can't be undone into them
        self.tool = EditorToolZone::init();
        self.tool_error = None;
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
        self.tool_error = None;
        self.history = History::default();
        self.normals_stale = Some((Vec2::ZERO, self.assigner.size()));
    }
}
