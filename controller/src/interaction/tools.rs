mod utils;
mod zone;
mod pen;
mod point;
mod result;
pub use zone::EditorToolZone;
pub use pen::EditorToolPen;
pub use point::EditorToolPoint;
use result::{ToolResult, EditorToolActionResult};

use studio_math::Vec2;
use crate::{Controller, ObjectBuffers, InputModifiers};

/// Options for which tool we are using in the editor.
/// Tool options may later be added as tuple parameters in this enum.
#[derive(PartialEq, Copy, Clone, Debug)]
pub enum EditorToolKind {
    /// Allows selecting and manipulating zones by their control point.
    Zone,

    /// Allows selecting and manipulating individual control points.
    Point,

    /// Allows clicking/dragging to create control points for a zone.
    /// If canceled or completed swaps back to Zone mode and selects the created path.
    Pen,
}

pub struct ControllerStateInput {
    // World-space position of the mouse
    pub mouse: Vec2,
    
    // Mutable reference to the objects list
    pub objects: ObjectBuffers,

    // Inverse camera scale
    pub camera_inv_scale: f32,
}

pub type SelectionType<'a> = &'a [u16]; // slice of vector of selected points

/// Defines a class for an editor tool.
/// Will call default when selecting this tool, and drop when selecting a different tool.
pub trait EditorTool {
    /// Create a new instance of this tool, initialized from nothing.
    fn init() -> Box<Self> where Self : Sized;

    /// Create a new instance of this tool, with a selection preserved
    /// Nothing else should be passed between objects
    fn select(selection: SelectionType, state: ControllerStateInput) -> Box<Self> where Self : Sized;

    /// Deselect a tool, cleaning up external state if needed.
    /// Returns the selection that should be passed to the new tool if any.
    fn deselect(&mut self, state: ControllerStateInput) -> SelectionType<'_>;

    /// Get the kind of this tool
    fn kind(&self) -> EditorToolKind;

    /// Get the points that are currently selected
    fn selection(&self) -> SelectionType<'_>;

    /// Get two coordinates of the bounding box we are currently dragging out
    fn selection_box(&self) -> Option<(Vec2, Vec2)> { None }

    /// If this tool is creating a path, returns whether the latest point is the first point of that path
    /// (i.e. the path is about to be closed). The path being created is always the last zone.
    fn creating_path(&self) -> Option<bool> { None }

    /// Handle a click at a point
    fn handle_click(&mut self, state: ControllerStateInput, pos: Vec2, modifiers: InputModifiers) -> ToolResult;

    /// Handle a drag starting at a point
    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: Vec2, modifiers: InputModifiers) -> ToolResult;

    /// Handle a drag with a new position
    fn handle_dragging_to(&mut self, state: ControllerStateInput, pos: Vec2, modifiers: InputModifiers) -> ToolResult;

    /// Handle releasing drag
    fn handle_drag_released(&mut self, state: ControllerStateInput, modifiers: InputModifiers) -> ToolResult;

    /// Handle cancelling something
    fn handle_cancel(&mut self, state: ControllerStateInput) -> ToolResult;

    /// Handle deleting something
    fn handle_delete(&mut self, state: ControllerStateInput) -> ToolResult;
}

impl Controller {
    pub fn selected_tool(&self) -> EditorToolKind { self.tool.kind() }

    /// Select a tool, and perform related side-effects
    pub fn select_tool(&mut self, tool: EditorToolKind) {
        // If this is the current tool, don't do anything.
        // Unless we have sub modes in which case cycle those instead of this stuff.
        if tool == self.tool.kind() { return; }

        // Drop the old tool
        let selection = Vec::from(self.tool.deselect(self.create_state())); // cloning should be okay

        // Select the new tool
        let state = self.create_state();
        self.tool = match tool {
            EditorToolKind::Zone  => EditorToolZone::select(&selection, state),
            EditorToolKind::Point => EditorToolPoint::select(&selection, state),
            EditorToolKind::Pen   => EditorToolPen::select(&selection, state),
        };
    }

    pub fn handle_tool_result(&mut self, result: ToolResult) {
        let EditorToolActionResult { new_tool, normals_stale } = result.unwrap(); // TODO: Handle error
        self.normals_stale |= normals_stale;
        self.select_tool(new_tool);
    }

    pub fn create_state(&self) -> ControllerStateInput {
        ControllerStateInput {
            mouse: self.cursor_pos,
            objects: self.objects.clone(),
            camera_inv_scale: self.camera.inv_scale(),
        }
    }
}


/// Helpers for running tools against objects in tests
#[cfg(test)]
mod test_utils {
    use studio_math::Vec2;

    use crate::ObjectBuffers;
    use super::ControllerStateInput;

    /// Create the input state for a tool, as if the camera is at 1:1 scale
    pub(super) fn state(objects: &ObjectBuffers) -> ControllerStateInput {
        state_scaled(objects, 1.0)
    }

    /// Create the input state for a tool with a specific inverse camera scale
    pub(super) fn state_scaled(objects: &ObjectBuffers, camera_inv_scale: f32) -> ControllerStateInput {
        ControllerStateInput { mouse: Vec2::ZERO, objects: objects.clone(), camera_inv_scale }
    }
}
