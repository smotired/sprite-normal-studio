
use crate::InputModifiers;
use super::{EditorTool, EditorToolKind, ControllerStateInput, SelectionType, utils};
use super::result::{ToolResult, DefaultResults};

/// The Point tool allows managing individual points within a zone.
#[derive(Default)]
pub struct EditorToolPoint {
    /// The points that are selected.
    selected_points: Vec<u16>,

    /// The handle we are manipulating, if any. True == right handle.
    handle: Option<bool>,
}

impl EditorTool for EditorToolPoint {
    fn init() -> Box<Self> where Self : Sized { Box::default() }
    
    fn select(selected_points: SelectionType, _state: ControllerStateInput) -> Box<Self> where Self : Sized {
        Box::new(Self { selected_points: Vec::from(selected_points), handle: None })
    }

    fn deselect(&mut self, _state: ControllerStateInput) -> SelectionType<'_> {
        // No cleanup needed
        &self.selected_points[..]
    }

    fn kind(&self) -> EditorToolKind { EditorToolKind::Point }
    fn selection(&self) -> SelectionType<'_> { &self.selected_points[..] }

    fn handle_click(&mut self, state: ControllerStateInput, pos: studio_math::Vec2, _modifiers: InputModifiers) -> ToolResult {
        // TODO: Create helper like in zone for selecting with modifier keys

        // Reselect unless we clicked the current control point's handle
        let selected_point = self.single_selected_point();
        if selected_point.is_none() || utils::get_clicked_handle(selected_point.unwrap(), &state, pos).is_none()
        {
            // Selecting a path selects all of its points, so the "selected zone" is the one all selected points share.
            let selected_zone = self.single_selected_zone(&state);
            self.selected_points =
                if let Some((_, point_id)) = utils::get_clicked_control_point(&state, None, pos) {
                    // If the clicked point has a sibling in the selected zone, select it.
                    if let Some(selected_zone_id) = selected_zone && let Some(sibling_id) = state.objects.sibling_in_zone(point_id, selected_zone_id) {
                        // If the clicked point's sibling in the selected zone IS our selected point (i.e. we clicked our selected point), select its sibling.
                        if selected_point == Some(sibling_id) {
                            let next_sibling_id = state.objects.get_sibling(sibling_id)?;
                            vec![next_sibling_id]
                        }

                        // Otherwise just select the clicked sibling
                        else { vec![sibling_id] }
                    }
                    
                    // If the clicked point doesn't have a sibling in the selected zone, click what we selected.
                    else { vec![point_id] }
                } 
                
                // If we selected a path, select or cycle zones.
                else if let Some((zone_id, point_id, _, _)) = utils::get_clicked_zone_path(&state, pos) {
                    // If the start point has a sibling in the selected zone, possibly select that point's sibling's zone instead.
                    if let Some(selected_zone_id) = selected_zone && let Some(sibling_id) = state.objects.sibling_in_zone(point_id, selected_zone_id) {
                        let next_sibling_id = state.objects.get_sibling(sibling_id)?;
                        let next_zone_id = state.objects.get_point_info(next_sibling_id).unwrap().zone_id();

                        // Only select the next sibling if the full path is shared
                        let (start, count) = state.objects.get_zone_info(selected_zone_id).unwrap().range();
                        let path_end_id = start + (sibling_id - start + 1) % count;
                        if state.objects.sibling_in_zone(path_end_id, next_zone_id).is_some() {
                            points_in_zone(next_zone_id, &state)
                        }

                        // Otherwise this path is not shared at all
                        else { points_in_zone(zone_id, &state) }

                    } else {
                        points_in_zone(zone_id, &state)
                    }
                }

                // If there's nothing, select nothing
                else {
                    vec![]
                };
        }
        self.ok()
    }

    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: studio_math::Vec2, _modifiers: InputModifiers) -> ToolResult {
        // TODO: Use modifier keys to add to or remove from selection first

        // If we select a handle of the control point start dragging it
        if let Some(point_id) = self.single_selected_point() && let Some(right) = utils::get_clicked_handle(point_id, &state, pos) {
            self.handle = Some(right);
        } else {
            self.selected_points = 
                if let Some((_, point_id)) = utils::get_clicked_control_point(&state, None, pos) {
                    // If our points are only in one zone, check for a sibling in that zone
                    let selected_zones: Vec<_> = self.selected_points.iter()
                        .map(|point_id| state.objects.get_point_info(*point_id).unwrap().zone_id())
                        .collect::<std::collections::HashSet<u16>>() // get unique zone IDs
                        .into_iter().collect();

                    if selected_zones.len() == 1 && let Some(sibling_id) = state.objects.sibling_in_zone(point_id, selected_zones[0]) {
                        vec![sibling_id]
                    }
                    else { vec![point_id] }
                } else {
                    vec![]
                };
        }
        self.ok()
    }

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2, _modifiers: InputModifiers) -> ToolResult {
        // Drag selected points
        if !self.selected_points.is_empty() {
            // If we are dragging the handle of a single point, adjust that
            if self.selected_points.len() == 1 && let Some(right) = self.handle {
                let handle = Some(pos - state.objects.get_point_info(self.selected_points[0]).unwrap().position());
                let (lh, rh) = if right { (None, handle) } else { (handle, None) };
                state.objects.update_point(self.selected_points[0], None, None, lh, rh)?;
                return self.stale();
            }
            // Otherwise drag the points, keeping them arranged the same way relative to the first one
            else {
                let first_position = state.objects.get_point_info(self.selected_points[0]).unwrap().position();
                let delta = pos - first_position;
                let targets: Vec<_> = self.selected_points.iter()
                    .map(|&point_id| (point_id, state.objects.get_point_info(point_id).unwrap().position() + delta))
                    .collect();
                for (point_id, target) in targets {
                    state.objects.update_point(point_id, Some(target), None, None, None)?;
                }
                return self.stale();
            }
        }
        self.ok()
    }

    fn handle_drag_released(&mut self, _state: ControllerStateInput, _modifiers: InputModifiers) -> ToolResult {
        // Release drag on selected point/handle
        self.handle = None;
        self.ok()
    }

    fn handle_cancel(&mut self, _state: ControllerStateInput) -> ToolResult { self.ok() }

    fn handle_delete(&mut self, mut state: ControllerStateInput) -> ToolResult {
        if !self.selected_points.is_empty() {
            // Delete the points one by one, being careful of indices
            self.selected_points.sort();
            let mut offset = 0;
            #[allow(clippy::explicit_counter_loop)] // offset counts deletions, not iterations
            for point_id in &self.selected_points {
                state.objects.delete_point(*point_id - offset)?;
                offset += 1;
            }

            self.selected_points.clear();
            self.handle = None;
            return self.stale();
        }
        self.ok()
    }
}

