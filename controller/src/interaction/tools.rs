mod utils;
mod zone;
mod pen;
mod point;
mod shape;
mod shape_pen;
mod result;
pub use zone::EditorToolZone;
pub use pen::EditorToolPen;
pub use point::EditorToolPoint;
pub use shape::EditorToolShape;
pub use shape_pen::EditorToolShapePen;
pub(super) use result::ToolResult;
use result::{EditorToolActionResult, DefaultResults};

use studio_math::{Vec2, Vec3};
use crate::{Axis, Controller, ControlPointMode, ObjectBuffers, ObjectDiff, Bounds, InputModifiers, SavedObjects, Status};
use crate::objects::{ShapeParams, ShapeParamChange, ShapeProfile, ZoneOrderChange};
use super::history::{History, DragStep};

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

    /// Allows selecting and manipulating the shapes in a zone by their vertices.
    Shape,

    /// Allows clicking to draw a path of shape vertices in a zone, which becomes shapes when it is finished.
    /// If completed swaps back to Shape mode in the same zone.
    ShapePen,
}

pub struct ControllerStateInput {
    // World-space position of the mouse
    pub mouse: Vec2,
    
    // Mutable reference to the objects list
    pub objects: ObjectBuffers,

    // Inverse camera scale
    pub camera_inv_scale: f32,

    // Params that new shapes get
    pub shape_defaults: ShapeParams,

    // Size in pixels of the grid that ctrl snaps to
    pub grid_size: f32,
}

pub type SelectionType<'a> = &'a [u16]; // slice of vector of selected points

/// A box that can be dragged to transform the selection, drawn with handles at its corners and edge midpoints
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct TransformBox {
    /// World space center of the box
    pub center: Vec2,

    /// Half the width and height of the box, before it is rotated
    pub half_size: Vec2,

    /// Rotation of the box around its center in radians, from +x towards +y
    pub angle: f32,
}

/// Actions from the UI that the selected tool handles in its own way. Tools ignore the ones they don't support.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum SpecialAction {
    /// Set a normal vector. The bool is true if this continues a drag, so a success doesn't hide an error from earlier in the drag.
    /// Zone: sets the normal of the selected zones. Pen: sets the normal for new zones, and the zone being drawn.
    NormalSet(Vec3, bool),

    /// Zone, Point: separate the selected points from their unselected siblings.
    /// Shape: give the shapes that aren't selected their own copies of the selected shape vertices they share.
    SelectionSeparated,

    /// Zone: mirror the selected zones across an axis through the center of their bounds. Flipping across the vertical axis flips horizontally.
    SelectionFlipped(Axis),

    /// Zone: turn the selected zones a quarter turn around the center of their bounds. True turns clockwise, false counterclockwise.
    SelectionRotated(bool),

    /// Point: set the handle mode of the selected point.
    PointModeSet(ControlPointMode),

    /// Point: toggle the handle mode of the selected point between linear and continuous.
    PointModeToggled,

    /// Point: replace the selected point with its next sibling.
    CycleSibling,

    /// Point: insert a point halfway along the selected path, and select it.
    PathSplit,

    /// Point: set how far the selected path is feathered into the zone on its other side, in pixels, if it is an edge two zones share.
    /// The bool is true if this continues a drag, like NormalSet.
    PathFeatherSet(u8, bool),

    /// Point: merge the selected points into one group of siblings, and select them.
    /// Shape: merge the selected shape vertices into one at their average position, and select it.
    SelectionMerged,

    /// Pen: toggle if branching zones start with the normal of the zone they branch from.
    ToggleInherit,

    /// Pen, Shape pen: toggle if the pen stays active after finishing a zone or a path of shapes.
    ToggleStayActive,

    /// Shape: change one param of the selected shapes and the rest of their groups.
    /// With no shapes selected, the controller changes the params new shapes get instead.
    /// The bool is true if this continues a drag, like NormalSet.
    ShapeParamSet(ShapeParamChange, bool),

    /// Shape: join the selected shapes into the group of the first of them, so they are one surface.
    ShapesGrouped,

    /// Shape: give each selected shape a group of its own.
    ShapesUngrouped,

    /// Shape: select the vertices of a range of shapes, given as the first shape and how many there are.
    /// Sent to the shape tool when the shape pen switches to it, to select the shapes it just made.
    ShapesSelected(u16, u16),

    /// Zone: remove every shape from the selected zones.
    ShapesCleared,

    /// Zone: move the selected zones in front of or behind other zones. Zones in front take priority where they overlap.
    ZonesReordered(ZoneOrderChange),

    /// Zone, Point: move the selected points by a world space delta. Shape: move the selected shape vertices.
    /// The bool is true if this continues a held key, so holding an arrow key is one undoable change.
    SelectionNudged(Vec2, bool),
}

