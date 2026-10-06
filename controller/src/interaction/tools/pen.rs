use studio_math::Vec2;

use crate::objects::ControlPointMode;

use super::{EditorTool, EditorToolKind, ControllerStateInput, SelectionType, utils};
use super::result::{ToolResult, DefaultResults, EditorToolActionResult};

/// The Pen tool allows creating a new zone by dragging out a bezier path.
#[derive(Default)]
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
}

impl EditorTool for EditorToolPen {
    fn init() -> Box<Self> where Self : Sized { Box::default() }

    fn select(_selection: SelectionType, _state: ControllerStateInput) -> Box<Self> where Self : Sized {
        Box::new(Self {
            creating_zone: None,
            latest_point: None,
            zone_points: vec![],
            handle: None,
            branch_point_id: None,
            joined_source: false,
            dragging_reversed: false,
        })
    }

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

    fn handle_click(&mut self, state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        self.handle_create_point(state, pos, false)
    }

    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        self.dragging_reversed = false;
        self.handle_create_point(state, pos, true)
    }

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // Assume we are dragging the handle of the currently selected point
        if let Some(point_id) = self.latest_point {
            let point_pos = state.objects.get_point_info(point_id).unwrap().position();
            let (first_point_id, _) = state.objects.get_zone_info(self.creating_zone.unwrap()).unwrap().range();
            let handle = if !self.dragging_reversed { pos - point_pos } else { point_pos - pos };
            let (lh, rh) = if point_id == first_point_id {
                if self.handle.unwrap() { (None, Some(handle)) } else { (Some(handle), None) }
            } else {
                if self.handle.unwrap() { (Some(-handle), Some(handle)) } else { (Some(handle), Some(-handle)) }
            };
            state.objects.update_point(point_id, None, None, lh, rh)?;
        }
        self.ok()
    }

    fn handle_drag_released(&mut self, state: ControllerStateInput) -> ToolResult {
        self.dragging_reversed = false;
        self.handle = None;

        // If the path has more than one point, and some other condition is met, finalize the path creation
        if let Some(zone_id) = self.creating_zone && let Some(point_id) = self.latest_point {
            let (start_point, point_count) = state.objects.get_zone_info(zone_id).unwrap().range();
            if point_count > 1 {
                // Finalize path if this is the first point or we are joining to another path
                if point_id == start_point || self.joined_source {
                    return self.path_complete();
                }
            }
        }
        self.ok()
    }

    fn handle_cancel(&mut self, mut state: ControllerStateInput) -> ToolResult {
        // If we are creating a zone, delete the whole zone.
        if let Some(zone_id) = self.creating_zone {
            state.objects.delete_zone(zone_id)?;
            self.creating_zone = None;
            self.latest_point = None;
            self.handle = None;
            self.branch_point_id = None;
            self.zone_points.clear();
        }
        self.ok()
    }

    fn handle_delete(&mut self, mut state: ControllerStateInput) -> ToolResult {
        if self.creating_zone.is_some() {
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
        }
        self.ok()
    }
}

impl EditorToolPen {
    /// Return a result for when the path is complete and we should move to another zone.
    fn path_complete(&mut self) -> ToolResult {
        self.creating_zone = None; // finished, so deselecting must not delete it
        Ok(EditorToolActionResult::new(EditorToolKind::Zone, true))
    }
    /// Return a result for when the path is joined to another path, and we should select it
    fn path_joined(&mut self) -> ToolResult {
        self.creating_zone = None; // finished, so deselecting must not delete it
        Ok(EditorToolActionResult::new(EditorToolKind::Point, true))
    }

