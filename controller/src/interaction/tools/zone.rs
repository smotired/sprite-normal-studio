use super::{EditorTool, EditorToolKind, ControllerStateInput, SelectionType, utils};
use super::result::{ToolResult, DefaultResults};

/// The Zone tool allows mananging entire zones at once. 
pub struct EditorToolZone {
    /// The zone that is selected.
    selected_zone: Option<u16>,

    /// The point that is selected.
    selected_point: Option<u16>,
}

impl EditorTool for EditorToolZone {
    fn select((selected_zone, _): SelectionType) -> Box<Self> where Self : Sized {
        Box::new(Self { selected_zone, selected_point: None })
    }

    fn deselect(&mut self) -> SelectionType {
        // No cleanup needed, but don't keep the point selected
        (self.selected_zone, None)    
    }

    fn kind(&self) -> EditorToolKind { EditorToolKind::Zone }
    fn selection(&self) -> (Option<u16>, Option<u16>) { (self.selected_zone, self.selected_point) }

    fn handle_click(&mut self, state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // Select the zone if a control point was clicked
        (self.selected_zone, self.selected_point) = 
            if let Some((zone_id, point_id)) = utils::get_clicked_control_point(&state, None, pos) {
                (Some(zone_id), Some(point_id))
            } else {
                (None, None)
            };
        self.ok()
    }

    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // Select the zone if a control point was clicked
        (self.selected_zone, self.selected_point) = 
            if let Some((zone_id, point_id)) = utils::get_clicked_control_point(&state, None, pos) {
                (Some(zone_id), Some(point_id))
            } else {
                (None, None)
            };
        self.ok()
    }

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // Drag selected zone
        if let Some(zone_id) = self.selected_zone {
            // Drag from selected point or first point
            let point_id = self.selected_point.unwrap_or(state.objects.get_zone_info(zone_id).unwrap().range().0);
            state.objects.update_zone_position(point_id, pos)?;
            return self.stale();
        }
        self.ok()
    }

    fn handle_drag_released(&mut self, _state: ControllerStateInput) -> ToolResult {
        // Release drag on selected zone. Nothing needs to happen.
        self.ok()
    }

    fn handle_cancel(&mut self, _state: ControllerStateInput) -> ToolResult { self.ok() }

    fn handle_delete(&mut self, mut state: ControllerStateInput) -> ToolResult {
        // Delete selected zone
        if let Some(zone_id) = self.selected_zone {
            state.objects.delete_zone(zone_id)?;
            self.selected_zone = None;
            self.selected_point = None;
            return self.stale();
        }
        self.ok()
    }
}