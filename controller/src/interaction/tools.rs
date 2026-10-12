mod utils;
mod zone;
mod pen;
mod point;
mod result;
pub use zone::EditorToolZone;
pub use pen::EditorToolPen;
pub use point::EditorToolPoint;
use result::{ToolResult, EditorToolActionResult};

use studio_math::{Vec2, Vec3};
use crate::{Controller, ControlPointMode, ObjectBuffers, ObjectSnapshot, InputModifiers};

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

/// Allows boxed editor tools to be cloned, so their state can be snapshotted.
/// Automatically implemented for every tool that implements Clone.
pub trait EditorToolClone {
    fn clone_box(&self) -> Box<dyn EditorTool>;
}

impl<T: EditorTool + Clone + 'static> EditorToolClone for T {
    fn clone_box(&self) -> Box<dyn EditorTool> { Box::new(self.clone()) }
}

impl Clone for Box<dyn EditorTool> {
    fn clone(&self) -> Self { (**self).clone_box() }
}

/// Defines a class for an editor tool.
/// Will call default when selecting this tool, and drop when selecting a different tool.
pub trait EditorTool: EditorToolClone {
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

    /// The error from the latest tool action, if it failed
    pub fn tool_error(&self) -> Option<&str> { self.tool_error.as_deref() }

    /// Run a tool action, restoring the objects and tool state if it fails.
    /// Set continuing_drag when this action continues a drag, so a success doesn't hide an error from earlier in the drag.
    pub(crate) fn run_tool(&mut self, continuing_drag: bool, action: impl FnOnce(&mut dyn EditorTool, ControllerStateInput) -> ToolResult) {
        let state = self.create_state();
        if let Some(EditorToolActionResult { new_tool, normals_stale }) = run_tool_action(&mut self.tool, state, &mut self.tool_error, continuing_drag, action) {
            self.normals_stale |= normals_stale;
            self.select_tool(new_tool);
        }
    }

    /// Run an action on the objects that isn't part of a tool, with the same rollback and error handling as tool actions.
    pub(crate) fn run_object_action(&mut self, continuing_drag: bool, normals_stale: bool, action: impl FnOnce(&mut ObjectBuffers) -> anyhow::Result<()>) {
        self.run_tool(continuing_drag, |tool, mut state| {
            action(&mut state.objects)?;
            Ok(EditorToolActionResult::new(tool.kind(), normals_stale))
        });
    }

    /// Run an action on the objects that isn't part of a tool and replaces the selection, in the point tool.
    /// The action gets the current selection and returns the new one. Has the same rollback and error handling as tool actions.
    pub(crate) fn run_selection_action(&mut self, normals_stale: bool, action: impl FnOnce(&mut ObjectBuffers, &[u16]) -> anyhow::Result<Vec<u16>>) {
        let selection = Vec::from(self.tool.selection());
        let mut new_selection = None;
        self.run_tool(false, |tool, mut state| {
            new_selection = Some(action(&mut state.objects, &selection)?);
            Ok(EditorToolActionResult::new(tool.kind(), normals_stale))
        });

        if let Some(selection) = new_selection {
            self.tool = EditorToolPoint::select(&selection, self.create_state());
        }
    }

    /// Get the IDs of the selected points
    pub fn selected_points(&self) -> &[u16] { self.tool.selection() }

    /// Get the handle mode of the point a point's handles sync to, or None if they sync to different points
    pub fn point_handle_mode(&self, point_id: u16) -> Option<ControlPointMode> {
        self.objects.get_point_mode(point_id).ok().flatten()
    }

    /// Returns true if a point has any siblings
    pub fn point_has_siblings(&self, point_id: u16) -> bool {
        point_id < self.objects.point_count() && !self.objects.get_siblings(point_id).is_empty()
    }

    /// If the selection is two sibling groups next to each other in a zone (i.e. a clicked path),
    /// get that zone and the ID of the point that starts the path between them.
    pub fn selected_path(&self) -> Option<(u16, u16)> {
        let groups = self.objects.sibling_groups(self.tool.selection());
        if groups.len() != 2 { return None; }
        self.objects.path_between(&groups[0], &groups[1]).ok().flatten()
    }

    /// Returns true if the selected points are from more than one sibling group
    pub fn can_merge_selection(&self) -> bool {
        self.objects.sibling_groups(self.tool.selection()).len() > 1
    }

    /// Get the IDs of the zones that have selected points, sorted.
    /// In the zone tool these are exactly the selected zones.
    pub fn selected_zones(&self) -> Vec<u16> {
        let mut zones: Vec<u16> = self.tool.selection().iter()
            .filter_map(|&point_id| self.objects.get_point_info(point_id).ok())
            .map(|point| point.zone_id())
            .collect();
        zones.sort();
        zones.dedup();
        zones
    }

    /// Get the normal vector shared by all selected zones, or None if nothing is selected or they don't all match.
    pub fn selected_zones_normal(&self) -> Option<Vec3> {
        let mut normals = self.selected_zones().into_iter()
            .filter_map(|zone_id| self.objects.get_zone_info(zone_id).ok())
            .map(|zone| zone.normal());
        let first = normals.next()?;
        normals.all(|normal| normal == first).then_some(first)
    }

    /// Returns true if any selected point has an unselected sibling
    pub fn can_separate_selection(&self) -> bool {
        self.objects.can_separate(self.tool.selection())
    }