    /// Helper method to handle creating a point. Used by both handle_click and handle_drag_start.
    fn handle_create_point(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2, dragging: bool) -> ToolResult {
        // If we have a zone selected, we are already creating one, so add a linear node
        if let Some(zone_id) = self.creating_zone {

            // If we clicked a point, see if we should end the path.
            if let Some((other_zone_id, point_id)) = utils::get_clicked_control_point(&state, None, pos) {

                // If it's the same zone, only do anything if we clicked the first point
                if other_zone_id == zone_id {
                    let (first_point, count) = state.objects.get_zone_info(zone_id).unwrap().range();
                    if point_id == first_point && count > 1 {
                        let point = state.objects.get_point_info(point_id).unwrap();
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
                            return self.path_complete();
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
                    let created_ids = state.objects.complete_branching_zone(source_id, zone_id, point_id)?;
                    // If nothing was created, we have joined our original point.
                    let point_id = if created_ids.is_empty() {
                        state.objects.get_zone_info(zone_id).unwrap().range().0
                    } else { created_ids[0] };

                    self.latest_point = Some(point_id);
                    self.zone_points.push(point_id);

                    // If not dragging just select the point.
                    if !dragging {
                        return self.path_complete();
                    }

                    // Otherwise, determine which handle to drag
                    else {
                        let right = {
                            let point = state.objects.get_point_info(point_id).unwrap();
                            point.self_syncs_in(point_id, false)
                        };

                        self.handle = Some(right);
                        self.joined_source = true;
                        self.dragging_reversed = true;
                    }
                }
            }

            // TODO: Try inserting into an existing path and then joining it. Shouldn't be too hard, just need to update the three selected things here to push them all back one

            // Otherwise just add a new point to the selected zone
            else {
                let point_id = state.objects.create_point(zone_id, pos)?;
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
            if let Some((_, sibling_id)) = utils::get_clicked_control_point(&state, None, pos) {
                let (zone_id, point_id) = state.objects.create_branching_zone(sibling_id)?;
                self.creating_zone = Some(zone_id);
                self.latest_point = Some(point_id);
                self.zone_points.push(point_id);
                self.branch_point_id = Some(sibling_id);
                if dragging { self.handle = Some(true); }
            }

            // If we clicked a path, add a vertex to that zone, select it, and move to Point tool
            else if let Some((zone_id, start_id, _, t)) = utils::get_clicked_zone_path(&state, pos) {
                // Add the point, which should also fix its handles
                let point_id = state.objects.insert_point(zone_id, start_id, t)?[0];

                // Select and return
                self.creating_zone = Some(zone_id);
                self.latest_point = Some(point_id);
                self.zone_points.push(point_id);
                return self.path_joined();
            }

            // Otherwise start creating a new path.
            else {
                let zone_id = state.objects.create_zone()?;
                self.creating_zone = Some(zone_id);
                let point_id = state.objects.create_point(zone_id, pos)?;
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


#[cfg(test)]
mod tests {
    use crate::ObjectBuffers;
    use crate::objects::test_utils::add_square;

    use super::*;
    use super::super::test_utils::state;

    /// Click three points far apart to start a path
    fn start_triangle(tool: &mut EditorToolPen, objects: &ObjectBuffers) {
        tool.handle_click(state(objects), Vec2::new(0.0, 0.0)).unwrap();
        tool.handle_click(state(objects), Vec2::new(50.0, 0.0)).unwrap();
        tool.handle_click(state(objects), Vec2::new(50.0, 50.0)).unwrap();
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

        let result = tool.handle_click(state(&objects), Vec2::new(0.0, 0.0)).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Pen);
        assert_eq!(objects.object_counts(), (1, 1));
        assert_eq!(tool.creating_path(), Some(true)); // the latest point is the first point

        tool.handle_click(state(&objects), Vec2::new(50.0, 0.0)).unwrap();
        assert_eq!(tool.creating_path(), Some(false));
        tool.handle_click(state(&objects), Vec2::new(50.0, 50.0)).unwrap();
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

        let result = tool.handle_click(state(&objects), Vec2::new(1.0, 1.0)).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert!(result.normals_stale);
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
        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0)).unwrap();

        let result = tool.handle_click(state(&objects), Vec2::new(1.0, 1.0)).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Pen);
        assert_eq!(objects.point_count(), 1);
    }

    /// Dragging out a new point makes it continuous and drags its handle
    #[test]
    fn drag_creates_continuous_point() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 10.0)).unwrap();
        assert_eq!(objects.get_point_info(0).unwrap().mode(), ControlPointMode::Continuous);
        assert_eq!(tool.handle, Some(true));

        tool.handle_dragging_to(state(&objects), Vec2::new(30.0, 10.0)).unwrap();
        let point = objects.get_point_info(0).unwrap();
        assert_eq!(point.right_handle(), Vec2::new(30.0, 10.0));

        tool.handle_drag_released(state(&objects)).unwrap();
        assert_eq!(tool.handle, None);
    }

    /// Dragging a later point sets both handles opposite to each other
    #[test]
    fn drag_sets_opposite_handles() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0)).unwrap();
        tool.handle_drag_start(state(&objects), Vec2::new(50.0, 0.0)).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(60.0, 5.0)).unwrap();

        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.right_handle(), Vec2::new(60.0, 5.0));
        assert_eq!(point.left_handle(), Vec2::new(40.0, -5.0));
    }

    /// Dragging off of the first point closes the path, and releasing finishes it
    #[test]
    fn drag_first_point_closes_path() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        start_triangle(&mut tool, &objects);

        tool.handle_drag_start(state(&objects), Vec2::new(1.0, 1.0)).unwrap();
        assert_eq!(tool.creating_path(), Some(true));
        assert_eq!(objects.get_point_info(0).unwrap().mode(), ControlPointMode::Broken);

        let result = tool.handle_drag_released(state(&objects)).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert!(result.normals_stale);
    }

    /// Releasing a drag on the first point of a path with a single point doesn't finish it
    #[test]
    fn release_needs_more_points() {
        let objects = ObjectBuffers::headless();
        let mut tool = EditorToolPen::init();
        tool.handle_drag_start(state(&objects), Vec2::new(0.0, 0.0)).unwrap();

        let result = tool.handle_drag_released(state(&objects)).unwrap();
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

        let result = tool.handle_click(state(&objects), Vec2::new(50.0, 1.0)).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Point);
        assert!(result.normals_stale);
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

        tool.handle_click(state(&objects), Vec2::new(100.0, 0.0)).unwrap();
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

        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0)).unwrap();
        tool.handle_click(state(&objects), Vec2::new(-100.0, 50.0)).unwrap();
        let result = tool.handle_click(state(&objects), Vec2::new(0.0, 100.0)).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert!(result.normals_stale);
        assert_eq!(objects.zone_count(), 2);
        assert_eq!(objects.get_zone_info(1).unwrap().range().1, 3);
    }
}
