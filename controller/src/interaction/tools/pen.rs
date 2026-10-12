use studio_math::{Vec2, Vec3};

use crate::InputModifiers;
use crate::objects::ControlPointMode;

use super::{EditorTool, EditorToolKind, ControllerStateInput, SelectionType, SpecialAction, SpecialQuery, SpecialValue, utils};
use super::result::{ToolResult, DefaultResults, EditorToolActionResult};

/// The Pen tool allows creating a new zone by dragging out a bezier path.
///
/// Clicking or dragging on an existing point starts a zone branching off of it, and while branching,
/// clicking or dragging on a point or path connected to it joins back to it.
/// Clicking a path when not creating a zone inserts a point into it, and dragging on it starts a branch from the inserted point.
///
/// Modifier keys:
/// SHIFT: when adding a new point, snap it to the grid. Doesn't apply when branching from or joining to existing points or paths.
/// SHIFT: when dragging a handle, snap it to horizontal, vertical, or diagonal, whichever is closest
/// CTRL: when dragging a handle, snap its end to the grid instead
/// ALT: when releasing a dragged point, make it broken. It still acts continuous while dragging, so it gets a handle on both sides.
///      Doesn't apply to points synced with the points we branched from or joined to.
/// ALT: when clicking a point or path, copy its zone's normal as the normal for new zones, and for the zone being drawn.
#[derive(Clone)]
pub struct EditorToolPen {
    /// The zone that is being created.
    creating_zone: Option<u16>,

    /// The latest point that was created.
    latest_point: Option<u16>,

    // Points in the zone we are creating. Only used for selection.
    zone_points: Vec<u16>,

    /// The handle we are manipulating, if any. True == right handle.
    handle: Option<bool>,

    /// ID of the point we are branching off of if we are creating a connected zone
    branch_point_id: Option<u16>,

    /// Only used when joining multiple source paths.
    joined_source: bool,

    /// If dragging a point sets the handle to the opposite position. Only used when rejoining to a broken path.
    dragging_reversed: bool,

    /// The normal vector that new zones get.
    normal: Vec3,

    /// If zones that branch off of another zone start with that zone's normal, instead of the normal for new zones.
    inherit_normal: bool,

    /// If the pen stays active after finishing a zone, instead of selecting the zone in the zone tool.
    stay_active: bool,
}

impl Default for EditorToolPen {
    fn default() -> Self {
        Self {
            creating_zone: None,
            latest_point: None,
            zone_points: vec![],
            handle: None,
            branch_point_id: None,
            joined_source: false,
            dragging_reversed: false,
            normal: Vec3::FORWARD,
            inherit_normal: true,
            stay_active: false,
        }
    }
}

impl EditorTool for EditorToolPen {
    fn init() -> Box<Self> where Self : Sized { Box::default() }

    fn select(_selection: SelectionType, _state: ControllerStateInput) -> Box<Self> where Self : Sized { Box::default() }