impl SpecialAction {
    /// How this action relates to a drag. Continuing to set a value continues the drag, everything else starts something new.
    pub fn drag_step(&self) -> DragStep {
        if matches!(self, Self::NormalSet(_, true) | Self::ShapeParamSet(_, true) | Self::SelectionNudged(_, true) | Self::PathFeatherSet(_, true)) { DragStep::Continue } else { DragStep::None }
    }
}

/// Where pasted zones go
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum PastePlacement {
    /// Exactly where they were copied from
    InPlace,

    /// With their center 10 screen pixels down and to the right of a world space position,
    /// or of where they were copied from if there is none
    Offset(Option<Vec2>),

    /// With their center at a world space position, i.e. the mouse
    At(Vec2),
}

/// How far pasted zones are offset from where they were copied from, in screen pixels
const PASTE_OFFSET: Vec2 = Vec2 { x: 10.0, y: 10.0 };

impl PastePlacement {
    /// Get where the center of pasted zones should go, given where it was copied from and the inverse camera scale
    pub(super) fn target(&self, center: Vec2, camera_inv_scale: f32) -> Vec2 {
        match *self {
            Self::InPlace => center,
            Self::Offset(base) => base.unwrap_or(center) + PASTE_OFFSET * camera_inv_scale,
            Self::At(pos) => pos,
        }
    }
}

/// Questions the UI can ask the selected tool about its own state. Tools answer None for the ones they don't support.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum SpecialQuery {
    /// Pen: the normal for new zones.
    Normal,

    /// Pen: if branching zones start with the normal of the zone they branch from.
    InheritNormal,

    /// Pen: if the pen stays active after finishing a zone.
    StayActive,

    /// Shape tools: the zone whose shapes are being edited, if any. Only tools that edit shapes answer,
    /// so the overlay always shows shapes for them, with knobs on the active zone's vertices.
    ActiveZone,

    /// Shape pen: the latest vertex of the path of shapes being drawn, which a ghost line joins to the cursor.
    /// Only answered while a path is being drawn.
    ShapeGhost,

    /// Shape pen: the shapes made from the latest finished path, until the next path starts.
    CreatedShapes,
}

/// Answers to special queries
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum SpecialValue {
    Normal(Vec3),
    Flag(bool),
    Zone(Option<u16>),
    Position(Vec2),

    /// A range of shapes, as the first shape and how many there are
    ShapeRange(u16, u16),
}

/// The params of the selected shapes, or of new shapes if none are selected. Each is None if the selected shapes have different values.
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ShapeParamsSummary {
    /// The params of the first selected shape, or of new shapes, to start from where the selected shapes differ
    pub first: ShapeParams,

    pub profile: Option<ShapeProfile>,
    pub inner_radius: Option<f32>,
    pub outer_radius: Option<f32>,
    pub steepness: Option<f32>,
    pub blend: Option<f32>,
}

