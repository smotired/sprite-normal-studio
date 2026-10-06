use crate::interaction::tools::utils::get_clicked_handle;

use super::{EditorTool, EditorToolKind, ControllerStateInput, SelectionType, utils};
use super::result::{ToolResult, DefaultResults};

/// The Point tool allows managing individual points within a zone.
pub struct EditorToolPoint {
    /// The zone that is selected.
    selected_zone: Option<u16>,

    /// The point that is selected.
    selected_point: Option<u16>,

    /// The handle we are manipulating, if any. True == right handle.
    handle: Option<bool>,
}

impl EditorTool for EditorToolPoint {
    fn select((selected_zone, selected_point): SelectionType) -> Box<Self> where Self : Sized {
        Box::new(Self { selected_zone, selected_point, handle: None })
    }

    fn deselect(&mut self) -> SelectionType {
        // No cleanup needed
        (self.selected_zone, self.selected_point)    
    }

    fn kind(&self) -> EditorToolKind { EditorToolKind::Point }
    fn selection(&self) -> (Option<u16>, Option<u16>) { (self.selected_zone, self.selected_point) }

    fn handle_click(&mut self, state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // Reselect unless we clicked the current control point's handle
        if self.selected_point.is_none() || get_clicked_handle(self.selected_point.unwrap(), &state, pos).is_none()
        {
            (self.selected_zone, self.selected_point) = 
                if let Some((zone_id, point_id)) = utils::get_clicked_control_point(&state, None, pos) {
                    // If the clicked point has a sibling in the selected zone, select it.
                    if let Some(selected_zone_id) = self.selected_zone && let Some(sibling_id) = state.objects.sibling_in_zone(point_id, selected_zone_id) {
                        // If the clicked point's sibling in the selected zone IS our selected point (i.e. we clicked our selected point), select its sibling.
                        if let Some(selected_point_id) = self.selected_point && sibling_id == selected_point_id {
                            let next_sibling_id = state.objects.get_sibling(sibling_id)?;
                            let next_zone_id = state.objects.get_point_info(next_sibling_id).unwrap().zone_id();
                            (Some(next_zone_id), Some(next_sibling_id))
                        }

                        // Otherwise just select the clicked sibling
                        else { (Some(selected_zone_id), Some(sibling_id)) }
                    }
                    
                    // If the clicked point doesn't have a sibling in the selected zone, click what we selected.
                    else { (Some(zone_id), Some(point_id)) }
                } 
                
                // If we selected a path, select or cycle zones.
                else if let Some((zone_id, point_id, _, _)) = utils::get_clicked_zone_path(&state, pos) {
                    // If the start point has a sibling in the selected zone, possibly select that point's sibling's zone instead.
                    if let Some(selected_id) = self.selected_zone && let Some(sibling_id) = state.objects.sibling_in_zone(point_id, selected_id) {
                        let next_sibling_id = state.objects.get_sibling(sibling_id)?;
                        let next_zone_id = state.objects.get_point_info(next_sibling_id).unwrap().zone_id();

                        // Only select the next sibling if the full path is shared
                        let (start, count) = state.objects.get_zone_info(selected_id).unwrap().range();
                        let path_end_id = start + (sibling_id - start + 1) % count;
                        if state.objects.sibling_in_zone(path_end_id, next_zone_id).is_some() {
                            (Some(next_zone_id), None)
                        }

                        // Otherwise this path is not shared at all
                        else { (Some(zone_id), None) }

                    } else {
                        (Some(zone_id), None)
                    }
                }

                // If there's nothing, select nothing
                else {
                    (None, None)
                };
        }
        self.ok()
    }

    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // If we select a handle of the control point start dragging it
        if let Some(point_id) = self.selected_point && let Some(right) = get_clicked_handle(point_id, &state, pos) {
            self.handle = Some(right);
        } else {
            (self.selected_zone, self.selected_point) = 
                if let Some((zone_id, point_id)) = utils::get_clicked_control_point(&state, None, pos) {
                    // If the clicked point has a sibling in the selected zone, select it.
                    if let Some(selected_zone_id) = self.selected_zone && let Some(sibling_id) = state.objects.sibling_in_zone(point_id, selected_zone_id) {
                        (Some(selected_zone_id), Some(sibling_id))
                    }
                    else { (Some(zone_id), Some(point_id)) }
                } else {
                    (None, None)
                };
        }
        self.ok()
    }

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // Drag selected point
        if let Some(point_id) = self.selected_point {
            // If we are dragging the handle, adjust that
            if let Some(right) = self.handle {
                let handle = Some(pos - state.objects.get_point_info(point_id).unwrap().position());
                let (lh, rh) = if right { (None, handle) } else { (handle, None) };
                state.objects.update_point(point_id, None, None, lh, rh)?;
                return self.stale();
            }
            // Otherwise drag the point
            else {
                state.objects.update_point(point_id, Some(pos), None, None, None)?;
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
        if let Some(point_id) = self.selected_point {
            state.objects.delete_point(point_id)?;
            self.selected_zone = None;
            self.selected_point = None;
            self.handle = None;
            return self.stale();
        }
        self.ok()
    }
}