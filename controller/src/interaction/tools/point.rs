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
                    (Some(zone_id), Some(point_id))
                } else {
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
                    (Some(zone_id), Some(point_id))
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