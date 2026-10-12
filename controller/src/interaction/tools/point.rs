use studio_math::Vec2;

use crate::InputModifiers;
use crate::objects::ControlPointMode;
use super::{EditorTool, EditorToolKind, ControllerStateInput, SelectionType, SpecialAction, utils};
use super::result::{ToolResult, DefaultResults};

/// The Point tool allows managing individual points within a zone.
///
/// Selection rules:
/// Clicking a point selects it along with its siblings, and clicking a path selects the points at either end of it, along with their siblings.
/// SHIFT: add to selection instead of replacing
/// ALT: remove from selection instead of replacing
/// CTRL: select all connected points instead of just the clicked point or path
///
/// Rules that follow from these:
/// Ctrl + Shift: add all connected points to selection
/// Ctrl + Alt: remove all connected points from selection
/// Shift + Alt: toggle points in selection
/// Shift + Alt + Ctrl: toggle all connected points in selection
///
/// Clicking a point without any modifier keys only selects one instance of it, and clicking again cycles through its siblings.
/// This is because handles can only be edited when a single instance of a point is selected.
///
/// When clicking and dragging instead of just clicking: should only change selection if not clicking on a selected point or path
///
/// Dragging points:
/// SHIFT: snap movement to horizontal, vertical, or diagonal, whichever is closest to the direction moved
/// CTRL: snap the grabbed point to the grid
///
/// Dragging a selection box:
/// Uses the same selection rules as above, applied against the selection from before the drag started.
/// The result is tentative until the drag is released, and can be cancelled.
#[derive(Default, Clone)]
pub struct EditorToolPoint {
    /// The points that are selected.
    selected_points: Vec<u16>,

    /// The points being selected by an in-progress selection box.
    /// Replaces the selection once the drag is released.
    tentative_selection: Option<Vec<u16>>,

    /// The handle we are manipulating, if any. True == right handle.
    handle: Option<bool>,

    /// The left and right handles of the point when we started dragging one, relative to the point.
    handle_start: Option<(Vec2, Vec2)>,

    /// The mouse position where we started dragging points.
    drag_origin: Option<Vec2>,

    /// The position of the grabbed point when we started dragging points.
    drag_anchor: Option<Vec2>,

    /// How far the selected points have been moved so far in the current drag.
    drag_offset: Vec2,

    /// The start point for our selection box if we are instead dragging out a box of points
    selection_origin: Option<Vec2>,

    /// The end point for our selection box if we are instead dragging out a box of points
    selection_box_end: Option<Vec2>,
}

impl EditorTool for EditorToolPoint {
    fn init() -> Box<Self> where Self : Sized { Box::default() }

    fn select(selected_points: SelectionType, _state: ControllerStateInput) -> Box<Self> where Self : Sized {
        Box::new(Self { selected_points: Vec::from(selected_points), ..Default::default() })
    }

