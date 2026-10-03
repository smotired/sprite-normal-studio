use studio_math::Vec2;
use studio_math::bezier::bezier_split_at;

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

    /// ID of the point we are branching off of if we are creating a connected zone
    branch_point_id: Option<u16>,

    /// Only used when joining multiple source paths.
    joined_source: bool,
}

impl EditorTool for EditorToolPen {
    fn select(_: SelectionType) -> Box<Self> where Self : Sized {
        Box::new(Self { selected_zone: None, selected_point: None, handle: None, branch_point_id: None, joined_source: false })
    }

    fn deselect(&mut self) -> SelectionType {
        (self.selected_zone, self.selected_point)    
    }

    fn kind(&self) -> EditorToolKind { EditorToolKind::Pen }
    fn selection(&self) -> (Option<u16>, Option<u16>) { (self.selected_zone, self.selected_point) }

    fn handle_click(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // If we have a zone selected, we are already creating one, so add a linear node
        if let Some(zone_id) = self.selected_zone {
            // If we clicked a point, see if we should end the path
            if let Some((other_zone_id, point_id)) = utils::get_clicked_control_point(&state, None, pos) {
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

                        // Go to the Point tool. Should keep the zone and point selected.
                        return self.path_complete();
                    }
                }

                // Otherwise, join the path if a path can be made to our original point via sibling points
                else if let Some(source_id) = self.branch_point_id {
                    let created_ids = state.objects.complete_branching_zone(source_id, zone_id, point_id)?;
                    // If nothing was created, we have joined our original point.
                    let point_id = if created_ids.is_empty() {
                        state.objects.get_zone_info(zone_id).unwrap().range().0
                    } else { created_ids[0] };

                    self.selected_point = Some(point_id);

                    // Go to the Point tool. Should keep the zone and point selected.
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
            // If we clicked a point, start a path with a sibling node
            if let Some((_, sibling_id)) = utils::get_clicked_control_point(&state, None, pos) {
                let (zone_id, point_id) = state.objects.create_branching_zone(sibling_id)?;
                self.selected_zone = Some(zone_id);
                self.selected_point = Some(point_id);
                self.branch_point_id = Some(sibling_id);
            }

            // If we clicked a path, add a vertex to that zone, select it, and move to Point tool
            else if let Some((zone_id, start_id, corrected, t)) = utils::get_clicked_zone_path(&state, pos) {
                // Get the points at the start and end of the zone
                let start = state.objects.get_point_info(start_id).unwrap();
                let end = {
                    let (start, count) = state.objects.get_zone_info(zone_id).unwrap().range();
                    let end_id = if start_id + 1 == start + count { start } else { start_id + 1 };
                    state.objects.get_point_info(end_id).unwrap()
                };
                
                // Determine mode and handles for the path to not change
                let (mode, left_handle, right_handle, start_rh, end_lh) = {
                    // If both are linear, this should be linear.
                    if let ControlPointMode::Linear = start.mode() && let ControlPointMode::Linear = end.mode() {
                        (ControlPointMode::Linear, None, None, None, None)
                    } 
                    
                    // Otherwise it should be continuous.
                    else {
                        let (start_r, new_l, _, new_r, end_l) = bezier_split_at(
                            start.position(),
                            start.right_handle(),
                            end.left_handle(),
                            end.position(),
                            t
                        );

                        (
                            ControlPointMode::Continuous,
                            Some(new_l - corrected),
                            Some(new_r - corrected),
                            Some(start_r - start.position()),
                            Some(end_l - end.position()),
                        )
                    }
                };

                // Add the point and update its mode/handles
                let point_id = state.objects.insert_point(zone_id, start_id + 1, corrected)?; // Insert after start of segment
                state.objects.update_point(point_id, None, Some(mode), left_handle, right_handle)?;

                // Also update the right and left handles of previous and next points, respectively.
                // Theoretically they should be on the same lines, so even if continuous they shouldn't affect other parts of the curve
                if let Some(handle) = start_rh {
                    state.objects.update_point(start_id, None, None, None, Some(handle))?;
                }
                if let Some(handle) = end_lh {
                    let end_id = {
                        let (start, count) = state.objects.get_zone_info(zone_id).unwrap().range();
                        if point_id + 1 == start + count { start } else { point_id + 1 }
                    };
                    state.objects.update_point(end_id, None, None, Some(handle), None)?;
                }

                // Select and return
                self.selected_zone = Some(zone_id);
                self.selected_point = Some(point_id);
                return Ok(EditorToolActionResult::new(EditorToolKind::Point, true))
            }

            // Otherwise start creating a new path.
            else {
                let zone_id = state.objects.create_zone()?;
                self.selected_zone = Some(zone_id);
                self.selected_point = Some(state.objects.create_point(zone_id, pos)?);
            }
        }
        self.ok()
    }

