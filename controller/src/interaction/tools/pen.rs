use vector::Vec2;

use crate::objects::ControlPointMode;

use super::{EditorTool, EditorToolKind, ControllerStateInput, SelectionType, utils};
use super::result::{ToolResult, DefaultResults, EditorToolActionResult};

/// The Pen tool allows creating a new zone by dragging out a bezier path.
pub struct EditorToolPen {
    /// The zone that is selected.
    selected_zone: Option<u16>,

    /// The point that is selected.
    selected_point: Option<u16>,

    /// The handle we are manipulating, if any. True == right handle.
    handle: Option<bool>,
}

impl EditorTool for EditorToolPen {
    fn select(_: SelectionType) -> Box<Self> where Self : Sized {
        Box::new(Self { selected_zone: None, selected_point: None, handle: None })
    }

    fn deselect(&mut self) -> SelectionType {
        (self.selected_zone, self.selected_point)    
    }

    fn kind(&self) -> EditorToolKind { EditorToolKind::Pen }
    fn selection(&self) -> (Option<u16>, Option<u16>) { (self.selected_zone, self.selected_point) }

    fn handle_click(&mut self, mut state: ControllerStateInput, pos: vector::Vec2) -> ToolResult {
        // If we have a zone selected, we are already creating one, so add a linear node
        if let Some(zone_id) = self.selected_zone {
            // If we clicked a point, see if we should end the path
            if let Some((other_zone_id, point_id)) = utils::get_clicked_control_point(&state, Some(zone_id), pos) {
                // If it's the same zone, only do anything if we clicked the first point
                if other_zone_id == zone_id {
                    let (first_point, count) = state.objects.get_zone_info(zone_id).unwrap().range();
                    if point_id == first_point && count > 1 {
                        let point = state.objects.get_point_info(point_id).unwrap();
                        // If the point is continuous, update it to be broken and make its left handle linear
                        if let ControlPointMode::Continuous = point.mode() {
                            state.objects.update_point(
                                point_id, 
                                None,
                                Some(ControlPointMode::Broken),
                                Some(Vec2::ZERO),
                                None
                            )?;
                        }
                        self.selected_point = Some(point_id);

                        // Go to the Zone tool. Should keep the zone selected.
                        return self.path_complete();
                    }
                }

                // Otherwise, join the path if a path can be made to our original point via sibling points
                else {
                    // TODO

                    // Go to the Zone tool. Should keep the zone selected.
                    return self.path_complete();
                }
            }

            // Otherwise add a new linear point to the selected zone
            else {
                self.selected_point = Some(state.objects.create_point(zone_id, pos)?);
            }
        }
        // Otherwise, we should try to create a zone by adding a linear point
        else {
            // TODO: Add sibling point if there is a point here
            let zone_id = state.objects.create_zone()?;
            self.selected_zone = Some(zone_id);
            self.selected_point = Some(state.objects.create_point(zone_id, pos)?);
        }
        self.ok()
    }

    fn handle_drag_start(&mut self, mut state: ControllerStateInput, pos: vector::Vec2) -> ToolResult {
        // If we have a zone selected, we are already creating one, so add a continuous point
        if let Some(zone_id) = self.selected_zone {
            // If we clicked a point, check if it's the first point of the zone
            if let Some((other_zone_id, point_id)) = utils::get_clicked_control_point(&state, Some(zone_id), pos) {
                // If it's the same zone, only do anything if we clicked the first point and have more than one point
                if other_zone_id == zone_id {
                    let (first_point, count) = state.objects.get_zone_info(zone_id).unwrap().range();
                    if point_id == first_point && count > 1 {
                        let point = state.objects.get_point_info(point_id).unwrap();
                        // If the point is linear, update it to be broken and select it. We will start dragging its left handle.
                        if let ControlPointMode::Linear = point.mode() {
                            state.objects.update_point(point_id, None, Some(ControlPointMode::Broken), None, None)?;
                        }
                        self.selected_point = Some(point_id);
                        self.handle = Some(false);
                    }
                }

                // Otherwise, join the path if a path can be made to our original point via sibling points
                else {
                    // TODO
                }
            }

            // Otherwise add a new continuous point to the selected zone, and drag its right handle.
            else {
                let point_id = state.objects.create_point(zone_id, pos)?;
                self.selected_point = Some(point_id);
                state.objects.update_point(point_id, None, Some(ControlPointMode::Continuous), None, None)?;
                self.handle = Some(true);
            }
        }
        // Otherwise, we should try to create a zone by adding a continuous point
        else {
            // TODO: Add sibling point if there is a point here
            let zone_id = state.objects.create_zone()?;
            self.selected_zone = Some(zone_id);
            let point_id = state.objects.create_point(zone_id, pos)?;
            self.selected_point = Some(point_id);
            state.objects.update_point(point_id, None, Some(ControlPointMode::Continuous), None, None)?;
            self.handle = Some(true);
        }
        self.ok()
    }

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: vector::Vec2) -> ToolResult {
        // Assume we are dragging the handle of the currently selected point
        if let Some(point_id) = self.selected_point {
            let point_pos = state.objects.get_point_info(point_id).unwrap().position();
            let (first_point_id, _) = state.objects.get_zone_info(self.selected_zone.unwrap()).unwrap().range();
            let handle = pos - point_pos;
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
        self.handle = None;

        // If the selected point is the first point of the path, and the path has more than one point, finalize the path creation
        if let Some(zone_id) = self.selected_zone && let Some(point_id) = self.selected_point {
            let (start_point, point_count) = state.objects.get_zone_info(zone_id).unwrap().range();
            if point_id == start_point && point_count > 1 {
                return self.path_complete();
            }
        }
        self.ok()
    }

    fn handle_cancel(&mut self, mut state: ControllerStateInput) -> ToolResult {
        // If we are creating a zone, delete the whole zone.
        if let Some(zone_id) = self.selected_zone {
            state.objects.delete_zone(zone_id)?;
            self.selected_zone = None;
            self.selected_point = None;
            self.handle = None;
        }
        self.ok()
    }

    fn handle_delete(&mut self, mut state: ControllerStateInput) -> ToolResult {
        if let Some(_) = self.selected_zone {
            if state.objects.delete_point_in_wip_path(self.selected_point.unwrap())? {
                // The path is now empty, the whole zone was deleted.
                self.selected_zone = None;
                self.selected_point = None;
            } else {
                self.selected_point = Some(self.selected_point.unwrap() - 1); // should be the previous point
            }
            self.handle = None;
        }
        self.ok()
    }
}

impl EditorToolPen {
    /// Return a result for when the path is complete and we should move to another zone.
    fn path_complete(&self) -> ToolResult {
        Ok(EditorToolActionResult::new(EditorToolKind::Zone, true))
    }
}