    fn deselect(&mut self, mut state: ControllerStateInput) -> SelectionType<'_> {
        // If we are creating a zone, delete the whole zone.
        if let Some(zone_id) = self.creating_zone {
            state.objects.delete_zone(zone_id).unwrap();
            self.creating_zone = None;
            self.latest_point = None;
            self.handle = None;
            self.branch_point_id = None;
            self.zone_points.clear();
        }
        self.selection() // empty if the zone was deleted, otherwise the final point
    }

    fn kind(&self) -> EditorToolKind { EditorToolKind::Pen }

    fn creating_path(&self) -> Option<bool> {
        self.creating_zone?;
        Some(self.latest_point.is_some() && self.latest_point == self.zone_points.first().copied())
    }

    fn selection(&self) -> SelectionType<'_> { &self.zone_points[..] }

    fn handle_click(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2, modifiers: InputModifiers) -> ToolResult {
        // Alt: copy the normal of the clicked zone instead of adding a point
        if modifiers.alt {
            let zone_id = match utils::get_clicked_control_point(&state, None, pos)? {
                Some((zone_id, _)) => Some(zone_id),
                None => utils::get_clicked_zone_path(&state, pos)?.map(|(zone_id, ..)| zone_id),
            };
            if let Some(zone_id) = zone_id {
                let normal = state.objects.get_zone_info(zone_id)?.normal();
                return self.set_normal(&mut state, normal);
            }
            return self.ok();
        }

        self.handle_create_point(state, pos, false, modifiers)
    }

    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: studio_math::Vec2, modifiers: InputModifiers) -> ToolResult {
        self.dragging_reversed = false;
        self.handle_create_point(state, pos, true, modifiers)
    }

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2, modifiers: InputModifiers) -> ToolResult {
        // Assume we are dragging the handle of the currently selected point
        if let Some(point_id) = self.latest_point {
            let point = state.objects.get_point_info(point_id)?;
            let point_pos = point.position();
            let (first_point_id, _) = state.objects.get_zone_info(self.creating_zone.unwrap())?.range();
            let handle = if !self.dragging_reversed { pos - point_pos } else { point_pos - pos };

            // Ctrl snaps the end of the handle to the grid, otherwise shift snaps its direction
            let snap_modifiers = if modifiers.ctrl { InputModifiers::ctrl() } else if modifiers.shift { InputModifiers::shift() } else { InputModifiers::none() };
            let handle_end = point_pos + handle;
            let handle = utils::snap_drag_target(handle_end, point_pos, point_pos, handle_end, snap_modifiers) - point_pos;

            let current = if self.handle.unwrap() { point.right_handle() } else { point.left_handle() } - point_pos;
            let handle = utils::enforce_handle_length(handle, current);
            let (lh, rh) = if point_id == first_point_id {
                if self.handle.unwrap() { (None, Some(handle)) } else { (Some(handle), None) }
            } else {
                if self.handle.unwrap() { (Some(-handle), Some(handle)) } else { (Some(handle), Some(-handle)) }
            };
            state.objects.update_point(point_id, None, None, lh, rh)?;
        }
        self.ok()
    }

    fn handle_drag_released(&mut self, mut state: ControllerStateInput, modifiers: InputModifiers) -> ToolResult {
        // Alt: the point acted continuous while dragging, but should be broken from now on.
        // Only applies to points that don't sync to other points, so it doesn't spread to the points we branched from or joined to.
        if modifiers.alt && self.handle.is_some() && let Some(point_id) = self.latest_point {
            let point = state.objects.get_point_info(point_id)?;
            if point.mode() == ControlPointMode::Continuous && !point.self_syncs_in(point_id, false) && !point.self_syncs_in(point_id, true) {
                state.objects.update_point(point_id, None, Some(ControlPointMode::Broken), None, None)?;
            }
        }

        self.dragging_reversed = false;
        self.handle = None;

        // If the path has more than one point, and some other condition is met, finalize the path creation
        if let Some(zone_id) = self.creating_zone && let Some(point_id) = self.latest_point {
            let (start_point, point_count) = state.objects.get_zone_info(zone_id)?.range();
            if point_count > 1 {
                // Finalize path if this is the first point or we are joining to another path
                if point_id == start_point || self.joined_source {
                    return self.path_complete(&state);
                }
            }
        }
        self.ok()
    }

    fn handle_cancel(&mut self, mut state: ControllerStateInput) -> ToolResult {
        // If we are creating a zone, delete the whole zone.
        // It may have been assigned if its normal was set, so normals are stale where it was.
        // It is the last zone, so deleting it doesn't change any other zone IDs.
        if let Some(zone_id) = self.creating_zone {
            let bounds = state.objects.zone_bounds(zone_id)?;
            state.objects.delete_zone(zone_id)?;
            self.creating_zone = None;
            self.latest_point = None;
            self.handle = None;
            self.branch_point_id = None;
            self.zone_points.clear();
            return self.stale_in(bounds);
        }
        self.ok()
    }

    fn handle_delete(&mut self, mut state: ControllerStateInput) -> ToolResult {
        // Like cancelling, the zone may have been assigned if its normal was set, so normals are stale where it was.
        if let Some(zone_id) = self.creating_zone {
            let bounds = state.objects.zone_bounds(zone_id)?;
            if state.objects.delete_point_in_wip_path(self.latest_point.unwrap())? {
                // The path is now empty, the whole zone was deleted.
                self.creating_zone = None;
                self.latest_point = None;
                self.branch_point_id = None;
                self.zone_points.clear();
            } else {
                self.zone_points.pop(); // the deleted point
                self.latest_point = self.zone_points.last().copied();
            }
            self.handle = None;
            return self.stale_in(bounds);
        }
        self.ok()
    }

    fn handle_special_action(&mut self, mut state: ControllerStateInput, action: SpecialAction) -> ToolResult {
        match action {
            SpecialAction::NormalSet(normal, _) => self.set_normal(&mut state, normal),
            SpecialAction::ToggleInherit => {
                self.inherit_normal = !self.inherit_normal;
                self.ok()
            },
            SpecialAction::ToggleStayActive => {
                self.stay_active = !self.stay_active;
                self.ok()
            },
            _ => self.ok(),
        }
    }

    fn handle_special_query(&self, query: SpecialQuery) -> Option<SpecialValue> {
        match query {
            SpecialQuery::Normal => Some(SpecialValue::Normal(self.normal)),
            SpecialQuery::InheritNormal => Some(SpecialValue::Flag(self.inherit_normal)),
            SpecialQuery::StayActive => Some(SpecialValue::Flag(self.stay_active)),
        }
    }
}

impl EditorToolPen {
    /// Return a result for when the path is complete and we should move to another zone.
    /// If the pen stays active, get ready to create another zone instead.
    /// Normals are stale around the finished zone and the zones it branched from or joined to.
    fn path_complete(&mut self, state: &ControllerStateInput) -> ToolResult {
        let bounds = state.objects.bounds_around_points(&self.zone_points)?;
        self.creating_zone = None; // finished, so deselecting must not delete it
        if self.stay_active {
            self.latest_point = None;
            self.zone_points.clear();
            self.handle = None;
            self.branch_point_id = None;
            self.joined_source = false;
            self.dragging_reversed = false;
            return Ok(EditorToolActionResult::new(EditorToolKind::Pen, bounds));
        }
        Ok(EditorToolActionResult::new(EditorToolKind::Zone, bounds))
    }

    /// Set the normal for new zones, and for the zone being drawn if there is one.
    fn set_normal(&mut self, state: &mut ControllerStateInput, normal: Vec3) -> ToolResult {
        self.normal = normal;
        if let Some(zone_id) = self.creating_zone {
            state.objects.set_zone_normals(&[zone_id], normal)?;
            return self.stale_in(state.objects.zone_bounds(zone_id)?);
        }
        self.ok()
    }