    fn deselect(&mut self, _state: ControllerStateInput) -> SelectionType<'_> {
        // Just return selection
        self.selection()
    }

    fn kind(&self) -> EditorToolKind { EditorToolKind::Point }
    fn selection(&self) -> SelectionType<'_> {
        match &self.tentative_selection {
            Some(points) => &points[..],
            None => &self.selected_points[..],
        }
    }
    fn selection_box(&self) -> Option<(Vec2, Vec2)> {
        if let (Some(start), Some(end)) = (self.selection_origin, self.selection_box_end) {
            Some((start, end))
        } else {
            None
        }
    }

    fn handle_click(&mut self, state: ControllerStateInput, pos: studio_math::Vec2, modifiers: InputModifiers) -> ToolResult {
        // Reselect unless we clicked the current control point's handle
        if let Some(point_id) = self.single_selected_point() && utils::get_clicked_handle(point_id, &state, pos)?.is_some() {
            return self.ok();
        }
        (self.selected_points, _) = self.get_updated_selection(&state, pos, false, modifiers)?;
        self.ok()
    }

    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: studio_math::Vec2, modifiers: InputModifiers) -> ToolResult {
        // If we select a handle of the control point start dragging it
        if let Some(point_id) = self.single_selected_point() && let Some(right) = utils::get_clicked_handle(point_id, &state, pos)? {
            let point = state.objects.get_point_info(point_id)?;
            self.handle = Some(right);
            self.handle_start = Some((point.left_handle() - point.position(), point.right_handle() - point.position()));
            return self.ok();
        }

        // If we grabbed something that's already selected, drag the selection as is.
        // Otherwise update the selection first.
        let anchor_id = match self.grabbed_selected(&state, pos)? {
            Some(anchor_id) => Some(anchor_id),
            None => {
                let (new_selection, anchor_id) = self.get_updated_selection(&state, pos, true, modifiers)?;
                if anchor_id.is_some() { self.selected_points = new_selection; }
                anchor_id
            }
        };

        // Start dragging the selected points
        if let Some(anchor_id) = anchor_id {
            self.drag_origin = Some(pos);
            self.drag_anchor = Some(state.objects.get_point_info(anchor_id)?.position());
            self.drag_offset = Vec2::ZERO;
        }

        // If we didn't click anything, start dragging a box instead.
        // The selection is tentative until released, when it will select everything within the box.
        else {
            self.selection_origin = Some(pos);
            self.selection_box_end = Some(pos);
            self.set_tentative_selection(&[], &state, modifiers)?;
        }
        self.ok()
    }

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2, modifiers: InputModifiers) -> ToolResult {
        // If we are dragging the handle of a single point, adjust that
        if let Some(point_id) = self.single_selected_point() && let Some(right) = self.handle {
            let point = state.objects.get_point_info(point_id)?;
            let current = if right { point.right_handle() } else { point.left_handle() } - point.position();
            let handle = Some(utils::enforce_handle_length(pos - point.position(), current));
            let (lh, rh) = if right { (None, handle) } else { (handle, None) };
            state.objects.update_point(point_id, None, None, lh, rh)?;
            return self.stale();
        }

        // Or if we are dragging points, move them all together. Moving points also moves their siblings, but each one is only moved once.
        else if let (Some(origin), Some(anchor)) = (self.drag_origin, self.drag_anchor) {
            if self.selected_points.is_empty() { return self.ok(); }
            let target = utils::snap_drag_target(anchor + (pos - origin), anchor, origin, pos, modifiers);
            let offset = target - anchor;
            state.objects.move_points(&self.selected_points[..], offset - self.drag_offset)?;
            self.drag_offset = offset;
            return self.stale();
        }

        // Or if we are dragging a selection box, tentatively select the points in the box
        else if let Some(selection_origin) = self.selection_origin {
            self.selection_box_end = Some(pos);
            let boxed_points = state.objects.get_points_in_rect(selection_origin, pos)?;
            self.set_tentative_selection(&boxed_points[..], &state, modifiers)?;
        }
        self.ok()
    }

    fn handle_drag_released(&mut self, _state: ControllerStateInput, _modifiers: InputModifiers) -> ToolResult {
        // Commit the tentative selection, if any
        if let Some(points) = self.tentative_selection.take() {
            self.selected_points = points;
        }

        // Release drag on selected points/handle, but keep them selected
        self.stop_dragging();
        self.ok()
    }

    fn handle_cancel(&mut self, mut state: ControllerStateInput) -> ToolResult {
        // If dragging a handle, put it back where it started
        if self.handle.take().is_some() {
            if let Some(point_id) = self.single_selected_point() && let Some((lh, rh)) = self.handle_start.take() {
                state.objects.update_point(point_id, None, None, Some(lh), Some(rh))?;
                return self.stale();
            }
        }
        // If dragging points, put them back where they started
        else if self.drag_origin.is_some() {
            let offset = self.drag_offset;
            self.stop_dragging();
            if !self.selected_points.is_empty() {
                state.objects.move_points(&self.selected_points[..], -offset)?;
                return self.stale();
            }
        }
        // If dragging a selection box, just abandon the box and keep the previous selection
        else if self.selection_origin.is_some() {
            self.stop_dragging();
        }
        // Otherwise clear selection
        else {
            self.selected_points.clear();
        }
        self.ok()
    }

    fn handle_delete(&mut self, mut state: ControllerStateInput) -> ToolResult {
        // Stop dragging out selection box
        if self.selection_origin.is_some() {
            self.stop_dragging();
        }
        // Or delete selected points
        else if !self.selected_points.is_empty() {
            let mut point_ids = std::mem::take(&mut self.selected_points);
            point_ids.sort();
            point_ids.dedup();

            // Delete from the highest ID down, so that deleting a point never changes the IDs of the points still to be deleted.
            // Deleting a point can delete its whole zone, so skip any remaining points that were deleted along with it.
            let mut deleted_zone_range = None;
            for &point_id in point_ids.iter().rev() {
                if let Some((start, end)) = deleted_zone_range && point_id >= start && point_id < end { continue; }

                let zone_id = state.objects.get_point_info(point_id)?.zone_id();
                let (start, count) = state.objects.get_zone_info(zone_id)?.range();
                let point_count = state.objects.point_count();
                state.objects.delete_point(point_id)?;
                if point_count - state.objects.point_count() > 1 {
                    deleted_zone_range = Some((start, start + count));
                }
            }

            self.stop_dragging();
            return self.stale();
        }
        self.ok()
    }

    fn handle_special_action(&mut self, mut state: ControllerStateInput, action: SpecialAction) -> ToolResult {
        if self.selected_points.is_empty() { return self.ok(); }
        match action {
            SpecialAction::SelectionSeparated => {
                self.stop_dragging();
                state.objects.separate_points(&self.selected_points)?;
                self.ok()
            },

            SpecialAction::PointModeSet(mode) => {
                let Some(point_id) = self.single_selected_point() else { return self.ok(); };
                self.stop_dragging();
                state.objects.set_point_mode(point_id, mode)?;
                self.stale()
            },

            SpecialAction::PointModeToggled => {
                let Some(point_id) = self.single_selected_point() else { return self.ok(); };
                self.stop_dragging();
                // Points whose handles sync to different points don't have a mode, so setting it will show why it can't change
                let mode = match state.objects.get_point_mode(point_id)? {
                    Some(ControlPointMode::Linear) => ControlPointMode::Continuous,
                    _ => ControlPointMode::Linear,
                };
                state.objects.set_point_mode(point_id, mode)?;
                self.stale()
            },

            SpecialAction::CycleSibling => {
                let Some(point_id) = self.single_selected_point() else { return self.ok(); };
                self.stop_dragging();
                self.selected_points = vec![state.objects.get_sibling(point_id)?];
                self.ok()
            },

            SpecialAction::PathSplit => {
                // Only a selected path can be split, i.e. two groups of siblings next to each other in a zone
                let groups = state.objects.sibling_groups(&self.selected_points);
                if groups.len() != 2 { return self.ok(); }
                let Some((zone_id, start_id)) = state.objects.path_between(&groups[0], &groups[1])? else { return self.ok(); };

                // Only select the inserted point, not the ones inserted into sibling paths
                self.stop_dragging();
                self.selected_points = vec![state.objects.insert_point(zone_id, start_id, 0.5)?[0]];
                self.stale()
            },

            SpecialAction::PointsMerged => {
                self.stop_dragging();
                self.selected_points = state.objects.merge_points(&self.selected_points)?;
                self.stale()
            },

            _ => self.ok(),
        }
    }
}