impl ShapeParamsSummary {
    /// Summarize several shapes' params, keeping only the values they all share. With no params, every value is the default.
    pub fn of(params: &[ShapeParams], defaults: ShapeParams) -> Self {
        fn shared<T: PartialEq + Copy>(params: &[ShapeParams], defaults: ShapeParams, value: impl Fn(&ShapeParams) -> T) -> Option<T> {
            let Some(first) = params.first() else { return Some(value(&defaults)); };
            params.iter().all(|other| value(other) == value(first)).then(|| value(first))
        }
        Self {
            first: params.first().copied().unwrap_or(defaults),
            profile: shared(params, defaults, |params| params.profile),
            inner_radius: shared(params, defaults, |params| params.inner_radius),
            outer_radius: shared(params, defaults, |params| params.outer_radius),
            steepness: shared(params, defaults, |params| params.steepness),
            blend: shared(params, defaults, |params| params.blend),
        }
    }
}

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

    /// Get the shape vertices that are currently selected. Only the shape tools select them.
    fn shape_selection(&self) -> SelectionType<'_> { &[] }

    /// Get two coordinates of the bounding box we are currently dragging out
    fn selection_box(&self) -> Option<(Vec2, Vec2)> { None }

    /// Get the box that can be dragged to transform the selection, if this tool has one
    fn transform_box(&self, _objects: &ObjectBuffers) -> Option<TransformBox> { None }

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

    /// Handle selecting everything. Ignored by default.
    fn handle_select_all(&mut self, _state: ControllerStateInput) -> ToolResult { self.ok() }

    /// Handle pasting copied zones, selecting them. Ignored by default.
    fn handle_paste(&mut self, _state: ControllerStateInput, _objects: &SavedObjects, _placement: PastePlacement) -> ToolResult { self.ok() }

    /// Handle an action from the UI that only some tools support. Ignored by default.
    fn handle_special_action(&mut self, _state: ControllerStateInput, _action: SpecialAction) -> ToolResult { self.ok() }

    /// Answer a question from the UI about this tool's state. Unanswered by default.
    fn handle_special_query(&self, _query: SpecialQuery) -> Option<SpecialValue> { None }
}

impl Controller {
    pub fn selected_tool(&self) -> EditorToolKind { self.tool.kind() }

    /// Select a tool, and perform related side-effects. Changes to the objects are recorded for undo.
    pub fn select_tool(&mut self, tool: EditorToolKind) {
        self.history.close(&self.objects, self.tool.as_ref());
        let (deselect_state, select_state) = (self.create_state(), self.create_state());
        let normals_stale = switch_tool(&mut self.tool, &mut self.history, tool, deselect_state, select_state);
        self.history.close(&self.objects, self.tool.as_ref());
        self.mark_normals_stale(normals_stale);
    }

    /// Run a tool action, restoring the objects and tool state if it fails, and recording its changes for undo.
    /// The step decides how errors are kept and how the changes are grouped with other actions in a drag.
    /// Returns true if the action succeeded.
    pub(crate) fn run_tool(&mut self, step: DragStep, action: impl FnOnce(&mut dyn EditorTool, ControllerStateInput) -> ToolResult) -> bool {
        if step == DragStep::None { self.history.close(&self.objects, self.tool.as_ref()); }

        let state = self.create_state();
        let result = run_tool_action(&mut self.tool, state, &mut self.status, step == DragStep::Continue, action);
        let succeeded = result.is_some();
        if let Some((EditorToolActionResult { new_tool, normals_stale }, change)) = result {
            self.history.record(change, normals_stale);
            self.mark_normals_stale(normals_stale);

            // Shapes the old tool just made are selected in the new one
            let switching = new_tool != self.tool.kind();
            let created = self.tool.handle_special_query(SpecialQuery::CreatedShapes);

            let (deselect_state, select_state) = (self.create_state(), self.create_state());
            let normals_stale = switch_tool(&mut self.tool, &mut self.history, new_tool, deselect_state, select_state);
            self.mark_normals_stale(normals_stale);

            // Selecting doesn't change any objects, so it can't fail in a way worth showing
            if switching && let Some(SpecialValue::ShapeRange(start, count)) = created {
                let _ = self.tool.handle_special_action(self.create_state(), SpecialAction::ShapesSelected(start, count));
            }
        }

        if step == DragStep::Release { self.history.close(&self.objects, self.tool.as_ref()); }
        succeeded
    }

    /// Get the selected zones in a form that can be pasted later. Only the zone tool can copy, and only if zones are selected.
    /// If copying fails, the status shows why.
    pub fn copy_selection(&mut self) -> Option<SavedObjects> {
        if self.tool.kind() != EditorToolKind::Zone { return None; }
        let zones = self.selected_zones();
        if zones.is_empty() { return None; }

        match self.objects.copy_zones(&zones) {
            Ok(objects) => Some(objects),
            Err(e) => {
                self.status = Status::Error(format!("{e:#}"));
                None
            },
        }
    }

    /// Paste copied zones with the selected tool, which selects them. Changes are recorded for undo.
    /// Returns where the center of the pasted zones ended up, or None if nothing was pasted.
    pub fn paste(&mut self, objects: &SavedObjects, placement: PastePlacement) -> Option<Vec2> {
        if !self.run_tool(DragStep::None, |tool, state| tool.handle_paste(state, objects, placement)) { return None; }
        if self.tool.kind() != EditorToolKind::Zone { return None; }
        self.objects.zones_center(&self.selected_zones()).ok().flatten()
    }