    /// Set the normal of a zone we just started creating.
    /// Zones branching off of a point in another zone get that zone's normal if inheriting it, and everything else gets the normal for new zones.
    fn set_new_zone_normal(&self, state: &mut ControllerStateInput, zone_id: u16, parent_point_id: Option<u16>) -> anyhow::Result<()> {
        let normal = match parent_point_id {
            Some(point_id) if self.inherit_normal => {
                let parent_zone_id = state.objects.get_point_info(point_id)?.zone_id();
                state.objects.get_zone_info(parent_zone_id)?.normal()
            },
            _ => self.normal,
        };
        state.objects.set_zone_normals(&[zone_id], normal)
    }
    /// Return a result for when the path is joined to another path, and we should select it
    /// Normals are stale around the joined zone and the zones it is connected to.
    fn path_joined(&mut self, state: &ControllerStateInput) -> ToolResult {
        let bounds = state.objects.bounds_around_points(&self.zone_points)?;
        self.creating_zone = None; // finished, so deselecting must not delete it
        Ok(EditorToolActionResult::new(EditorToolKind::Point, bounds))
    }

    /// Join the zone we are creating to a point connected to the point we branched off of.
    /// If not dragging this completes the path, otherwise we start dragging the joined point's free handle.
    fn join_branch(&mut self, mut state: ControllerStateInput, zone_id: u16, source_id: u16, point_id: u16, dragging: bool) -> ToolResult {
        let created_ids = state.objects.complete_branching_zone(source_id, zone_id, point_id)?;
        // If nothing was created, we have joined our original point.
        let point_id = if created_ids.is_empty() {
            state.objects.get_zone_info(zone_id)?.range().0
        } else { created_ids[0] };

        self.latest_point = Some(point_id);
        self.zone_points.push(point_id);

        // If not dragging just select the point.
        if !dragging {
            return self.path_complete(&state);
        }

        // Otherwise, determine which handle to drag
        else {
            let right = {
                let point = state.objects.get_point_info(point_id)?;
                point.self_syncs_in(point_id, false)
            };

            self.handle = Some(right);
            self.joined_source = true;
            self.dragging_reversed = true;
        }
        self.ok()
    }