    fn handle_drag_start(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // If we have a zone selected, we are already creating one, so add a continuous point
        if let Some(zone_id) = self.selected_zone {
            // If we clicked a point, check if it's the first point of the zone
            if let Some((other_zone_id, point_id)) = utils::get_clicked_control_point(&state, None, pos) {
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
                else if let Some(source_id) = self.branch_point_id {
                    let created_ids = state.objects.complete_branching_zone(source_id, zone_id, point_id)?;
                    // If nothing was created, we have joined our original point.
                    let point_id = if created_ids.is_empty() {
                        state.objects.get_zone_info(zone_id).unwrap().range().0
                    } else { created_ids[0] };

                    // Determine which handle to drag
                    let right = {
                        let point = state.objects.get_point_info(point_id).unwrap();
                        point.sync_mode().syncing_left()
                    };

                    self.selected_point = Some(point_id);
                    self.handle = Some(right);
                    self.joined_source = true;
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
            // If we clicked a point, start a path with a continuous sibling node and start dragging it
            if let Some((_, sibling_id)) = utils::get_clicked_control_point(&state, None, pos) {
                let (zone_id, point_id) = state.objects.create_branching_zone(sibling_id)?;
                self.selected_zone = Some(zone_id);
                self.selected_point = Some(point_id);
                self.branch_point_id = Some(sibling_id);
            }

            // If we clicked a path, add a vertex to that zone, select it, and move to Point tool
            else if let Some((zone_id, start_id, corrected, t)) = utils::get_clicked_zone_path(&state, pos) {
                // Get the points at the start and end of the zone
                let start = state.objects.get_point_info(start_id).unwrap();
                let end = {
                    let (start, count) = state.objects.get_zone_info(zone_id).unwrap().range();
                    let end_id = if start_id + 1 == start + count { start } else { start_id + 1 };
                    state.objects.get_point_info(end_id).unwrap()
                };
                
                // Determine mode and handles for the path to not change
                let (mode, lh, rh, start_rh, end_lh) = {
                    // If both are linear, this should be linear.
                    if let ControlPointMode::Linear = start.mode() && let ControlPointMode::Linear = end.mode() {
                        (ControlPointMode::Linear, None, None, None, None)
                    } 
                    
                    // Otherwise it should be continuous.
                    else {
                        let (start_r, new_l, _, new_r, end_l) = bezier_split_at(
                            start.position(),
                            start.right_handle(),
                            end.left_handle(),
                            end.position(),
                            t
                        );

                        (
                            ControlPointMode::Continuous,
                            Some(new_l - corrected),
                            Some(new_r - corrected),
                            Some(start_r - start.position()),
                            Some(end_l - end.position()),
                        )
                    }
                };

                // Add the point and update its mode/handles
                let point_id = state.objects.insert_point(zone_id, start_id + 1, corrected)?; // Insert after start of segment
                state.objects.update_point(point_id, None, Some(mode), lh, rh)?;

                // Also update the right and left handles of previous and next points, respectively.
                // Theoretically they should be on the same lines, so even if continuous they shouldn't affect other parts of the curve
                if let Some(handle) = start_rh {
                    state.objects.update_point(start_id, None, None, None, Some(handle))?;
                }
                if let Some(handle) = end_lh {
                    let end_id = {
                        let (start, count) = state.objects.get_zone_info(zone_id).unwrap().range();
                        if point_id + 1 == start + count { start } else { point_id + 1 }
                    };
                    state.objects.update_point(end_id, None, None, Some(handle), None)?;
                }

                // Select and return
                self.selected_zone = Some(zone_id);
                self.selected_point = Some(point_id);
                return Ok(EditorToolActionResult::new(EditorToolKind::Point, true))
            }

            // Otherwise start creating a new path from a continuous node
            else {
                let zone_id = state.objects.create_zone()?;
                self.selected_zone = Some(zone_id);
                let point_id = state.objects.create_point(zone_id, pos)?;
                self.selected_point = Some(point_id);
                state.objects.update_point(point_id, None, Some(ControlPointMode::Continuous), None, None)?;
                self.handle = Some(true);
            }
        }
        self.ok()
    }

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
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

        // If the path has more than one point, and some other condition is met, finalize the path creation
        if let Some(zone_id) = self.selected_zone && let Some(point_id) = self.selected_point {
            let (start_point, point_count) = state.objects.get_zone_info(zone_id).unwrap().range();
            if point_count > 1 {
                if point_id == start_point || self.joined_source {
                    return self.path_complete();
                }
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