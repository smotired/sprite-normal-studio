use studio_math::Vec2;

use super::{EditorTool, EditorToolKind, ControllerStateInput, SelectionType, utils};
use super::result::{ToolResult, DefaultResults};

/// The Zone tool allows mananging entire zones at once. 
pub struct EditorToolZone {
    /// The zone that is selected.
    selected_zone: Option<u16>,

    /// The reference point for if we are dragging a zone, relative to the position of the first point.
    reference_delta: Option<Vec2>,
}

impl EditorTool for EditorToolZone {
    fn select((selected_zone, _): SelectionType) -> Box<Self> where Self : Sized {
        Box::new(Self { selected_zone, reference_delta: None })
    }

    fn deselect(&mut self) -> SelectionType {
        // No cleanup needed, but don't keep the point selected
        (self.selected_zone, None)    
    }

    fn kind(&self) -> EditorToolKind { EditorToolKind::Zone }
    fn selection(&self) -> (Option<u16>, Option<u16>) { (self.selected_zone, None) }

    fn handle_click(&mut self, state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // Select the zone if a control point or the path was clicked
        self.selected_zone = 
            if let Some((zone_id, _)) = utils::get_clicked_control_point(&state, None, pos) {
                Some(zone_id)
            } else if let Some((zone_id, _, _, _)) = utils::get_clicked_zone_path(&state, pos) {
                Some(zone_id)
            } else {
                None
            };
        self.ok()
    }

    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // Select the zone if a control point or the path was clicked
        (self.selected_zone, self.reference_delta) = 
            if let Some((zone_id, point_id)) = utils::get_clicked_control_point(&state, None, pos) {
                let point = state.objects.get_point_info(point_id).unwrap();

                let (start_id, _) = state.objects.get_zone_info(zone_id).unwrap().range();
                let first_point = state.objects.get_point_info(start_id).unwrap();

                (Some(zone_id), Some(point.position() - first_point.position()))
            } else if let Some((zone_id, _, corrected, _)) = utils::get_clicked_zone_path(&state, pos) {
                let (start_id, _) = state.objects.get_zone_info(zone_id).unwrap().range();
                let first_point = state.objects.get_point_info(start_id).unwrap();
                
                (Some(zone_id), Some(corrected - first_point.position()))
            } else {
                (None, None)
            };
        self.ok()
    }

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // Drag selected zone
        if let Some(zone_id) = self.selected_zone {
            // Move the first point to the position
            state.objects.update_zone_position(zone_id, pos - self.reference_delta.unwrap())?;
            return self.stale();
        }
        self.ok()
    }

    fn handle_drag_released(&mut self, _state: ControllerStateInput) -> ToolResult {
        // Release drag on selected zone, but keep zone selected
        self.reference_delta = None;
        self.ok()
    }

    fn handle_cancel(&mut self, _state: ControllerStateInput) -> ToolResult { self.ok() }

    fn handle_delete(&mut self, mut state: ControllerStateInput) -> ToolResult {
        // Delete selected zone
        if let Some(zone_id) = self.selected_zone {
            state.objects.delete_zone(zone_id)?;
            self.selected_zone = None;
            self.reference_delta = None;
            return self.stale();
        }
        self.ok()
    }
}