    /// Helper method to handle creating a point. Used by both handle_click and handle_drag_start.
    fn handle_create_point(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2, dragging: bool, modifiers: InputModifiers) -> ToolResult {
        // Shift snaps newly added points to the grid
        let new_point_pos = if modifiers.shift { utils::snap_to_grid(pos) } else { pos };

        // If we have a zone selected, we are already creating one, so add a linear node
        if let Some(zone_id) = self.creating_zone {

            // If we clicked a point, see if we should end the path.
            if let Some((other_zone_id, point_id)) = utils::get_clicked_control_point(&state, None, pos)? {

                // If it's the same zone, only do anything if we clicked the first point
                if other_zone_id == zone_id {
                    let (first_point, count) = state.objects.get_zone_info(zone_id)?.range();
                    if point_id == first_point && count > 1 {
                        let point = state.objects.get_point_info(point_id)?;
                        self.latest_point = Some(point_id);
                        self.zone_points.push(point_id);

                        // Do different things if we're dragging or not
                        if !dragging {
                            // If the point is continuous, update it to be broken and make its left handle linear
                            if point.mode() == ControlPointMode::Continuous {
                                state.objects.update_point(
                                    point_id, 
                                    None,
                                    Some(ControlPointMode::Broken),
                                    Some(Vec2::ZERO),
                                    None
                                )?;
                            }

                            // Go to the Point tool. Should keep the zone and point selected.
                            return self.path_complete(&state);
                        }

                        else {
                            // If the point is linear, update it to be broken and select it. We will start dragging its left handle.
                            if point.mode() == ControlPointMode::Linear {
                                state.objects.update_point(point_id, None, Some(ControlPointMode::Broken), None, None)?;
                            }
                            self.handle = Some(false);
                            self.dragging_reversed = true;
                        }
                    }
                }

                // Otherwise, join the path if a path can be made to our original point via sibling points
                else if let Some(source_id) = self.branch_point_id {
                    return self.join_branch(state, zone_id, source_id, point_id, dragging);
                }
            }

            // If we are branching and clicked another zone's path, insert a point into it and join to that
            else if let Some(source_id) = self.branch_point_id
                && let Some((path_zone_id, start_id, _, t)) = utils::get_clicked_zone_path(&state, pos)?
                && path_zone_id != zone_id {
                // Check first, so we don't insert a point we can't join to
                if !state.objects.check_points_connected(source_id, start_id)? {
                    anyhow::bail!("Cannot join control points in disconnected zones! Make the paths close together and use the merge tool.");
                }
                let inserted_ids = state.objects.insert_point(path_zone_id, start_id, t)?;

                // Inserting pushes back the IDs of every point after the inserted ones, including the ones we're keeping track of
                self.latest_point = self.latest_point.map(|point_id| shift_id_after_insert(point_id, &inserted_ids));
                for point_id in &mut self.zone_points { *point_id = shift_id_after_insert(*point_id, &inserted_ids); }
                let source_id = shift_id_after_insert(source_id, &inserted_ids);
                self.branch_point_id = Some(source_id);

                return self.join_branch(state, zone_id, source_id, inserted_ids[0], dragging);
            }

            // Otherwise just add a new point to the selected zone
            else {
                let point_id = state.objects.create_point(zone_id, new_point_pos)?;
                self.latest_point = Some(point_id);
                self.zone_points.push(point_id);

                // Start dragging it if we are dragging
                if dragging {
                    state.objects.update_point(point_id, None, Some(ControlPointMode::Continuous), None, None)?;
                    self.handle = Some(true);
                }
            }
        }

        // Otherwise, we should try to create a zone by adding a linear point
        else {
            // If we clicked a point, start a path with a sibling node
            if let Some((_, sibling_id)) = utils::get_clicked_control_point(&state, None, pos)? {
                let (zone_id, point_id) = state.objects.create_branching_zone(sibling_id)?;
                self.set_new_zone_normal(&mut state, zone_id, Some(sibling_id))?;
                self.creating_zone = Some(zone_id);
                self.latest_point = Some(point_id);
                self.zone_points.push(point_id);
                self.branch_point_id = Some(sibling_id);
                if dragging { self.handle = Some(true); }
            }

            // If we clicked a path, add a vertex to that zone
            else if let Some((zone_id, start_id, _, t)) = utils::get_clicked_zone_path(&state, pos)? {
                // Add the point, which should also fix its handles
                let inserted_id = state.objects.insert_point(zone_id, start_id, t)?[0];

                // If dragging, start a path branching off of the inserted point
                if dragging {
                    let (zone_id, point_id) = state.objects.create_branching_zone(inserted_id)?;
                    self.set_new_zone_normal(&mut state, zone_id, Some(inserted_id))?;
                    self.creating_zone = Some(zone_id);
                    self.latest_point = Some(point_id);
                    self.zone_points.push(point_id);
                    self.branch_point_id = Some(inserted_id);
                    self.handle = Some(true);
                }

                // Otherwise select it and move to the Point tool
                else {
                    self.creating_zone = Some(zone_id);
                    self.latest_point = Some(inserted_id);
                    self.zone_points.push(inserted_id);
                    return self.path_joined(&state);
                }
            }

            // Otherwise start creating a new path.
            else {
                let zone_id = state.objects.create_zone()?;
                self.set_new_zone_normal(&mut state, zone_id, None)?;
                self.creating_zone = Some(zone_id);
                let point_id = state.objects.create_point(zone_id, new_point_pos)?;
                self.latest_point = Some(point_id);
                self.zone_points.push(point_id);

                if dragging {
                    state.objects.update_point(point_id, None, Some(ControlPointMode::Continuous), None, None)?;
                    self.handle = Some(true);
                }
            }
        }
        self.ok()
    }
}
/// Get the new ID of a point after other points were inserted, since inserting a point pushes back every point after it.
/// `inserted_ids` are the IDs of the inserted points, after inserting.
fn shift_id_after_insert(mut point_id: u16, inserted_ids: &[u16]) -> u16 {
    let mut inserted_ids = inserted_ids.to_vec();
    inserted_ids.sort();
    for inserted_id in inserted_ids {
        if inserted_id <= point_id { point_id += 1; }
    }
    point_id
}


#[cfg(test)]
mod tests {
    use crate::{ObjectBuffers, InputModifiers as IM};
    use crate::objects::test_utils::{add_square, add_zone};

    use super::*;
    use super::super::test_utils::state;