impl EditorToolPoint {
    /// The selected point, if exactly one point is selected.
    fn single_selected_point(&self) -> Option<u16> {
        if self.selected_points.len() == 1 { Some(self.selected_points[0]) } else { None }
    }

    /// The selected zone, if all selected points are in the same zone.
    fn single_selected_zone(&self, state: &ControllerStateInput) -> Option<u16> {
        let mut zones = self.selected_points.iter()
            .map(|&point_id| state.objects.get_point_info(point_id).unwrap().zone_id());
        let first = zones.next()?;
        if zones.all(|zone_id| zone_id == first) { Some(first) } else { None }
    }
}

/// Get the IDs of every point in a zone.
fn points_in_zone(zone_id: u16, state: &ControllerStateInput) -> Vec<u16> {
    let (start, count) = state.objects.get_zone_info(zone_id).unwrap().range();
    (start..start + count).collect()
}



#[cfg(test)]
mod tests {
    use studio_math::Vec2;

    use crate::ObjectBuffers;
    use crate::objects::ControlPointMode;
    use crate::objects::test_utils::add_square;

    use super::*;
    use super::super::test_utils::state;

    /// A tool with nothing selected
    #[test]
    fn init() {
        let tool = EditorToolPoint::init();
        assert_eq!(tool.kind(), EditorToolKind::Point);
        assert!(tool.selection().is_empty());
    }

    /// Selecting keeps the selected points
    #[test]
    fn select_keeps_points() {
        let objects = ObjectBuffers::headless();
        let tool = EditorToolPoint::select(&[2, 3], state(&objects));
        assert_eq!(tool.selection(), &[2, 3]);
    }