    /// Run an action from the UI that the selected tool handles in its own way, with the same rollback and error handling as other tool actions.
    pub(crate) fn run_special_action(&mut self, action: SpecialAction) {
        // Shape params with no shapes selected are for new shapes, which the controller keeps so every shape tool uses them
        if let SpecialAction::ShapeParamSet(change, _) = action && self.selected_shapes().is_empty() {
            self.shape_defaults = self.shape_defaults.with(change);
            return;
        }
        self.run_tool(action.drag_step(), |tool, state| tool.handle_special_action(state, action));
    }

    /// Ask the selected tool about its state, if it supports the question
    pub fn tool_query(&self, query: SpecialQuery) -> Option<SpecialValue> {
        self.tool.handle_special_query(query)
    }

    /// Get the IDs of the selected points
    pub fn selected_points(&self) -> &[u16] { self.tool.selection() }

    /// Returns true if the selected zones can be moved in front of or behind other zones
    pub fn can_reorder_zones(&self) -> bool {
        self.tool.kind() == EditorToolKind::Zone && !self.selected_zones().is_empty() && self.objects.zone_count() > 1
    }

    /// Returns true if there are any zones
    pub fn has_objects(&self) -> bool { self.objects.zone_count() > 0 }

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

    /// How far the selected path is feathered, if it is an edge two zones share. Only the point tool selects paths.
    pub fn selected_path_feather(&self) -> Option<u8> {
        if self.tool.kind() != EditorToolKind::Point { return None; }
        let groups = self.objects.sibling_groups(self.tool.selection());
        if groups.len() != 2 { return None; }
        let starts = self.objects.shared_segment_starts(&groups[0], &groups[1]).ok()?;
        if starts.len() < 2 { return None; }
        starts.iter().filter_map(|&start| self.objects.get_point_info(start).ok()).map(|point| point.feather()).max()
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

    /// Get the IDs of the shapes whose vertices are all selected, sorted
    pub fn selected_shapes(&self) -> Vec<u16> {
        self.objects.shapes_with_all_vertices(self.tool.shape_selection()).unwrap_or_default()
    }

    /// Get the IDs of the selected shape vertices
    pub fn selected_shape_vertices(&self) -> &[u16] { self.tool.shape_selection() }

    /// Get the params the selected shapes share, or the params new shapes get if no shapes are selected
    pub fn selected_shape_params(&self) -> ShapeParamsSummary {
        let params: Vec<ShapeParams> = self.selected_shapes().into_iter()
            .filter_map(|shape_id| self.objects.get_shape_info(shape_id).ok())
            .map(|shape| shape.params())
            .collect();
        ShapeParamsSummary::of(&params, self.shape_defaults)
    }

    /// Returns true if the selected shapes are from more than one group, so they can be joined into one
    pub fn can_group_shapes(&self) -> bool {
        let mut groups: Vec<u16> = self.selected_shapes().into_iter()
            .filter_map(|shape_id| self.objects.get_shape_info(shape_id).ok())
            .map(|shape| shape.group())
            .collect();
        groups.sort();
        groups.dedup();
        groups.len() > 1
    }

    /// Returns true if any selected shape shares its group with another shape, so it can be given its own
    pub fn can_ungroup_shapes(&self) -> bool {
        self.selected_shapes().into_iter().any(|shape_id| self.objects.shape_groups(&[shape_id]).is_ok_and(|group| group.len() > 1))
    }

    /// Returns true if detaching the selected shape vertices would give any shape its own copy of one
    pub fn can_detach_shape_vertices(&self) -> bool {
        self.objects.can_detach_shape_vertices(self.tool.shape_selection(), &self.selected_shapes())
    }

    /// Returns true if any selected zone has shapes
    pub fn selected_zones_have_shapes(&self) -> bool {
        self.selected_zones().into_iter().any(|zone_id| self.objects.get_zone_info(zone_id).is_ok_and(|zone| zone.shapes().1 > 0))
    }

    /// Returns true if the selected shape vertices are at least two vertices in one zone, so they can be merged
    pub fn can_merge_shape_vertices(&self) -> bool {
        let vertices = self.tool.shape_selection();
        let zones: Vec<Option<u16>> = vertices.iter().map(|&vertex_id| self.objects.zone_of_vertex(vertex_id).ok()).collect();
        vertices.len() > 1 && zones[0].is_some() && zones.iter().all(|&zone| zone == zones[0])
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

    pub fn create_state(&self) -> ControllerStateInput {
        ControllerStateInput {
            mouse: self.cursor_pos,
            objects: self.objects.clone(),
            camera_inv_scale: self.camera.inv_scale(),
            shape_defaults: self.shape_defaults,
            grid_size: self.grid_size as f32,
        }
    }
}


/// The changes one action made to the objects, and the tool state from before it.
pub struct EditorChange {
    pub objects: ObjectDiff,
    pub tool: Box<dyn EditorTool>,
}

/// Switch to a different tool, recording any changes to the objects for undo. Does nothing if the tool is already selected.
/// Returns the region where normals might have changed.
pub(super) fn switch_tool(
    tool: &mut Box<dyn EditorTool>,
    history: &mut History,
    kind: EditorToolKind,
    deselect_state: ControllerStateInput,
    select_state: ControllerStateInput,
) -> Option<Bounds> {
    // If this is the current tool, don't do anything.
    // Unless we have sub modes in which case cycle those instead of this stuff.
    if kind == tool.kind() { return None; }
    let objects = deselect_state.objects.clone();

    // Deselecting a tool that is creating a path deletes it, and it may have been assigned if its normal was set.
    // The path being created is always the last zone.
    let normals_stale = match objects.zone_count().checked_sub(1) {
        Some(zone_id) if tool.creating_path().is_some() => objects.zone_bounds(zone_id).ok().flatten(),
        _ => None,
    };

    // Drop the old tool
    let tool_before = tool.clone_box();
    objects.begin_change();
    let selection = Vec::from(tool.deselect(deselect_state)); // cloning should be okay

    // Select the new tool
    *tool = match kind {
        EditorToolKind::Zone  => EditorToolZone::select(&selection, select_state),
        EditorToolKind::Point => EditorToolPoint::select(&selection, select_state),
        EditorToolKind::Pen   => EditorToolPen::select(&selection, select_state),
        EditorToolKind::Shape => EditorToolShape::select(&selection, select_state),
        EditorToolKind::ShapePen => EditorToolShapePen::select(&selection, select_state),
    };
    history.record(EditorChange { objects: objects.finish_change(), tool: tool_before }, normals_stale);
    normals_stale
}

/// Run a tool action, restoring the objects and tool state if it fails.
/// Errors always replace the status, and successes clear it unless we are continuing a drag.
/// Returns the result of the action and the change it made if it succeeded.
pub(super) fn run_tool_action(
    tool: &mut Box<dyn EditorTool>,
    state: ControllerStateInput,
    status: &mut Status,
    continuing_drag: bool,
    action: impl FnOnce(&mut dyn EditorTool, ControllerStateInput) -> ToolResult,
) -> Option<(EditorToolActionResult, EditorChange)> {
    let objects = state.objects.clone();
    let tool_before = tool.clone_box();
    objects.begin_change();
    match action(tool.as_mut(), state) {
        Ok(result) => {
            if !continuing_drag { *status = Status::Ok; }
            Some((result, EditorChange { objects: objects.finish_change(), tool: tool_before }))
        },
        Err(e) => {
            objects.rollback_change();
            *tool = tool_before;
            *status = Status::Error(format!("{e:#}"));
            None
        },
    }
}

/// Helpers for running tools against objects in tests
#[cfg(test)]
pub(super) mod test_utils {
    use studio_math::Vec2;

    use crate::ObjectBuffers;
    use super::ControllerStateInput;

    /// Create the input state for a tool, as if the camera is at 1:1 scale
    pub(crate) fn state(objects: &ObjectBuffers) -> ControllerStateInput {
        state_scaled(objects, 1.0)
    }

    /// Create the input state for a tool with a specific inverse camera scale
    pub(super) fn state_scaled(objects: &ObjectBuffers, camera_inv_scale: f32) -> ControllerStateInput {
        ControllerStateInput { mouse: Vec2::ZERO, objects: objects.clone(), camera_inv_scale, shape_defaults: crate::objects::ShapeParams::default(), grid_size: 1.0 }
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
        Ok(EditorToolActionResult::new(EditorToolKind::Zone, None))
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
        let mut status = Status::Ok;

        let result = run_tool_action(&mut tool, state(&objects), &mut status, false, |tool, mut state| {
            tool.handle_click(test_utils::state(&state.objects), Vec2::ZERO, InputModifiers::none())?;
            assert!(!tool.selection().is_empty());
            state.objects.create_zone()?;
            anyhow::bail!("Failed partway")
        });

        assert!(result.is_none());
        assert_eq!(status, Status::Error("Failed partway".to_owned()));
        assert!(tool.selection().is_empty());
        assert_eq!(objects.object_counts(), (1, 4));
    }

    /// A successful action keeps its changes and clears the status
    #[test]
    fn success_keeps_state() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool: Box<dyn EditorTool> = EditorToolZone::init();
        let mut status = Status::Error("Old error".to_owned());

        let result = run_tool_action(&mut tool, state(&objects), &mut status, false, |tool, state| {
            tool.handle_click(state, Vec2::ZERO, InputModifiers::none())
        });

        assert!(result.is_some());
        assert_eq!(status, Status::Ok);
        assert!(!tool.selection().is_empty());
    }

    /// Continuing a drag only replaces the status if the action fails
    #[test]
    fn continuing_drag_keeps_status() {
        let objects = ObjectBuffers::headless();
        let mut tool: Box<dyn EditorTool> = EditorToolZone::init();

        let mut status = Status::Error("Old error".to_owned());
        run_tool_action(&mut tool, state(&objects), &mut status, true, ok_action);
        assert_eq!(status, Status::Error("Old error".to_owned()));

        run_tool_action(&mut tool, state(&objects), &mut status, true, err_action);
        assert_eq!(status, Status::Error("New error".to_owned()));

        let mut status = Status::Feedback("Saved".to_owned());
        run_tool_action(&mut tool, state(&objects), &mut status, true, ok_action);
        assert_eq!(status, Status::Feedback("Saved".to_owned()));

        let mut status = Status::Ok;
        run_tool_action(&mut tool, state(&objects), &mut status, true, ok_action);
        assert_eq!(status, Status::Ok);
    }

    /// Actions other than continuing a drag always replace the status, including feedback
    #[test]
    fn other_actions_replace_status() {
        let objects = ObjectBuffers::headless();
        let mut tool: Box<dyn EditorTool> = EditorToolZone::init();

        let mut status = Status::Error("Old error".to_owned());
        run_tool_action(&mut tool, state(&objects), &mut status, false, err_action);
        assert_eq!(status, Status::Error("New error".to_owned()));

        run_tool_action(&mut tool, state(&objects), &mut status, false, ok_action);
        assert_eq!(status, Status::Ok);

        let mut status = Status::Feedback("Saved".to_owned());
        run_tool_action(&mut tool, state(&objects), &mut status, false, ok_action);
        assert_eq!(status, Status::Ok);
    }

    /// Summaries keep the values every shape shares, and use the defaults when there are no shapes
    #[test]
    fn shape_params_summary() {
        let defaults = ShapeParams::default();
        let all_default = ShapeParamsSummary::of(&[], defaults);
        assert_eq!(all_default.outer_radius, Some(defaults.outer_radius));
        assert_eq!(all_default.profile, Some(ShapeProfile::Cone));

        let dome = ShapeParams { profile: ShapeProfile::Dome, outer_radius: 4.0, ..defaults };
        let mixed = ShapeParamsSummary::of(&[defaults, dome], ShapeParams { steepness: 9.0, ..defaults });
        assert_eq!((mixed.profile, mixed.outer_radius), (None, None));
        assert_eq!(mixed.first, defaults);
        assert_eq!((mixed.inner_radius, mixed.steepness, mixed.blend), (Some(0.0), Some(1.0), Some(2.0)));
    }

    /// Tools ignore special actions and queries they don't support
    #[test]
    fn special_unsupported_is_ignored() {
        let objects = ObjectBuffers::headless();
        let mut tool: Box<dyn EditorTool> = EditorToolZone::init();

        let result = tool.handle_special_action(state(&objects), SpecialAction::ToggleInherit).unwrap();
        assert!(result.normals_stale.is_none());
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert_eq!(tool.handle_special_query(SpecialQuery::Normal), None);
    }
}