impl EditorToolPoint {
    /// The selected point, if exactly one point is selected.
    fn single_selected_point(&self) -> Option<u16> {
        if self.selected_points.len() == 1 { Some(self.selected_points[0]) } else { None }
    }

    /// The selected zone, if all selected points are in the same zone.
    fn single_selected_zone(&self, state: &ControllerStateInput) -> anyhow::Result<Option<u16>> {
        let zones = self.selected_points.iter()
            .map(|&point_id| Ok(state.objects.get_point_info(point_id)?.zone_id()))
            .collect::<anyhow::Result<Vec<u16>>>()?;
        let Some(&first) = zones.first() else { return Ok(None); };
        Ok(if zones.iter().all(|&zone_id| zone_id == first) { Some(first) } else { None })
    }

    /// Clear everything about an in-progress drag, other than the selection
    fn stop_dragging(&mut self) {
        self.tentative_selection = None;
        self.handle = None;
        self.handle_start = None;
        self.drag_origin = None;
        self.drag_anchor = None;
        self.drag_offset = Vec2::ZERO;
        self.selection_origin = None;
        self.selection_box_end = None;
    }

    /// Get the points that a click (or drag start) affects, and the ID of the point that was grabbed, if anything was clicked.
    /// Clicking a point gives that point and its siblings, and clicking a path gives the points at either end and their siblings.
    /// If ctrl is held, it instead gives all points connected to the clicked point or path.
    /// If this is a click without any modifier keys on a point, it gives a single instance of the point, cycling through its siblings.
    fn clicked_unit(&self, state: &ControllerStateInput, pos: Vec2, modifiers: InputModifiers, dragging: bool) -> anyhow::Result<Option<(Vec<u16>, u16)>> {
        // Check the point
        if let Some((_, point_id)) = utils::get_clicked_control_point(state, None, pos)? {
            if modifiers.ctrl {
                return Ok(Some((state.objects.get_connected_points(point_id, true)?, point_id)));
            }
            if dragging || modifiers.shift || modifiers.alt {
                return Ok(Some((with_siblings(&[point_id], state), point_id)));
            }

            // Selecting a path selects all of its points, so the "selected zone" is the one all selected points share.
            let selected_point = self.single_selected_point();
            let point_id =
                // If the clicked point has a sibling in the selected zone, select it.
                if let Some(selected_zone_id) = self.single_selected_zone(state)? && let Some(sibling_id) = state.objects.sibling_in_zone(point_id, selected_zone_id) {
                    // If the clicked point's sibling in the selected zone IS our selected point (i.e. we clicked our selected point), select its sibling.
                    if selected_point == Some(sibling_id) { state.objects.get_sibling(sibling_id)? }

                    // Otherwise just select the clicked sibling
                    else { sibling_id }
                }

                // If the clicked point doesn't have a sibling in the selected zone, select what we clicked.
                else { point_id };
            return Ok(Some((vec![point_id], point_id)));
        }

        // Check the path
        if let Some((zone_id, start_id, _, _)) = utils::get_clicked_zone_path(state, pos)? {
            if modifiers.ctrl {
                return Ok(Some((state.objects.get_connected_points(start_id, true)?, start_id)));
            }
            let (start, count) = state.objects.get_zone_info(zone_id)?.range();
            let end_id = start + (start_id - start + 1) % count;
            return Ok(Some((with_siblings(&[start_id, end_id], state), start_id)));
        }

        // Nothing is clicked
        Ok(None)
    }

    /// If the point or path at a position is already selected, get the ID of the grabbed point.
    /// A point counts as selected if any of its siblings are, and a path counts as selected if the points at both ends are.
    fn grabbed_selected(&self, state: &ControllerStateInput, pos: Vec2) -> anyhow::Result<Option<u16>> {
        let is_selected = |point_id: u16| {
            self.selected_points.contains(&point_id)
                || state.objects.get_siblings(point_id).iter().any(|sibling_id| self.selected_points.contains(sibling_id))
        };

        if let Some((_, point_id)) = utils::get_clicked_control_point(state, None, pos)? {
            Ok(if is_selected(point_id) { Some(point_id) } else { None })
        } else if let Some((zone_id, start_id, _, _)) = utils::get_clicked_zone_path(state, pos)? {
            let (start, count) = state.objects.get_zone_info(zone_id)?.range();
            let end_id = start + (start_id - start + 1) % count;
            Ok(if is_selected(start_id) && is_selected(end_id) { Some(start_id) } else { None })
        } else {
            Ok(None)
        }
    }

    /// Given a click (or drag start) point and input info, and whether or not this is a drag action,
    /// and using the state of the current selection, return a new selection.
    /// Also return the ID of the grabbed point, in case this is a drag start.
    fn get_updated_selection(&self, state: &ControllerStateInput, pos: Vec2, dragging: bool, modifiers: InputModifiers) -> anyhow::Result<(Vec<u16>, Option<u16>)> {
        // If something is clicked, add/remove/toggle/replace it
        if let Some((unit, anchor_id)) = self.clicked_unit(state, pos, modifiers, dragging)? {
            Ok((utils::combine_selection(&self.selected_points[..], &unit[..], modifiers, true), Some(anchor_id)))
        }

        // We are adding to/removing from/toggling selected points, so leave unmodified
        else if modifiers.shift || modifiers.alt {
            Ok((self.selected_points.clone(), None))
        }

        // Otherwise deselect
        else {
            Ok((vec![], None))
        }
    }