    /// Click three points far apart to start a path
    fn start_triangle(tool: &mut EditorToolPen, objects: &ObjectBuffers) {
        tool.handle_click(state(objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
        tool.handle_click(state(objects), Vec2::new(50.0, 0.0), Default::default()).unwrap();
        tool.handle_click(state(objects), Vec2::new(50.0, 50.0), Default::default()).unwrap();
    }

    /// A tool that is not creating anything
    #[test]
    fn init() {
        let tool = EditorToolPen::init();
        assert_eq!(tool.kind(), EditorToolKind::Pen);
        assert!(tool.selection().is_empty());
        assert_eq!(tool.creating_path(), None);
    }

    /// Clicking empty space starts a new zone, and further clicks add points to it
    #[test]
    fn click_creates_points() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();

        let result = tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Pen);
        assert_eq!(objects.object_counts(), (1, 1));
        assert_eq!(tool.creating_path(), Some(true)); // the latest point is the first point

        tool.handle_click(state(&objects), Vec2::new(50.0, 0.0), Default::default()).unwrap();
        assert_eq!(tool.creating_path(), Some(false));
        tool.handle_click(state(&objects), Vec2::new(50.0, 50.0), Default::default()).unwrap();
        assert_eq!(objects.object_counts(), (1, 3));
        assert_eq!(tool.selection(), &[0, 1, 2]);
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(50.0, 50.0));
    }

    /// Clicking the first point completes the path and moves to the zone tool
    #[test]
    fn click_first_point_completes_path() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        start_triangle(&mut tool, &objects);

        let result = tool.handle_click(state(&objects), Vec2::new(1.0, 1.0), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert_eq!(result.normals_stale, Some((Vec2::ZERO, Vec2::new(50.0, 50.0))));
        assert_eq!(objects.object_counts(), (1, 3));

        // Deselecting should not delete the finished zone
        tool.deselect(state(&objects));
        assert_eq!(objects.object_counts(), (1, 3));
    }

    /// Clicking the first point when there is only one point doesn't finish anything
    #[test]
    fn click_first_point_needs_more_points() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();

        let result = tool.handle_click(state(&objects), Vec2::new(1.0, 1.0), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Pen);
        assert_eq!(objects.point_count(), 1);
    }

    /// Dragging out a new point makes it continuous and drags its handle
    #[test]
    fn drag_creates_continuous_point() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 10.0), Default::default()).unwrap();
        assert_eq!(objects.get_point_info(0).unwrap().mode(), ControlPointMode::Continuous);
        assert_eq!(tool.handle, Some(true));

        tool.handle_dragging_to(state(&objects), Vec2::new(30.0, 10.0), Default::default()).unwrap();
        let point = objects.get_point_info(0).unwrap();
        assert_eq!(point.right_handle(), Vec2::new(30.0, 10.0));

        tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(tool.handle, None);
    }

    /// Dragging a later point sets both handles opposite to each other
    #[test]
    fn drag_sets_opposite_handles() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
        tool.handle_drag_start(state(&objects), Vec2::new(50.0, 0.0), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(60.0, 5.0), Default::default()).unwrap();

        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.right_handle(), Vec2::new(60.0, 5.0));
        assert_eq!(point.left_handle(), Vec2::new(40.0, -5.0));
    }

    /// Dragging a handle shorter than the minimum length pushes it out to the minimum
    #[test]
    fn drag_handle_minimum_length() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
        tool.handle_drag_start(state(&objects), Vec2::new(50.0, 0.0), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(50.0, 0.5), Default::default()).unwrap();

        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.right_handle(), Vec2::new(50.0, 2.0));
        assert_eq!(point.left_handle(), Vec2::new(50.0, -2.0));
    }

    /// Dragging off of the first point closes the path, and releasing finishes it
    #[test]
    fn drag_first_point_closes_path() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        start_triangle(&mut tool, &objects);

        tool.handle_drag_start(state(&objects), Vec2::new(1.0, 1.0), Default::default()).unwrap();
        assert_eq!(tool.creating_path(), Some(true));
        assert_eq!(objects.get_point_info(0).unwrap().mode(), ControlPointMode::Broken);

        let result = tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert!(result.normals_stale.is_some());
    }

    /// Releasing a drag on the first point of a path with a single point doesn't finish it
    #[test]
    fn release_needs_more_points() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        tool.handle_drag_start(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();

        let result = tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Pen);
    }

    /// Cancelling throws away the path being created
    #[test]
    fn cancel_deletes_path() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        start_triangle(&mut tool, &objects);

        let result = tool.handle_cancel(state(&objects)).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Pen);
        assert_eq!(objects.object_counts(), (0, 0));
        assert_eq!(tool.creating_path(), None);
        assert!(tool.selection().is_empty());

        // The path may have been assigned if its normal was set, so normals are stale where it was
        assert_eq!(result.normals_stale, Some((Vec2::ZERO, Vec2::new(50.0, 50.0))));
    }

    /// Deselecting throws away an unfinished path
    #[test]
    fn deselect_deletes_path() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        start_triangle(&mut tool, &objects);

        assert!(tool.deselect(state(&objects)).is_empty());
        assert_eq!(objects.object_counts(), (0, 0));
    }

    /// Deleting removes the latest point, and the zone with it if it was the only one
    #[test]
    fn delete_latest_point() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        start_triangle(&mut tool, &objects);

        tool.handle_delete(state(&objects)).unwrap();
        assert_eq!(objects.object_counts(), (1, 2));
        assert_eq!(tool.selection(), &[0, 1]);

        tool.handle_delete(state(&objects)).unwrap();
        tool.handle_delete(state(&objects)).unwrap();
        assert_eq!(objects.object_counts(), (0, 0));
        assert_eq!(tool.creating_path(), None);
    }

    /// Deleting when there is no path does nothing
    #[test]
    fn delete_nothing() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolPen::init();
        tool.handle_delete(state(&objects)).unwrap();
        assert_eq!(objects.object_counts(), (1, 4));
    }

    /// Clicking an existing path adds a point to it, and moves to the point tool
    #[test]
    fn click_path_inserts_point() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 100.0);
        let mut tool = EditorToolPen::init();

        let result = tool.handle_click(state(&objects), Vec2::new(50.0, 1.0), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Point);
        assert!(result.normals_stale.is_some());
        assert_eq!(objects.point_count(), 5);
        assert_eq!(tool.selection(), &[1]);

        // Deselecting should not delete the zone we added to
        tool.deselect(state(&objects));
        assert_eq!(objects.zone_count(), 1);
    }

    /// Clicking an existing point starts a zone branching off of it
    #[test]
    fn click_point_branches() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 100.0);
        let mut tool = EditorToolPen::init();

        tool.handle_click(state(&objects), Vec2::new(100.0, 0.0), Default::default()).unwrap();
        assert_eq!(objects.object_counts(), (2, 5));
        assert_eq!(tool.creating_path(), Some(true));
        assert_eq!(objects.get_sibling(1).unwrap(), 4);
    }

    /// Branching off of a point then clicking another one joins the zones together
    #[test]
    fn click_second_point_joins() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 100.0);
        let mut tool = EditorToolPen::init();

        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
        tool.handle_click(state(&objects), Vec2::new(-100.0, 50.0), Default::default()).unwrap();
        let result = tool.handle_click(state(&objects), Vec2::new(0.0, 100.0), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert!(result.normals_stale.is_some());
        assert_eq!(objects.zone_count(), 2);
        assert_eq!(objects.get_zone_info(1).unwrap().range().1, 3);
    }

    /// Shift snaps new points to the grid, but not points joined to or branched off of existing ones
    #[test]
    fn shift_click_snaps_to_grid() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        tool.handle_click(state(&objects), Vec2::new(10.4, 20.6), IM::shift()).unwrap();
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(10.0, 21.0));
        tool.handle_drag_start(state(&objects), Vec2::new(50.6, 0.2), IM::shift()).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(51.0, 0.0));

        // Branching off of a point that isn't on the grid starts exactly on that point
        let mut objects = ObjectBuffers::headless();
        add_zone(&mut objects, &[(0.0, 0.0), (100.3, 0.0), (100.0, 100.0)]);
        let mut tool = EditorToolPen::init();
        tool.handle_click(state(&objects), Vec2::new(100.0, 0.0), IM::shift()).unwrap();
        assert_eq!(objects.object_counts(), (2, 4));
        assert_eq!(objects.get_sibling(1).unwrap(), 3);
        assert_eq!(objects.get_point_info(3).unwrap().position(), Vec2::new(100.3, 0.0));
    }

    /// Shift snaps a dragged handle to horizontal, vertical, or diagonal
    #[test]
    fn drag_handle_shift_snaps_direction() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
        tool.handle_drag_start(state(&objects), Vec2::new(50.0, 0.0), Default::default()).unwrap();

        tool.handle_dragging_to(state(&objects), Vec2::new(60.0, 3.0), IM::shift()).unwrap();
        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.right_handle(), Vec2::new(60.0, 0.0));
        assert_eq!(point.left_handle(), Vec2::new(40.0, 0.0));

        // Diagonal: moved (+4, -6), which is closer to the diagonal than the vertical
        tool.handle_dragging_to(state(&objects), Vec2::new(54.0, -6.0), IM::shift()).unwrap();
        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.right_handle(), Vec2::new(55.0, -5.0));
        assert_eq!(point.left_handle(), Vec2::new(45.0, 5.0));
    }

    /// Ctrl snaps the end of a dragged handle to the grid, and takes priority over shift
    #[test]
    fn drag_handle_ctrl_snaps_to_grid() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
        tool.handle_drag_start(state(&objects), Vec2::new(50.0, 0.0), Default::default()).unwrap();

        tool.handle_dragging_to(state(&objects), Vec2::new(60.4, 3.6), IM::ctrl()).unwrap();
        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.right_handle(), Vec2::new(60.0, 4.0));
        assert_eq!(point.left_handle(), Vec2::new(40.0, -4.0));

        tool.handle_dragging_to(state(&objects), Vec2::new(60.4, 3.6), IM::ctrl_shift()).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().right_handle(), Vec2::new(60.0, 4.0));
    }

    /// Releasing a drag with alt makes the point broken, keeping the handles it got from acting continuous
    #[test]
    fn alt_release_breaks_point() {
        for (modifiers, mode) in [(IM::alt(), ControlPointMode::Broken), (IM::none(), ControlPointMode::Continuous)] {
            let objects = ObjectBuffers::headless();
            let mut tool = EditorToolPen::init();
            tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
            tool.handle_drag_start(state(&objects), Vec2::new(50.0, 0.0), Default::default()).unwrap();
            tool.handle_dragging_to(state(&objects), Vec2::new(60.0, 5.0), Default::default()).unwrap();
            tool.handle_drag_released(state(&objects), modifiers).unwrap();

            let point = objects.get_point_info(1).unwrap();
            assert_eq!(point.mode(), mode);
            assert_eq!(point.right_handle(), Vec2::new(60.0, 5.0));
            assert_eq!(point.left_handle(), Vec2::new(40.0, -5.0));
        }
    }

    /// Releasing a drag with alt doesn't change points synced to the zone we branched off of
    #[test]
    fn alt_release_skips_branch_points() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 100.0);
        let mut tool = EditorToolPen::init();

        tool.handle_drag_start(state(&objects), Vec2::new(100.0, 0.0), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(120.0, 10.0), Default::default()).unwrap();
        tool.handle_drag_released(state(&objects), IM::alt()).unwrap();

        assert_eq!(objects.get_point_info(4).unwrap().mode(), ControlPointMode::Broken);
        assert_eq!(objects.get_point_info(4).unwrap().right_handle(), Vec2::new(120.0, 10.0));
        let parent = objects.get_point_info(1).unwrap();
        assert_eq!(parent.mode(), ControlPointMode::Linear);
        assert_eq!(parent.left_handle(), parent.position());
        assert_eq!(parent.right_handle(), parent.position());
    }

    /// Dragging on a path inserts a point into it and starts a zone branching off of that point
    #[test]
    fn drag_path_starts_branch() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 100.0);
        let mut tool = EditorToolPen::init();

        let result = tool.handle_drag_start(state(&objects), Vec2::new(50.0, 1.0), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Pen);
        assert_eq!(objects.object_counts(), (2, 6));
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(50.0, 0.0));
        assert_eq!(objects.get_sibling(1).unwrap(), 5);
        assert_eq!(objects.get_point_info(5).unwrap().zone_id(), 1);
        assert_eq!(tool.selection(), &[5]);

        // The new point's handle is dragged
        tool.handle_dragging_to(state(&objects), Vec2::new(50.0, -20.0), Default::default()).unwrap();
        assert_eq!(objects.get_point_info(5).unwrap().right_handle(), Vec2::new(50.0, -20.0));
        let result = tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Pen);

        // Joining back to the square works, so we're tracking the right source point
        tool.handle_click(state(&objects), Vec2::new(150.0, -50.0), Default::default()).unwrap();
        let result = tool.handle_click(state(&objects), Vec2::new(100.0, 0.0), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert_eq!(objects.zone_count(), 2);
        assert_eq!(objects.get_zone_info(0).unwrap().range(), (0, 5));
    }

    /// Clicking another zone's path while branching inserts a point into it and joins to that point
    #[test]
    fn click_path_joins_branch() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 100.0);
        let mut tool = EditorToolPen::init();

        tool.handle_click(state(&objects), Vec2::new(100.0, 0.0), Default::default()).unwrap();
        tool.handle_click(state(&objects), Vec2::new(150.0, -50.0), Default::default()).unwrap();
        let result = tool.handle_click(state(&objects), Vec2::new(101.0, 50.0), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert!(result.normals_stale.is_some());
        assert_eq!(objects.zone_count(), 2);

        // The square got a point on its right edge, and the IDs of the branching zone moved back to make room
        assert_eq!(objects.get_zone_info(0).unwrap().range(), (0, 5));
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(100.0, 50.0));
        let (start, count) = objects.get_zone_info(1).unwrap().range();
        assert_eq!(start, 5);
        assert_eq!(count, 3);

        // The branching zone starts at the original corner, and ends at the inserted point
        assert_eq!(objects.sibling_in_zone(1, 1).map(|id| objects.get_point_info(id).unwrap().position()), Some(Vec2::new(100.0, 0.0)));
        assert_eq!(objects.sibling_in_zone(2, 1).map(|id| objects.get_point_info(id).unwrap().position()), Some(Vec2::new(100.0, 50.0)));
        assert!((start..start + count).any(|id| objects.get_point_info(id).unwrap().position() == Vec2::new(150.0, -50.0)));
    }

    /// Dragging another zone's path while branching inserts a point, joins to it, and drags its free handle
    #[test]
    fn drag_path_joins_branch() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 100.0);
        let mut tool = EditorToolPen::init();

        tool.handle_click(state(&objects), Vec2::new(100.0, 0.0), Default::default()).unwrap();
        tool.handle_click(state(&objects), Vec2::new(150.0, -50.0), Default::default()).unwrap();
        let result = tool.handle_drag_start(state(&objects), Vec2::new(101.0, 50.0), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Pen);
        assert_eq!(objects.get_zone_info(0).unwrap().range(), (0, 5));
        let join_id = objects.sibling_in_zone(2, 1).unwrap();

        // The handle is dragged reversed, so it points away from the mouse
        tool.handle_dragging_to(state(&objects), Vec2::new(120.0, 60.0), Default::default()).unwrap();
        let point = objects.get_point_info(join_id).unwrap();
        assert!(point.left_handle() == Vec2::new(80.0, 40.0) || point.right_handle() == Vec2::new(80.0, 40.0));

        let result = tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert_eq!(objects.zone_count(), 2);
    }

    /// Clicking a path that isn't connected to the zone we branched off of is an error, and doesn't change the path
    #[test]
    fn click_disconnected_path_errors() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 100.0);
        add_zone(&mut objects, &[(200.0, 0.0), (300.0, 0.0), (300.0, 100.0), (200.0, 100.0)]);
        let mut tool = EditorToolPen::init();

        tool.handle_click(state(&objects), Vec2::new(100.0, 0.0), Default::default()).unwrap();
        tool.handle_click(state(&objects), Vec2::new(150.0, -50.0), Default::default()).unwrap();
        assert!(tool.handle_click(state(&objects), Vec2::new(250.0, 1.0), Default::default()).is_err());
        assert_eq!(objects.get_zone_info(1).unwrap().range(), (4, 4));
        assert_eq!(objects.point_count(), 10);

        // Same as clicking a disconnected point
        assert!(tool.handle_click(state(&objects), Vec2::new(200.0, 0.0), Default::default()).is_err());
    }

    /// Point IDs are pushed back by every point inserted at or before them
    #[test]
    fn shift_ids_after_insert() {
        assert_eq!(shift_id_after_insert(3, &[5]), 3);
        assert_eq!(shift_id_after_insert(3, &[3]), 4);
        assert_eq!(shift_id_after_insert(3, &[9, 1]), 4);
        assert_eq!(shift_id_after_insert(3, &[4, 2]), 5); // 3 -> 4 after inserting 2, then 4 -> 5 after inserting 4
    }

    /// Make a square at (0, 0) with a size of 10, whose zone has an upward normal
    fn up_square() -> ObjectBuffers {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        objects.set_zone_normals(&[0], Vec3::UP).unwrap();
        objects
    }

    /// New zones get the normal for new zones
    #[test]
    fn new_zone_gets_pen_normal() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        tool.normal = Vec3::RIGHT;

        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
        assert_eq!(objects.get_zone_info(0).unwrap().normal(), Vec3::RIGHT);
    }

    /// Zones branching off of another zone start with that zone's normal by default
    #[test]
    fn branch_inherits_parent_normal() {
        let objects = up_square();
        let mut tool = EditorToolPen::init();
        tool.normal = Vec3::RIGHT;

        tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), Default::default()).unwrap();
        assert_eq!(objects.zone_count(), 2);
        assert_eq!(objects.get_zone_info(1).unwrap().normal(), Vec3::UP);
    }

    /// Zones branching off of another zone get the normal for new zones if not inheriting
    #[test]
    fn branch_uses_pen_normal_without_inherit() {
        let objects = up_square();
        let mut tool = EditorToolPen::init();
        tool.normal = Vec3::RIGHT;
        tool.inherit_normal = false;

        tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), Default::default()).unwrap();
        assert_eq!(objects.get_zone_info(1).unwrap().normal(), Vec3::RIGHT);
    }

    /// Setting the normal also sets it for the zone being drawn, even one that inherited its parent's normal
    #[test]
    fn normal_set_updates_drawing_zone() {
        let objects = up_square();
        let mut tool = EditorToolPen::init();
        tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), Default::default()).unwrap();

        let result = tool.handle_special_action(state(&objects), SpecialAction::NormalSet(Vec3::LEFT, false)).unwrap();
        assert_eq!(result.normals_stale, Some((Vec2::new(10.0, 0.0), Vec2::new(10.0, 0.0))));
        assert_eq!(tool.normal, Vec3::LEFT);
        assert_eq!(objects.get_zone_info(1).unwrap().normal(), Vec3::LEFT);
        assert_eq!(objects.get_zone_info(0).unwrap().normal(), Vec3::UP);
    }

    /// Alt clicking a point copies its zone's normal, without adding anything
    #[test]
    fn alt_click_point_copies_normal() {
        let objects = up_square();
        let mut tool = EditorToolPen::init();

        tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), IM::alt()).unwrap();
        assert_eq!(tool.normal, Vec3::UP);
        assert_eq!(objects.object_counts(), (1, 4));
        assert_eq!(tool.creating_path(), None);
    }

    /// Alt clicking a path copies its zone's normal
    #[test]
    fn alt_click_path_copies_normal() {
        let objects = up_square();
        let mut tool = EditorToolPen::init();

        tool.handle_click(state(&objects), Vec2::new(5.0, 0.0), IM::alt()).unwrap();
        assert_eq!(tool.normal, Vec3::UP);
        assert_eq!(objects.object_counts(), (1, 4));
    }

    /// Alt clicking empty space does nothing
    #[test]
    fn alt_click_empty_does_nothing() {
        let objects = up_square();
        let mut tool = EditorToolPen::init();

        tool.handle_click(state(&objects), Vec2::new(100.0, 100.0), IM::alt()).unwrap();
        assert_eq!(tool.normal, Vec3::FORWARD);
        assert_eq!(objects.object_counts(), (1, 4));
    }

    /// Alt clicking also sets the normal of the zone being drawn
    #[test]
    fn alt_click_updates_drawing_zone() {
        let objects = up_square();
        let mut tool = EditorToolPen::init();
        tool.handle_click(state(&objects), Vec2::new(100.0, 100.0), Default::default()).unwrap();
        tool.handle_click(state(&objects), Vec2::new(150.0, 100.0), Default::default()).unwrap();

        let result = tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), IM::alt()).unwrap();
        assert!(result.normals_stale.is_some());
        assert_eq!(objects.get_zone_info(1).unwrap().normal(), Vec3::UP);
        assert_eq!(objects.object_counts(), (2, 6));
    }

    /// If the pen stays active, finishing a zone keeps the pen tool and gets ready for the next zone
    #[test]
    fn stay_active_keeps_tool() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        tool.stay_active = true;
        start_triangle(&mut tool, &objects);

        let result = tool.handle_click(state(&objects), Vec2::new(1.0, 1.0), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Pen);
        assert_eq!(tool.creating_path(), None);
        assert!(tool.selection().is_empty());

        // The finished zone is kept, and clicking starts another one
        tool.handle_click(state(&objects), Vec2::new(200.0, 200.0), Default::default()).unwrap();
        assert_eq!(objects.object_counts(), (2, 4));
    }

    /// The flags can be toggled, and the UI can ask about them and the normal
    #[test]
    fn toggles_and_queries() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        assert_eq!(tool.handle_special_query(SpecialQuery::Normal), Some(SpecialValue::Normal(Vec3::FORWARD)));
        assert_eq!(tool.handle_special_query(SpecialQuery::InheritNormal), Some(SpecialValue::Flag(true)));
        assert_eq!(tool.handle_special_query(SpecialQuery::StayActive), Some(SpecialValue::Flag(false)));

        tool.handle_special_action(state(&objects), SpecialAction::ToggleInherit).unwrap();
        tool.handle_special_action(state(&objects), SpecialAction::ToggleStayActive).unwrap();
        assert_eq!(tool.handle_special_query(SpecialQuery::InheritNormal), Some(SpecialValue::Flag(false)));
        assert_eq!(tool.handle_special_query(SpecialQuery::StayActive), Some(SpecialValue::Flag(true)));
    }
}