    /// Take a snapshot of the objects and tool state
    pub fn snapshot(&self) -> EditorSnapshot { EditorSnapshot::take(&self.objects, self.tool.as_ref()) }

    /// Restore the objects and tool state from a snapshot
    pub fn restore(&mut self, snapshot: EditorSnapshot) { snapshot.restore_into(&self.objects, &mut self.tool); }

    pub fn create_state(&self) -> ControllerStateInput {
        ControllerStateInput {
            mouse: self.cursor_pos,
            objects: self.objects.clone(),
            camera_inv_scale: self.camera.inv_scale(),
        }
    }
}


/// A copy of the objects and the tool state, which can be restored later.
#[derive(Clone)]
pub struct EditorSnapshot {
    objects: ObjectSnapshot,
    tool: Box<dyn EditorTool>,
}

impl EditorSnapshot {
    fn take(objects: &ObjectBuffers, tool: &dyn EditorTool) -> Self {
        Self { objects: objects.snapshot(), tool: tool.clone_box() }
    }

    fn restore_into(self, objects: &ObjectBuffers, tool: &mut Box<dyn EditorTool>) {
        objects.restore(self.objects);
        *tool = self.tool;
    }
}

/// Run a tool action, restoring the objects and tool state if it fails.
/// Errors always replace the previous error, and successes clear it unless we are continuing a drag.
/// Returns the result of the action if it succeeded.
fn run_tool_action(
    tool: &mut Box<dyn EditorTool>,
    state: ControllerStateInput,
    error: &mut Option<String>,
    continuing_drag: bool,
    action: impl FnOnce(&mut dyn EditorTool, ControllerStateInput) -> ToolResult,
) -> Option<EditorToolActionResult> {
    let objects = state.objects.clone();
    let snapshot = EditorSnapshot::take(&objects, tool.as_ref());
    match action(tool.as_mut(), state) {
        Ok(result) => {
            if !continuing_drag { *error = None; }
            Some(result)
        },
        Err(e) => {
            snapshot.restore_into(&objects, tool);
            *error = Some(format!("{e:#}"));
            None
        },
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

#[cfg(test)]
mod tests {
    use studio_math::Vec2;

    use crate::ObjectBuffers;
    use crate::objects::test_utils::add_square;
    use super::*;
    use super::test_utils::{self, state};

    /// A successful action that doesn't change anything
    fn ok_action(_tool: &mut dyn EditorTool, _state: ControllerStateInput) -> ToolResult {
        Ok(EditorToolActionResult::new(EditorToolKind::Zone, false))
    }

    /// A failing action that doesn't change anything
    fn err_action(_tool: &mut dyn EditorTool, _state: ControllerStateInput) -> ToolResult {
        anyhow::bail!("New error")
    }

    /// An action that fails partway through restores the objects and tool state
    #[test]
    fn error_restores_state() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool: Box<dyn EditorTool> = EditorToolZone::init();
        let mut error = None;

        let result = run_tool_action(&mut tool, state(&objects), &mut error, false, |tool, mut state| {
            tool.handle_click(test_utils::state(&state.objects), Vec2::ZERO, InputModifiers::none())?;
            assert!(!tool.selection().is_empty());
            state.objects.create_zone()?;
            anyhow::bail!("Failed partway")
        });

        assert!(result.is_none());
        assert_eq!(error.as_deref(), Some("Failed partway"));
        assert!(tool.selection().is_empty());
        assert_eq!(objects.object_counts(), (1, 4));
    }

    /// A successful action keeps its changes and clears the error
    #[test]
    fn success_keeps_state() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool: Box<dyn EditorTool> = EditorToolZone::init();
        let mut error = Some("Old error".to_owned());

        let result = run_tool_action(&mut tool, state(&objects), &mut error, false, |tool, state| {
            tool.handle_click(state, Vec2::ZERO, InputModifiers::none())
        });

        assert!(result.is_some());
        assert!(error.is_none());
        assert!(!tool.selection().is_empty());
    }

    /// Continuing a drag only replaces the error if the action fails
    #[test]
    fn continuing_drag_keeps_error() {
        let objects = ObjectBuffers::headless();
        let mut tool: Box<dyn EditorTool> = EditorToolZone::init();

        let mut error = Some("Old error".to_owned());
        run_tool_action(&mut tool, state(&objects), &mut error, true, ok_action);
        assert_eq!(error.as_deref(), Some("Old error"));

        run_tool_action(&mut tool, state(&objects), &mut error, true, err_action);
        assert_eq!(error.as_deref(), Some("New error"));

        let mut error = None;
        run_tool_action(&mut tool, state(&objects), &mut error, true, ok_action);
        assert!(error.is_none());
    }

    /// Actions other than continuing a drag always replace the error
    #[test]
    fn other_actions_replace_error() {
        let objects = ObjectBuffers::headless();
        let mut tool: Box<dyn EditorTool> = EditorToolZone::init();

        let mut error = Some("Old error".to_owned());
        run_tool_action(&mut tool, state(&objects), &mut error, false, err_action);
        assert_eq!(error.as_deref(), Some("New error"));

        run_tool_action(&mut tool, state(&objects), &mut error, false, ok_action);
        assert!(error.is_none());
    }
}