    /// Set the tentative selection from the points inside a selection box, applying modifiers against the committed selection.
    /// SHIFT adds, ALT removes, SHIFT+ALT toggles, and CTRL selects all connected points instead of only the boxed points and their siblings.
    fn set_tentative_selection(&mut self, boxed_points: &[u16], state: &ControllerStateInput, modifiers: InputModifiers) -> anyhow::Result<()> {
        let boxed = if modifiers.ctrl {
            let mut connected = std::collections::HashSet::new();
            for &point_id in boxed_points {
                if !connected.contains(&point_id) {
                    connected.extend(state.objects.get_connected_points(point_id, true)?);
                }
            }
            connected.into_iter().collect()
        } else {
            with_siblings(boxed_points, state)
        };

        self.tentative_selection = Some(utils::combine_selection(&self.selected_points[..], &boxed[..], modifiers, false));
        Ok(())
    }
}

/// Get a list of points along with all of their siblings, without duplicates.
fn with_siblings(point_ids: &[u16], state: &ControllerStateInput) -> Vec<u16> {
    let mut points = std::collections::HashSet::new();
    for &point_id in point_ids {
        points.insert(point_id);
        points.extend(state.objects.get_siblings(point_id));
    }
    let mut points: Vec<u16> = points.into_iter().collect();
    points.sort();
    points
}



#[cfg(test)]
mod tests {
    use crate::{ObjectBuffers, InputModifiers as IM};
    use crate::objects::ControlPointMode;
    use crate::objects::test_utils::{add_square, add_zone};

    use super::*;
    use super::super::test_utils::state;

    /// Sort a selection so it can be compared regardless of order
    fn sorted(selection: &[u16]) -> Vec<u16> {
        let mut selection = selection.to_vec();
        selection.sort();
        selection
    }

    /// Add a 20x20 square (zone 0, points 0-3) with a second square branching off its bottom edge (zone 1, points 4-7).
    /// The bottom corners of the first square, (20, 20) and (0, 20), are shared with the second.
    /// The squares are big enough that clicking the middle of a path doesn't click a point.
    fn add_branching_squares(objects: &mut ObjectBuffers) {
        add_square(objects, 20.0);
        let (branch_zone, _) = objects.create_branching_zone(2).unwrap();
        objects.create_point(branch_zone, Vec2::new(20.0, 40.0)).unwrap();
        objects.create_point(branch_zone, Vec2::new(0.0, 40.0)).unwrap();
        objects.complete_branching_zone(2, branch_zone, 3).unwrap();
    }

    /// Point 2 (at (20, 20)) and its sibling in zone 1
    fn shared_point(objects: &ObjectBuffers) -> Vec<u16> {
        let mut points = vec![2];
        points.extend(objects.get_siblings(2));
        points.sort();
        points
    }

    /// The positions of every point, in order
    fn positions(objects: &ObjectBuffers) -> Vec<Vec2> {
        (0..objects.point_count()).map(|id| objects.get_point_info(id).unwrap().position()).collect()
    }

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

