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
use crate::{Controller, ObjectBuffers};

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

pub type SelectionType = (Option<u16>, Option<u16>);

/// Defines a class for an editor tool.
/// Will call default when selecting this tool, and drop when selecting a different tool.
pub trait EditorTool {
    /// Create a new instance of this tool, with a selection preserved
    /// Nothing else should be passed between objects
    fn select(selection: SelectionType) -> Box<Self> where Self : Sized;

    /// Deselect a tool, cleaning up external state if needed.
    /// Returns the selection that should be passed to the new tool if any.
    fn deselect(&mut self) -> SelectionType;

    /// Get the kind of this tool
    fn kind(&self) -> EditorToolKind;

    /// Get the zone and point that are currently selected
    fn selection(&self) -> SelectionType { (None, None) }

    /// Get the "ignore zone" which we pass to the shader in some modes
    fn zone_ignore(&self) -> Option<u16> { None }

    /// Handle a click at a point
    fn handle_click(&mut self, state: ControllerStateInput, pos: Vec2) -> ToolResult;

    /// Handle a drag starting at a point
    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: Vec2) -> ToolResult;

    /// Handle a drag with a new position
    fn handle_dragging_to(&mut self, state: ControllerStateInput, pos: Vec2) -> ToolResult;

    /// Handle releasing drag
    fn handle_drag_released(&mut self, state: ControllerStateInput) -> ToolResult;

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

        // Select the new tool, which should drop the old tool.
        let selection = self.tool.deselect();
        self.tool = match tool {
            EditorToolKind::Zone  => EditorToolZone::select(selection),
            EditorToolKind::Point => EditorToolPoint::select(selection),
            EditorToolKind::Pen   => EditorToolPen::select(selection),
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