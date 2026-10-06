use crate::interaction::tools::utils::get_clicked_handle;

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
    fn init() -> Box<Self> where Self : Sized { Box::new(Default::default()) }
    
    fn select(selected_points: SelectionType, _state: ControllerStateInput) -> Box<Self> where Self : Sized {
        Box::new(Self { selected_points: Vec::from(selected_points), handle: None })
    }

    fn deselect(&mut self, _state: ControllerStateInput) -> SelectionType<'_> {
        // No cleanup needed
        &self.selected_points[..]
    }

    fn kind(&self) -> EditorToolKind { EditorToolKind::Point }
    fn selection(&self) -> SelectionType<'_> { &self.selected_points[..] }

    fn handle_click(&mut self, state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // TODO: Create helper like in zone for selecting with modifier keys

        // Reselect unless we clicked the current control point's handle
        let selected_point = self.single_selected_point();
        if selected_point.is_none() || get_clicked_handle(selected_point.unwrap(), &state, pos).is_none()
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

    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // TODO: Use modifier keys to add to or remove from selection first

        // If we select a handle of the control point start dragging it
        if let Some(point_id) = self.single_selected_point() && let Some(right) = get_clicked_handle(point_id, &state, pos) {
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

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
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

    fn handle_drag_released(&mut self, _state: ControllerStateInput) -> ToolResult {
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