    /// Clicking a point selects it, a path selects the points at either end, and nothing deselects
    #[test]
    fn click_selects() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 100.0);
        let mut tool = EditorToolPoint::init();

        tool.handle_click(state(&objects), Vec2::new(100.0, 100.5), Default::default()).unwrap();
        assert_eq!(tool.selection(), &[2]);

        tool.handle_click(state(&objects), Vec2::new(50.0, -1.0), Default::default()).unwrap();
        assert_eq!(tool.selection(), &[0, 1]);

        tool.handle_click(state(&objects), Vec2::new(500.0, 500.0), Default::default()).unwrap();
        assert!(tool.selection().is_empty());
    }

    /// Clicking a path selects the points at either end, including their siblings when the path is shared
    #[test]
    fn click_path_selects_segment() {
        let mut objects = ObjectBuffers::headless();
        add_branching_squares(&mut objects);
        let mut tool = EditorToolPoint::init();

        // The shared path along the bottom of the first square
        tool.handle_click(state(&objects), Vec2::new(10.0, 20.0), Default::default()).unwrap();
        let mut expected = shared_point(&objects);
        expected.push(3);
        expected.extend(objects.get_siblings(3));
        expected.sort();
        assert_eq!(expected.len(), 4);
        assert_eq!(sorted(tool.selection()), expected);

        // A path that isn't shared, but still includes the siblings of its end points
        tool.handle_click(state(&objects), Vec2::new(0.0, 10.0), Default::default()).unwrap();
        let mut expected = vec![0, 3];
        expected.extend(objects.get_siblings(3));
        expected.sort();
        assert_eq!(sorted(tool.selection()), expected);
    }

    /// Ctrl+clicking a point or a path selects every point connected to it
    #[test]
    fn ctrl_click_selects_connected() {
        let mut objects = ObjectBuffers::headless();
        add_branching_squares(&mut objects);
        add_zone(&mut objects, &[(40.0, 0.0), (50.0, 0.0), (50.0, 10.0)]);
        let mut tool = EditorToolPoint::init();

        tool.handle_click(state(&objects), Vec2::new(20.0, 0.0), IM::ctrl()).unwrap();
        assert_eq!(sorted(tool.selection()), &[0, 1, 2, 3, 4, 5, 6, 7]);

        tool.handle_click(state(&objects), Vec2::new(45.0, 0.0), IM::ctrl()).unwrap(); // path
        assert_eq!(sorted(tool.selection()), &[8, 9, 10]);

        tool.handle_click(state(&objects), Vec2::new(10.0, 40.0), IM::ctrl()).unwrap(); // path in the branching zone
        assert_eq!(sorted(tool.selection()), &[0, 1, 2, 3, 4, 5, 6, 7]);

        tool.handle_click(state(&objects), Vec2::new(100.0, 100.0), IM::ctrl()).unwrap();
        assert!(tool.selection().is_empty());
    }

    /// Modifier keys add to, remove from, or toggle the selection
    #[test]
    fn modifier_clicks() {
        let mut objects = ObjectBuffers::headless();
        add_branching_squares(&mut objects);
        add_zone(&mut objects, &[(40.0, 0.0), (50.0, 0.0), (50.0, 10.0)]);
        let mut tool = EditorToolPoint::init();
        let with = |extra: &[u16]| {
            let mut points = extra.to_vec();
            points.sort();
            points
        };

        tool.handle_click(state(&objects), Vec2::new(40.0, 0.0), IM::none()).unwrap();
        assert_eq!(tool.selection(), &[8]);

        // Shift adds the point and its siblings
        tool.handle_click(state(&objects), Vec2::new(20.0, 20.0), IM::shift()).unwrap();
        let mut expected = shared_point(&objects);
        expected.push(8);
        assert_eq!(sorted(tool.selection()), with(&expected));

        // Alt removes them
        tool.handle_click(state(&objects), Vec2::new(20.0, 20.0), IM::alt()).unwrap();
        assert_eq!(tool.selection(), &[8]);

        // Shift+Alt toggles them on and off
        tool.handle_click(state(&objects), Vec2::new(20.0, 20.0), IM::alt_shift()).unwrap();
        assert_eq!(sorted(tool.selection()), with(&expected));
        tool.handle_click(state(&objects), Vec2::new(20.0, 20.0), IM::alt_shift()).unwrap();
        assert_eq!(tool.selection(), &[8]);

        // Clicking nothing with a modifier keeps the selection
        tool.handle_click(state(&objects), Vec2::new(100.0, 100.0), IM::shift()).unwrap();
        assert_eq!(tool.selection(), &[8]);
        tool.handle_click(state(&objects), Vec2::new(100.0, 100.0), IM::alt()).unwrap();
        assert_eq!(tool.selection(), &[8]);

        // Ctrl+Shift adds everything connected
        tool.handle_click(state(&objects), Vec2::new(0.0, 40.0), IM::ctrl_shift()).unwrap();
        assert_eq!(sorted(tool.selection()), &[0, 1, 2, 3, 4, 5, 6, 7, 8]);

        // Ctrl+Alt removes everything connected, even if the clicked point isn't selected
        tool.handle_click(state(&objects), Vec2::new(50.0, 10.0), IM::ctrl_alt()).unwrap();
        assert_eq!(sorted(tool.selection()), &[0, 1, 2, 3, 4, 5, 6, 7]);

        // Shift+Alt on a single point toggles just that point off
        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), IM::alt_shift()).unwrap();
        assert_eq!(sorted(tool.selection()), &[1, 2, 3, 4, 5, 6, 7]);

        // Shift+Alt+Ctrl toggles everything connected on, since not all of it was selected
        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), IM::all()).unwrap();
        assert_eq!(sorted(tool.selection()), &[0, 1, 2, 3, 4, 5, 6, 7]);

        // And then off, since all of it was selected
        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), IM::all()).unwrap();
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

    /// Clicking a shared point without modifiers cycles through its instances, but with modifiers selects all of them
    #[test]
    fn plain_click_cycles_siblings() {
        let mut objects = ObjectBuffers::headless();
        add_branching_squares(&mut objects);
        let shared = shared_point(&objects);

        let mut tool = EditorToolPoint::init();

        tool.handle_click(state(&objects), Vec2::new(20.0, 20.0), Default::default()).unwrap();
        let first = tool.selection().to_vec();
        assert_eq!(first.len(), 1);
        tool.handle_click(state(&objects), Vec2::new(20.0, 20.0), Default::default()).unwrap();
        let second = tool.selection().to_vec();
        assert_eq!(second.len(), 1);
        assert_ne!(first, second);
        tool.handle_click(state(&objects), Vec2::new(20.0, 20.0), Default::default()).unwrap();
        assert_eq!(tool.selection(), &first[..]);

        tool.handle_click(state(&objects), Vec2::new(20.0, 20.0), IM::shift()).unwrap();
        assert_eq!(sorted(tool.selection()), shared);
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

        // Releasing keeps the point selected but stops the drag
        tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(tool.selection(), &[1]);
        assert!(!tool.handle_dragging_to(state(&objects), Vec2::new(50.0, 50.0), Default::default()).unwrap().normals_stale);
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(14.0, 3.0));
    }

    /// Dragging several points keeps them arranged the same way
    #[test]
    fn drag_moves_selection_together() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolPoint::select(&[0, 1, 2, 3], state(&objects));

        tool.handle_drag_start(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
        assert_eq!(tool.selection(), &[0, 1, 2, 3]);
        tool.handle_dragging_to(state(&objects), Vec2::new(5.0, 5.0), Default::default()).unwrap();
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(5.0, 5.0));
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 15.0));
    }

    /// Dragging an unselected shared point selects all of its instances
    #[test]
    fn drag_selects_siblings() {
        let mut objects = ObjectBuffers::headless();
        add_branching_squares(&mut objects);
        let mut tool = EditorToolPoint::select(&[0], state(&objects));

        tool.handle_drag_start(state(&objects), Vec2::new(20.0, 20.0), Default::default()).unwrap();
        assert_eq!(sorted(tool.selection()), shared_point(&objects));
    }

    /// Dragging a selected path keeps the selection
    #[test]
    fn drag_selected_path_keeps_selection() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 100.0);
        let mut tool = EditorToolPoint::select(&[0, 1, 2], state(&objects));

        tool.handle_drag_start(state(&objects), Vec2::new(100.0, 50.0), Default::default()).unwrap();
        assert_eq!(tool.selection(), &[0, 1, 2]);

        // But a path with only one end selected is replaced
        tool.handle_drag_start(state(&objects), Vec2::new(50.0, 100.0), Default::default()).unwrap();
        assert_eq!(tool.selection(), &[2, 3]);
    }

    /// Dragging a shared point applies the movement once, even though moving one instance moves all of them
    #[test]
    fn drag_moves_siblings_once() {
        let mut objects = ObjectBuffers::headless();
        add_branching_squares(&mut objects);
        let shared = shared_point(&objects);
        let mut tool = EditorToolPoint::init();

        // Drag the shared point and both its instances
        tool.handle_drag_start(state(&objects), Vec2::new(20.0, 20.0), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(23.0, 21.0), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(25.0, 22.0), Default::default()).unwrap();
        for &point_id in &shared {
            assert_eq!(objects.get_point_info(point_id).unwrap().position(), Vec2::new(25.0, 22.0));
        }
        assert_eq!(objects.get_point_info(3).unwrap().position(), Vec2::new(0.0, 20.0));
        tool.handle_drag_released(state(&objects), Default::default()).unwrap();

        // Drag everything connected
        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), IM::ctrl()).unwrap();
        tool.handle_drag_start(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
        assert_eq!(tool.selection().len(), 8);
        tool.handle_dragging_to(state(&objects), Vec2::new(5.0, 2.0), Default::default()).unwrap();
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(5.0, 2.0));
        for &point_id in &shared {
            assert_eq!(objects.get_point_info(point_id).unwrap().position(), Vec2::new(30.0, 24.0));
        }
        let mut corner = vec![3];
        corner.extend(objects.get_siblings(3));
        for point_id in corner {
            assert_eq!(objects.get_point_info(point_id).unwrap().position(), Vec2::new(5.0, 22.0));
        }

        // Cancelling puts every instance back
        tool.handle_cancel(state(&objects)).unwrap();
        for &point_id in &shared {
            assert_eq!(objects.get_point_info(point_id).unwrap().position(), Vec2::new(25.0, 22.0));
        }
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::ZERO);
    }

    /// Dragging a single selected instance of a shared point keeps just that instance selected, so its handles stay editable
    #[test]
    fn drag_selected_keeps_selection() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (_, branch_id) = objects.create_branching_zone(1).unwrap();
        objects.create_point(1, Vec2::new(20.0, 0.0)).unwrap();
        let mut tool = EditorToolPoint::select(&[1], state(&objects));

        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 0.0), Default::default()).unwrap();
        assert_eq!(tool.selection(), &[1]);
        tool.handle_dragging_to(state(&objects), Vec2::new(12.0, 0.0), Default::default()).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(12.0, 0.0));
        assert_eq!(objects.get_point_info(branch_id).unwrap().position(), Vec2::new(12.0, 0.0));
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

    /// Dragging a handle onto its point keeps it at the minimum length, pointing the way it was before
    #[test]
    fn drag_handle_minimum_length() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        objects.update_point(1, None, Some(ControlPointMode::Broken), Some(Vec2::new(-20.0, 0.0)), Some(Vec2::new(0.0, 30.0))).unwrap();
        let mut tool = EditorToolPoint::select(&[1], state(&objects));

        // Dragged close to the point, the handle keeps its new direction
        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 30.0), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(11.0, 0.0), Default::default()).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().right_handle(), Vec2::new(12.0, 0.0));

        // Dragged exactly onto the point, the handle keeps its previous direction
        tool.handle_dragging_to(state(&objects), Vec2::new(10.0, 0.0), Default::default()).unwrap();
        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.right_handle(), Vec2::new(12.0, 0.0));
        assert_eq!(point.left_handle(), Vec2::new(-10.0, 0.0));
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

    /// Shift constrains dragging to an axis or diagonal, and ctrl snaps the grabbed point to the grid
    #[test]
    fn drag_modifiers() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolPoint::init();

        tool.handle_drag_start(state(&objects), Vec2::new(10.5, 0.5), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(15.5, 2.5), IM::shift()).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(15.0, 0.0));

        tool.handle_dragging_to(state(&objects), Vec2::new(11.5, 8.5), IM::shift()).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(10.0, 8.0));

        // Diagonal: mouse moved (+4, -6), which is closer to the diagonal than the vertical
        tool.handle_dragging_to(state(&objects), Vec2::new(14.5, -5.5), IM::shift()).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(15.0, -5.0));

        // Ctrl snaps the point, not the mouse
        tool.handle_dragging_to(state(&objects), Vec2::new(15.9, 3.1), IM::ctrl()).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(15.0, 3.0));

        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::ZERO);
    }

    /// Dragging a box over points selects them once released, and ctrl selects everything connected to them
    #[test]
    fn drag_to_select() {
        let mut objects = ObjectBuffers::headless();
        add_branching_squares(&mut objects);
        add_zone(&mut objects, &[(40.0, 0.0), (50.0, 0.0), (50.0, 10.0)]);
        let mut tool = EditorToolPoint::init();

        // Selection box should be empty when not dragging
        assert_eq!(tool.selection_box(), None);

        tool.handle_drag_start(state(&objects), Vec2::new(-10.0, -10.0), Default::default()).unwrap();
        assert_eq!(tool.selection_box(), Some((Vec2::new(-10.0, -10.0), Vec2::new(-10.0, -10.0))));
        assert!(tool.selection().is_empty());

        let result = tool.handle_dragging_to(state(&objects), Vec2::new(5.0, 5.0), Default::default()).unwrap();
        assert!(!result.normals_stale);
        assert_eq!(tool.selection(), &[0]);

        // Boxing shared points selects every instance
        tool.handle_dragging_to(state(&objects), Vec2::new(22.0, 22.0), Default::default()).unwrap();
        let mut expected = vec![0, 1, 3];
        expected.extend(shared_point(&objects));
        expected.extend(objects.get_siblings(3));
        expected.sort();
        assert_eq!(sorted(tool.selection()), expected);

        // Ctrl selects everything connected
        tool.handle_dragging_to(state(&objects), Vec2::new(5.0, 5.0), IM::ctrl()).unwrap();
        assert_eq!(sorted(tool.selection()), &[0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(tool.selection_box(), Some((Vec2::new(-10.0, -10.0), Vec2::new(5.0, 5.0))));

        // Releasing keeps the points selected but stops the drag
        tool.handle_drag_released(state(&objects), IM::ctrl()).unwrap();
        assert_eq!(sorted(tool.selection()), &[0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(tool.selection_box(), None);
        tool.handle_dragging_to(state(&objects), Vec2::new(100.0, 100.0), Default::default()).unwrap();
        assert_eq!(sorted(tool.selection()), &[0, 1, 2, 3, 4, 5, 6, 7]);
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::ZERO);
    }

    /// Box selection is tentative until released, follows modifiers, and can be cancelled
    #[test]
    fn drag_box_modifiers() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(40.0, 0.0), (50.0, 0.0), (50.0, 10.0)]);
        let mut tool = EditorToolPoint::init();

        tool.handle_click(state(&objects), Vec2::new(40.0, 0.0), Default::default()).unwrap();
        assert_eq!(tool.selection(), &[4]);

        // Shift adds to the selection, but only tentatively
        tool.handle_drag_start(state(&objects), Vec2::new(-10.0, -10.0), IM::shift()).unwrap();
        assert_eq!(tool.selection(), &[4]);
        tool.handle_dragging_to(state(&objects), Vec2::new(5.0, 5.0), IM::shift()).unwrap();
        assert_eq!(tool.selection(), &[0, 4]);
        tool.handle_cancel(state(&objects)).unwrap();
        assert_eq!(tool.selection(), &[4]);
        assert_eq!(tool.selection_box(), None);
        tool.handle_drag_released(state(&objects), IM::shift()).unwrap();
        assert_eq!(tool.selection(), &[4]);

        // Shift+Alt toggles each boxed point on its own
        tool.handle_drag_start(state(&objects), Vec2::new(30.0, -10.0), IM::alt_shift()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(55.0, 5.0), IM::alt_shift()).unwrap();
        assert_eq!(tool.selection(), &[5]);
        tool.handle_drag_released(state(&objects), IM::alt_shift()).unwrap();
        assert_eq!(tool.selection(), &[5]);

        // Alt removes from the selection once released
        tool.handle_drag_start(state(&objects), Vec2::new(30.0, -10.0), IM::alt()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(55.0, 15.0), IM::alt()).unwrap();
        assert!(tool.selection().is_empty());
        tool.handle_drag_released(state(&objects), IM::alt()).unwrap();
        assert!(tool.selection().is_empty());
    }

    /// Cancel input should deselect everything
    #[test]
    fn cancel_deselects() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolPoint::init();

        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), IM::ctrl()).unwrap();
        assert_eq!(tool.selection().len(), 4);

        assert!(!tool.handle_cancel(state(&objects)).unwrap().normals_stale);
        assert!(tool.selection().is_empty());
    }

    /// Cancel while dragging points puts them back and keeps the selection
    #[test]
    fn cancel_drag_restores_position() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolPoint::init();

        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 0.0), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(25.0, 32.0), Default::default()).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(25.0, 32.0));

        let result = tool.handle_cancel(state(&objects)).unwrap();
        assert!(result.normals_stale);
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(10.0, 0.0));
        assert_eq!(tool.selection(), &[1]);

        // The drag is over, so releasing or moving more does nothing
        tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(50.0, 50.0), Default::default()).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(10.0, 0.0));

        // A second cancel deselects
        tool.handle_cancel(state(&objects)).unwrap();
        assert!(tool.selection().is_empty());
    }

    /// Cancel while dragging a handle puts it back and keeps the selection
    #[test]
    fn cancel_handle_drag_restores_handle() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        objects.update_point(1, None, Some(ControlPointMode::Broken), Some(Vec2::new(-5.0, 0.0)), Some(Vec2::new(0.0, 30.0))).unwrap();
        let mut tool = EditorToolPoint::select(&[1], state(&objects));

        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 30.0), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(25.0, 10.0), Default::default()).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().right_handle(), Vec2::new(25.0, 10.0));

        assert!(tool.handle_cancel(state(&objects)).unwrap().normals_stale);
        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.right_handle(), Vec2::new(10.0, 30.0));
        assert_eq!(point.left_handle(), Vec2::new(5.0, 0.0));
        assert_eq!(point.position(), Vec2::new(10.0, 0.0));
        assert_eq!(tool.selection(), &[1]);
        assert_eq!(tool.handle, None);
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

    /// Square A (points 0-3), triangle B (points 4-6) and square C (points 7-10)
    fn add_delete_layout(objects: &mut ObjectBuffers) {
        add_square(objects, 10.0);
        add_zone(objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);
        add_zone(objects, &[(40.0, 0.0), (50.0, 0.0), (50.0, 10.0), (40.0, 10.0)]);
    }

    /// Delete a selection from a fresh delete layout, and return the objects afterward
    fn delete_from_layout(selection: &[u16]) -> ObjectBuffers {
        let mut objects = ObjectBuffers::headless();
        add_delete_layout(&mut objects);
        let mut tool = EditorToolPoint::select(selection, state(&objects));
        assert!(tool.handle_delete(state(&objects)).unwrap().normals_stale);
        assert!(tool.selection().is_empty());
        objects
    }

    /// Deleting two points of a triangle deletes the whole zone partway through, without deleting the wrong points
    #[test]
    fn delete_two_of_triangle() {
        let objects = delete_from_layout(&[4, 5]);
        assert_eq!(objects.object_counts(), (2, 8));
        assert_eq!(positions(&objects), vec![
            Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0),
            Vec2::new(40.0, 0.0), Vec2::new(50.0, 0.0), Vec2::new(50.0, 10.0), Vec2::new(40.0, 10.0),
        ]);
        assert_eq!(objects.get_zone_info(1).unwrap().range(), (4, 4));
    }

    /// Selected points left over from a deleted zone are skipped, and points in other zones are still deleted
    #[test]
    fn delete_whole_zone_and_others() {
        let objects = delete_from_layout(&[1, 4, 5, 6, 8]);
        assert_eq!(objects.object_counts(), (2, 6));
        assert_eq!(positions(&objects), vec![
            Vec2::new(0.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0),
            Vec2::new(40.0, 0.0), Vec2::new(50.0, 10.0), Vec2::new(40.0, 10.0),
        ]);
        assert_eq!(objects.get_zone_info(0).unwrap().range(), (0, 3));
        assert_eq!(objects.get_zone_info(1).unwrap().range(), (3, 3));
    }

    /// The order of the selection doesn't matter
    #[test]
    fn delete_unsorted_selection() {
        let unsorted = delete_from_layout(&[6, 0, 5, 4]);
        let sorted = delete_from_layout(&[0, 4, 5, 6]);
        assert_eq!(unsorted.object_counts(), (2, 7));
        assert_eq!(positions(&unsorted), positions(&sorted));
        assert_eq!(positions(&unsorted), vec![
            Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0),
            Vec2::new(40.0, 0.0), Vec2::new(50.0, 0.0), Vec2::new(50.0, 10.0), Vec2::new(40.0, 10.0),
        ]);
    }

    /// Deleting every point of the last zone deletes it and leaves the others untouched
    #[test]
    fn delete_only_last_zone_points() {
        let objects = delete_from_layout(&[7, 8, 9, 10]);
        assert_eq!(objects.object_counts(), (2, 7));
        assert_eq!(positions(&objects), vec![
            Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(10.0, 10.0), Vec2::new(0.0, 10.0),
            Vec2::new(20.0, 0.0), Vec2::new(30.0, 0.0), Vec2::new(30.0, 10.0),
        ]);
    }

    /// Deleting every instance of a shared point removes it from each zone, and other shared points stay linked
    #[test]
    fn delete_shared_point_with_siblings() {
        let mut objects = ObjectBuffers::headless();
        add_branching_squares(&mut objects);
        let mut tool = EditorToolPoint::init();

        tool.handle_click(state(&objects), Vec2::new(20.0, 20.0), IM::shift()).unwrap();
        assert_eq!(tool.selection().len(), 2);
        assert!(tool.handle_delete(state(&objects)).unwrap().normals_stale);
        assert_eq!(objects.object_counts(), (2, 6));
        assert!(positions(&objects).iter().all(|&position| position != Vec2::new(20.0, 20.0)));

        // The other shared point should still have its sibling, and move with it
        let corner = (0..objects.point_count()).find(|&id| objects.get_point_info(id).unwrap().position() == Vec2::new(0.0, 20.0)).unwrap();
        assert_eq!(objects.get_siblings(corner).len(), 1);
        tool.handle_drag_start(state(&objects), Vec2::new(0.0, 20.0), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(3.0, 23.0), Default::default()).unwrap();
        assert_eq!(positions(&objects).iter().filter(|&&position| position == Vec2::new(3.0, 23.0)).count(), 2);
        assert!(positions(&objects).iter().all(|&position| position != Vec2::new(0.0, 20.0)));
    }

    /// Cycling replaces the selected point with its next sibling
    #[test]
    fn special_cycle_sibling() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (_, branch_id) = objects.create_branching_zone(1).unwrap();
        let mut tool = EditorToolPoint::select(&[1], state(&objects));

        tool.handle_special_action(state(&objects), SpecialAction::CycleSibling).unwrap();
        assert_eq!(tool.selection(), &[branch_id]);
    }

    /// Splitting a selected path inserts a point halfway along it and selects only that point
    #[test]
    fn special_split_selects_new_point() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolPoint::select(&[1, 2], state(&objects));

        let result = tool.handle_special_action(state(&objects), SpecialAction::PathSplit).unwrap();
        assert!(result.normals_stale);
        assert_eq!(objects.point_count(), 5);
        assert_eq!(tool.selection(), &[2]);
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(10.0, 5.0));
    }

    /// Merging selects the merged points
    #[test]
    fn special_merge_selects_merged() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);
        let mut tool = EditorToolPoint::select(&[1, 4], state(&objects));

        tool.handle_special_action(state(&objects), SpecialAction::PointsMerged).unwrap();
        assert_eq!(sorted(tool.selection()), vec![1, 4]);
        assert_eq!(objects.get_siblings(1), vec![4]);
    }

    /// The handle mode of a single selected point can be set
    #[test]
    fn special_mode_set() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolPoint::select(&[1], state(&objects));

        let result = tool.handle_special_action(state(&objects), SpecialAction::PointModeSet(ControlPointMode::Broken)).unwrap();
        assert!(result.normals_stale);
        assert_eq!(objects.get_point_info(1).unwrap().mode(), ControlPointMode::Broken);
    }

    /// Toggling switches a single selected point between linear and continuous
    #[test]
    fn special_mode_toggled() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolPoint::select(&[1], state(&objects));

        tool.handle_special_action(state(&objects), SpecialAction::PointModeToggled).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().mode(), ControlPointMode::Continuous);
        tool.handle_special_action(state(&objects), SpecialAction::PointModeToggled).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().mode(), ControlPointMode::Linear);
    }

    /// Mode changes and cycling only apply when exactly one point is selected
    #[test]
    fn special_requires_single_point() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolPoint::select(&[1, 3], state(&objects));

        tool.handle_special_action(state(&objects), SpecialAction::PointModeSet(ControlPointMode::Broken)).unwrap();
        tool.handle_special_action(state(&objects), SpecialAction::CycleSibling).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().mode(), ControlPointMode::Linear);
        assert_eq!(tool.selection(), &[1, 3]);
    }
}