    /// Clicking a point selects it, a path selects its whole zone, and nothing deselects
    #[test]
    fn click_selects() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 100.0);
        let mut tool = EditorToolPoint::init();

        tool.handle_click(state(&objects), Vec2::new(100.0, 100.5), Default::default()).unwrap();
        assert_eq!(tool.selection(), &[2]);

        tool.handle_click(state(&objects), Vec2::new(50.0, -1.0), Default::default()).unwrap();
        assert_eq!(tool.selection(), &[0, 1, 2, 3]);

        tool.handle_click(state(&objects), Vec2::new(500.0, 500.0), Default::default()).unwrap();
        assert!(tool.selection().is_empty());
    }

    /// Clicking the handle of the selected point should keep it selected
    #[test]
    fn click_handle_keeps_selection() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        objects.update_point(1, None, Some(ControlPointMode::Broken), None, Some(Vec2::new(0.0, 30.0))).unwrap();
        let mut tool = EditorToolPoint::select(&[1], state(&objects));

        tool.handle_click(state(&objects), Vec2::new(10.0, 30.0), Default::default()).unwrap();
        assert_eq!(tool.selection(), &[1]);
    }

    /// Clicking a point shared with another zone selects its sibling in the other zone
    #[test]
    fn click_selects_sibling() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (_, branch_id) = objects.create_branching_zone(1).unwrap();
        objects.create_point(1, Vec2::new(20.0, 0.0)).unwrap();
        let mut tool = EditorToolPoint::select(&[1], state(&objects));

        tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), Default::default()).unwrap();
        assert_eq!(tool.selection(), &[branch_id]);
    }

    /// Dragging a point moves it, and marks the normals stale
    #[test]
    fn drag_moves_point() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolPoint::init();

        let result = tool.handle_drag_start(state(&objects), Vec2::new(10.0, 0.0), Default::default()).unwrap();
        assert!(!result.normals_stale);
        assert_eq!(tool.selection(), &[1]);

        let result = tool.handle_dragging_to(state(&objects), Vec2::new(14.0, 3.0), Default::default()).unwrap();
        assert!(result.normals_stale);
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(14.0, 3.0));
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::ZERO);
    }

    /// Dragging several points keeps them arranged the same way
    #[test]
    fn drag_moves_selection_together() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolPoint::select(&[0, 1, 2, 3], state(&objects));

        tool.handle_dragging_to(state(&objects), Vec2::new(5.0, 5.0), Default::default()).unwrap();
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(5.0, 5.0));
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 15.0));
    }

    /// Dragging a handle of the selected point moves the handle and not the point
    #[test]
    fn drag_moves_handle() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        objects.update_point(1, None, Some(ControlPointMode::Broken), None, Some(Vec2::new(0.0, 30.0))).unwrap();
        let mut tool = EditorToolPoint::select(&[1], state(&objects));

        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 30.0), Default::default()).unwrap();
        assert_eq!(tool.handle, Some(true));
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(25.0, 10.0), Default::default()).unwrap();
        assert!(result.normals_stale);

        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.position(), Vec2::new(10.0, 0.0));
        assert_eq!(point.right_handle(), Vec2::new(25.0, 10.0));

        tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(tool.handle, None);
    }

    /// Dragging with nothing selected does nothing
    #[test]
    fn drag_nothing() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolPoint::init();

        tool.handle_drag_start(state(&objects), Vec2::new(50.0, 50.0), Default::default()).unwrap();
        assert!(!tool.handle_dragging_to(state(&objects), Vec2::new(60.0, 60.0), Default::default()).unwrap().normals_stale);
    }

    /// Deleting removes every selected point, even though IDs shift as they go
    #[test]
    fn delete_points() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        objects.insert_point(0, 0, 0.5).unwrap();
        let mut tool = EditorToolPoint::select(&[1, 2], state(&objects));

        assert!(tool.handle_delete(state(&objects)).unwrap().normals_stale);
        assert_eq!(objects.point_count(), 3);
        assert!(tool.selection().is_empty());
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(10.0, 10.0));
    }

    /// Deleting with nothing selected does nothing
    #[test]
    fn delete_nothing() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolPoint::init();
        assert!(!tool.handle_delete(state(&objects)).unwrap().normals_stale);
        assert_eq!(objects.point_count(), 4);
    }
}
