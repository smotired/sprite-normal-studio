use studio_math::Vec2;

use crate::InputModifiers;

use super::{EditorTool, EditorToolKind, ControllerStateInput, SelectionType, utils};
use super::result::{ToolResult, DefaultResults};

// TODO: Modifier keys for dragging
//
// dragging position:
// Shift - move along axis
// Ctrl - snap to grid (1px grid for now)
//
// dragging box:
// shift: add to selection
// alt: remove from selection
// ctrl: only select exact zones
// same multi rules as below except toggle affects the normal selected list instead of the tentative list

// also maybe only actually make it save the changed selection when ending drag, so you can like cancel or whatever. add a tentative_selection list, and join em in selection()

/// The Zone tool allows mananging entire zones at once. 
/// 
/// SELECTION RULES:
/// SHIFT: add to selection instead of replacing
/// ALT: remove from selection instead of replacing
/// CTRL: select a single zone instead of all connected zones
/// 
/// Rules that follow from these:
/// Ctrl + Shift: add single zone to selection
/// Ctrl + Alt: remove single zone from selection
/// Shift + Alt: toggle zones in selection
/// Shift + Alt + Ctrl: toggle single zone in selected
/// 
/// When clicking and dragging instead of just clicking: should only change selection if not clicking on a selected zone
#[derive(Default)]
pub struct EditorToolZone {
    /// The zone that is selected.
    selected_zones: Vec<u16>,

    /// Reference to selected points, generated when doing selection()
    selected_points_reference: Vec<u16>,

    /// The reference point for if we are dragging a zone, relative to the position of the first point.
    reference_delta: Option<Vec2>,

    /// The start point for our selection box if we are instead dragging out a box of zones
    selection_origin: Option<Vec2>,

    /// The end point for our selection box if we are instead dragging out a box of zones
    selection_box_end: Option<Vec2>,
}

impl EditorTool for EditorToolZone {
    fn init() -> Box<Self> where Self : Sized { Box::default() }

    fn select(selected_points: SelectionType, state: ControllerStateInput) -> Box<Self> where Self : Sized {
        // Get all zones with a point selected
        let selected_zones: std::collections::HashSet<_> = selected_points.iter().filter_map(|&point_id| {
            state.objects.get_point_info(point_id).map(|info| info.zone_id())
        }).collect();
        let selected_zones: Vec<_> = selected_zones.into_iter().collect();

        let selected_points_reference = points_from_zone_selection(&selected_zones[..], &state);

        Box::new(Self { selected_zones, selected_points_reference, reference_delta: None, selection_origin: None, selection_box_end: None })
    }

    fn deselect(&mut self, _state: ControllerStateInput) -> SelectionType<'_> {
        // Just return selection
        &self.selected_points_reference[..]
    }

    fn kind(&self) -> EditorToolKind { EditorToolKind::Zone }
    fn selection(&self) -> SelectionType<'_> { &self.selected_points_reference[..] }
    fn selection_box(&self) -> Option<(Vec2, Vec2)> {
        if let (Some(start), Some(end)) = (self.selection_origin, self.selection_box_end) {
            Some((start, end))
        } else {
            None
        }
    }

    fn handle_click(&mut self, state: ControllerStateInput, pos: studio_math::Vec2, modifiers: InputModifiers) -> ToolResult {
        let (new_selection, _) = self.get_updated_selection(&state, pos, false, modifiers);
        self.set_selection(new_selection, &state);
        self.ok()
    }

    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: studio_math::Vec2, modifiers: InputModifiers) -> ToolResult {
        let clicked_zone_info = self.zone_from_pos(&state, pos);
        let (new_selection, reference_delta) = self.get_updated_selection(&state, pos, true, modifiers);

        // If we clicked a zone, only update selection if the clicked zone isn't selected.
        if let Some(clicked_zone_id) = clicked_zone_info.0 {
            // Determine if this (or a sibling) is in the selected zone
            let mut selected = self.selected_zones.contains(&clicked_zone_id);
            if !selected && let Some(clicked_point_id) = clicked_zone_info.1 {
                for &zone_id in &self.selected_zones {
                    if let Some(sibling_id) = state.objects.sibling_in_zone(clicked_point_id, zone_id) { // can prolly make a helper for this
                        if clicked_zone_info.2.is_none() {
                            selected = true;
                            break;
                        } else if let Some(end_id) = state.objects.sibling_in_zone(clicked_zone_info.2.unwrap(), zone_id) {
                            let (start, count) = state.objects.get_zone_info(zone_id).unwrap().range();
                            let expected_next_id = start + (sibling_id - start + 1) % count;
                            if expected_next_id == end_id {
                                selected = true;
                                break;
                            }
                        }
                    }
                }
            }

            // Update selection if not in selected zone
            if !selected {
                self.set_selection(new_selection, &state);
                self.reference_delta = reference_delta;
            }

            // Otherwise keep selection as is and recalculate reference delta
            else {
                let first_zone_id = *self.selected_zones.iter().min().unwrap();
                let (first_point_id, _) = state.objects.get_zone_info(first_zone_id).unwrap().range();
                let first_point_position = state.objects.get_point_info(first_point_id).unwrap().position();
                self.reference_delta = Some(pos - first_point_position);
            }
        }

        // If we didn't click a zone, just update the selection
        else {
            // If new_selection is empty, start dragging a box instead of the reference delta. When released, will select everything within the box.
            if new_selection.is_empty() {
                self.selection_origin = Some(pos);
                self.selection_box_end = Some(pos);
            } else {
                self.reference_delta = reference_delta;
            }

            self.set_selection(new_selection, &state);
        }
        self.ok()
    }

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2, _modifiers: InputModifiers) -> ToolResult {
        // Drag selected zones if we are doing that
        if let Some(reference_delta) = self.reference_delta && !self.selected_zones.is_empty() {
            // Move the first point to the position across all the selected zones.
            state.objects.move_zones(&self.selected_zones[..], pos - reference_delta)?;
            return self.stale();
        }
        // Or if we are dragging a selection box, set our selected zones to be the ones in the box
        else if let Some(selection_origin) = self.selection_origin {
            self.selection_box_end = Some(pos);
            let selected_points = state.objects.get_points_in_rect(selection_origin, pos);
            let mut selected_zones = std::collections::HashSet::new();
            for point_id in selected_points {
                let zone_id = state.objects.get_point_info(point_id).unwrap().zone_id();
                let all_connected = get_connected_zones(zone_id, &state);
                selected_zones.extend(all_connected);
            }
            self.set_selection(selected_zones.into_iter().collect(), &state);
        }
        self.ok()
    }

    fn handle_drag_released(&mut self, _state: ControllerStateInput, _modifiers: InputModifiers) -> ToolResult {
        // Release drag on selected zones, but keep them selected
        self.reference_delta = None;
        self.selection_origin = None;
        self.selection_box_end = None;
        self.ok()
    }

    fn handle_cancel(&mut self, state: ControllerStateInput) -> ToolResult {
        // Clear selection
        self.set_selection(vec![], &state);
        self.selection_origin = None;
        self.selection_box_end = None;
        self.ok()
    }

    fn handle_delete(&mut self, mut state: ControllerStateInput) -> ToolResult {
        // Stop dragging out selection box
        if self.selection_origin.is_some() {
            self.selection_origin = None;
            self.selection_box_end = None;
        }
        // Or Delete selected zones
        else if !self.selected_zones.is_empty() {
            self.selected_zones.sort();
            let mut offset = 0;
            #[allow(clippy::explicit_counter_loop)] // offset counts deletions, not iterations
            for zone_id in &self.selected_zones[..] {
                state.objects.delete_zone(*zone_id - offset)?;
                offset += 1;
            }

            self.set_selection(Vec::new(), &state);
            self.reference_delta = None;

            return self.stale();
        }
        self.ok()
    }
}

impl EditorToolZone {
    fn set_selection(&mut self, selected_zones: Vec<u16>, state: &ControllerStateInput) {
        self.selected_zones = selected_zones;
        self.selected_points_reference = points_from_zone_selection(&self.selected_zones[..], state);
    }

    /// Get a single zone selection from a click position.
    /// Returns the ID of the zone that was clicked, and the ID of the clicked point or its path start/end
    fn zone_from_pos(&self, state: &ControllerStateInput, pos: Vec2) -> (Option<u16>, Option<u16>, Option<u16>) {
        // Check the point
        if let Some((zone_id, point_id)) = utils::get_clicked_control_point(&state, None, pos) {
            (Some(zone_id), Some(point_id), None)
        }

        // Check the curve
        else if let Some((zone_id, start_id, _, _)) = utils::get_clicked_zone_path(&state, pos) {
            let (start, count) = state.objects.get_zone_info(zone_id).unwrap().range();
            (Some(zone_id), Some(start_id), Some(start + (start_id - start + 1) % count))
        }

        // Notihing is clicked
        else {
            (None, None, None)
        }
    }

    /// Given a click (or drag start) point and input info, and whether or not this is a drag action,
    /// and using the state of the current selection, return a new selection.
    /// Also return delta from the lowest ID selected point to the click position, in case this is a drag start.
    fn get_updated_selection(&self, state: &ControllerStateInput, pos: Vec2, dragging: bool, modifiers: InputModifiers) -> (Vec<u16>, Option<Vec2>) {
        let (zone_id, point_id, neighbor_id) = self.zone_from_pos(&state, pos);

        // If a zone is clicked, determine point/path/etc
        if let Some(zone_id) = zone_id {
            let toggling = modifiers.shift && modifiers.alt;

            // If ctrl is clicked, only operate on the exact clicked zone
            let new_selection = if modifiers.ctrl {
                // If a modifier key is selected do what you would expect
                if toggling {
                    let mut selected: std::collections::HashSet<_> = self.selected_zones.clone().into_iter().collect();
                    if !selected.insert(zone_id) { selected.remove(&zone_id); } // if it was already present, remove it
                    selected.into_iter().collect()
                } else if modifiers.shift {
                    let mut selected: std::collections::HashSet<_> = self.selected_zones.clone().into_iter().collect();
                    selected.insert(zone_id);
                    selected.into_iter().collect()
                } else if modifiers.alt {
                    let mut selected: std::collections::HashSet<_> = self.selected_zones.clone().into_iter().collect();
                    selected.remove(&zone_id);
                    selected.into_iter().collect()
                }
                
                // Otherwise, we are just selecting a single zone.
                // Decide if we should cycle selection.
                else {
                    // If we don't have exactly one zone selected already, just return the zone
                    if self.selected_zones.len() != 1 { vec![zone_id] }

                    // If we have a first point ID
                    else if let Some(point_id) = point_id {
                        let selected_zone_id = self.selected_zones[0];
                        // If the point we clicked has a sibling in this zone
                        if let Some(point_id) = state.objects.sibling_in_zone(point_id, selected_zone_id)  {
                            // If we have an endpoint id, determine if this is a shared path
                            if let Some(neighbor_id) = neighbor_id {
                                if let Some(neighbor_id) = state.objects.sibling_in_zone(neighbor_id, selected_zone_id) {
                                    // If it is an interior path, return the next zone.
                                    let (start, count) = state.objects.get_zone_info(selected_zone_id).unwrap().range();
                                    let projected_next_id = start + (point_id - start + 1) % count;
                                    if neighbor_id == projected_next_id {
                                        let sibling_id = state.objects.get_sibling(point_id).unwrap();
                                        vec![state.objects.get_point_info(sibling_id).unwrap().zone_id()]
                                    } else { vec![zone_id] }
                                } else { vec![zone_id] } // return selected zone
                            } else {
                                // Otherwise, just return the next zone.
                                let sibling_id = state.objects.get_sibling(point_id).unwrap();
                                vec![state.objects.get_point_info(sibling_id).unwrap().zone_id()]
                            }
                        }

                        // Otherwise it's a different point in the selected zone
                        else { vec![zone_id] }
                    }

                    // Otherwise just return the zone
                    else { vec![zone_id] }
                }
            }

            // Otherwise, add/remove/toggle all selected zones
            else {
                let all_connected = get_connected_zones(zone_id, &state);
                
                if toggling {
                    // Remove if and only if all were already selected
                    let mut selected: std::collections::HashSet<_> = self.selected_zones.clone().into_iter().collect();

                    if all_connected.iter().all(|id| selected.contains(id)) {
                        for zone_id in all_connected { selected.remove(&zone_id); }
                    } else {
                        for zone_id in all_connected { selected.insert(zone_id); }
                    }
                    
                    selected.into_iter().collect()
                } else if modifiers.shift {
                    let mut selected: std::collections::HashSet<_> = self.selected_zones.clone().into_iter().collect();
                    for zone_id in all_connected { selected.insert(zone_id); }
                    selected.into_iter().collect()
                } else if modifiers.alt {
                    let mut selected: std::collections::HashSet<_> = self.selected_zones.clone().into_iter().collect();
                    for zone_id in all_connected { selected.remove(&zone_id); }
                    selected.into_iter().collect()
                } else {
                    all_connected
                }
            };

            let reference_delta = if !dragging || new_selection.is_empty() { None } else {
                // Get the difference in position with the first point in the first zone
                let first_zone_id = *new_selection.iter().min().unwrap();
                let (first_point_id, _) = state.objects.get_zone_info(first_zone_id).unwrap().range();
                let first_point_position = state.objects.get_point_info(first_point_id).unwrap().position();
                Some(pos - first_point_position)
            };

            (new_selection, reference_delta)
        }

        // If no zone is clicked, deselect or return selection unmodified
        else {
            if modifiers.shift || modifiers.alt {
                // We are adding to/removing from/toggling selected zones, so leave unmodified
                (self.selected_zones.clone(), None)
            } else {
                (vec![], None)
            }
        }
    }
}

/// Get the points we have selected from the zone we have selected
fn points_from_zone_selection(selected_zones: &[u16], state: &ControllerStateInput) -> Vec<u16> {
    let mut points = vec![];
    for zone_id in selected_zones {
        let (start, count) = state.objects.get_zone_info(*zone_id).unwrap().range();
        points.extend((start..start + count).collect::<Vec<u16>>());
    }
    points
}

fn get_connected_zones(zone_id: u16, state: &ControllerStateInput) -> Vec<u16> {
    let (point_id, _) = state.objects.get_zone_info(zone_id).unwrap().range();
    let connected_points = state.objects.get_connected_points(point_id, true).unwrap();
    let connected_zones: std::collections::HashSet<_> = connected_points.iter().filter_map(|&point_id| {
        state.objects.get_point_info(point_id).map(|info| info.zone_id())
    }).collect(); // Collect as hash set first to remove duplicates
    connected_zones.into_iter().collect()
}

/* ******************************** */
/*           TESTS MODULE           */
/* ******************************** */

#[cfg(test)]
mod tests {
use crate::{ObjectBuffers, InputModifiers as IM};
    use crate::objects::test_utils::{add_square, add_zone};

    use super::*;
    use super::super::test_utils::state;

    /// A tool with nothing selected
    #[test]
    fn init() {
        let tool = EditorToolZone::init();
        assert_eq!(tool.kind(), EditorToolKind::Zone);
        assert!(tool.selection().is_empty());
    }

    /// Selecting from points selects every point of the zones they are in
    #[test]
    fn select_from_points() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);

        let tool = EditorToolZone::select(&[5], state(&objects));
        assert_eq!(tool.selection(), &[4, 5, 6]);
    }

    /// Clicking a control point or a path selects the zone, and clicking nothing deselects
    #[test]
    fn click_selects_zone() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(40.0, 0.0), (140.0, 0.0), (140.0, 100.0)]);
        let mut tool = EditorToolZone::init();

        let result = tool.handle_click(state(&objects), Vec2::new(10.5, 0.0), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert!(!result.normals_stale);
        assert_eq!(tool.selection(), &[0, 1, 2, 3]);

        tool.handle_click(state(&objects), Vec2::new(90.0, 1.0), Default::default()).unwrap(); // path
        assert_eq!(tool.selection(), &[4, 5, 6]);

        tool.handle_click(state(&objects), Vec2::new(25.0, 25.0), Default::default()).unwrap();
        assert!(tool.selection().is_empty());
    }

    /// Clicking a control point or a path selects the all connected zones
    #[test]
    fn click_selects_all_connected() {
        let assert_eq_contents = |a: &[u16], b: &[u16]| {
            assert_eq!(
                a.into_iter().map(|&item| item).collect::<std::collections::HashSet<u16>>(),
                b.into_iter().map(|&item| item).collect::<std::collections::HashSet<u16>>(),
            );
        };

        let mut objects = ObjectBuffers::headless();

        // Add one set of connected zones
        add_square(&mut objects, 10.0);
        let (branch_zone, _) = objects.create_branching_zone(2).unwrap();
        objects.create_point(branch_zone, Vec2::new(10.0, 20.0)).unwrap();
        objects.create_point(branch_zone, Vec2::new(0.0, 20.0)).unwrap();
        objects.complete_branching_zone(2, branch_zone, 3).unwrap();

        // Add another set of connected zones
        add_zone(&mut objects, &[(40.0, 0.0), (140.0, 0.0), (140.0, 100.0)]);
        let (branch_zone, _) = objects.create_branching_zone(8).unwrap();
        objects.create_point(branch_zone, Vec2::new(40.0, 100.0)).unwrap();
        objects.complete_branching_zone(8, branch_zone, 10).unwrap();

        let mut tool = EditorToolZone::init();

        let result = tool.handle_click(state(&objects), Vec2::new(10.5, 0.0), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert!(!result.normals_stale);
        assert_eq_contents(tool.selection(), &[0, 1, 2, 3, 4, 5, 6, 7]);

        tool.handle_click(state(&objects), Vec2::new(90.0, 1.0), Default::default()).unwrap(); // path
        assert_eq_contents(tool.selection(), &[8, 9, 10, 11, 12, 13]);

        tool.handle_click(state(&objects), Vec2::new(25.0, 25.0), Default::default()).unwrap();
        assert!(tool.selection().is_empty());
    }

    /// Clicking a control point or a path selects the all connected zones
    #[test]
    fn ctrl_click_selects_only_zone() {
        let mut objects = ObjectBuffers::headless();

        // Add a set of connected zones
        add_square(&mut objects, 10.0);
        let (branch_zone, _) = objects.create_branching_zone(2).unwrap();
        objects.create_point(branch_zone, Vec2::new(10.0, 20.0)).unwrap();
        objects.create_point(branch_zone, Vec2::new(0.0, 20.0)).unwrap();
        objects.complete_branching_zone(2, branch_zone, 3).unwrap();

        let mut tool = EditorToolZone::init();

        // Ctrl-click on a single zone and ensure only that zone is selected
        let result = tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), IM::ctrl()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert!(!result.normals_stale);
        assert_eq!(tool.selection(), &[0, 1, 2, 3]);

        // Ctrl+click on the other zone and ensure only it is selected
        tool.handle_click(state(&objects), Vec2::new(0.0, 20.0), IM::ctrl()).unwrap();
        assert_eq!(tool.selection(), &[4, 5, 6, 7]);

        // Ctrl+click on a shared path and ensure only one or the other is selected
        tool.handle_click(state(&objects), Vec2::new(0.0, 20.0), IM::ctrl()).unwrap();
        assert_eq!(tool.selection().len(), 4);

        // Click nothing and ensure deselect
        tool.handle_click(state(&objects), Vec2::new(25.0, 25.0), IM::ctrl()).unwrap();
        assert!(tool.selection().is_empty());
    }

    /// Clicking with shift adds to selection, clicking with alt removes from selection.
    #[test]
    fn adding_or_removing_selection() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(40.0, 0.0), (140.0, 0.0), (140.0, 100.0)]);
        let mut tool = EditorToolZone::init();
        
        let assert_selected_zones = |tool: &EditorToolZone, zone_ids: &[u16]| {
            let mut selection = vec![];
            for &zone_id in zone_ids {
                match zone_id {
                    0 => selection.extend_from_slice(&[0, 1, 2, 3]),
                    1 => selection.extend_from_slice(&[4, 5, 6]),
                    _ => { }
                }
            }
            selection.sort();
            let mut actual_selection = Vec::from(tool.selection());
            actual_selection.sort();
            assert_eq!(actual_selection, &selection[..]);
        };

        let result = tool.handle_click(state(&objects), Vec2::new(10.5, 0.0), IM::shift()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert!(!result.normals_stale);
        assert_selected_zones(&tool, &[0]);

        tool.handle_click(state(&objects), Vec2::new(90.0, 1.0), IM::shift()).unwrap(); // path
        assert_selected_zones(&tool, &[0, 1]);

        tool.handle_click(state(&objects), Vec2::new(25.0, 25.0), IM::shift()).unwrap();
        assert_selected_zones(&tool, &[0, 1]);

        tool.handle_click(state(&objects), Vec2::new(25.0, 25.0), IM::alt()).unwrap();
        assert_selected_zones(&tool, &[0, 1]);

        tool.handle_click(state(&objects), Vec2::new(0.0, 5.0), IM::alt()).unwrap(); // path
        assert_selected_zones(&tool, &[1]);

        tool.handle_click(state(&objects), Vec2::new(140.0, 0.0), IM::alt()).unwrap();
        assert!(tool.selection().is_empty());
    }

    /// Test different combinations with multiple modifier keys
    #[test]
    fn multiple_modifier_keys() {
            let mut objects = ObjectBuffers::headless();

        // Add one set of connected zones
        add_square(&mut objects, 10.0);
        let (branch_zone, _) = objects.create_branching_zone(2).unwrap();
        objects.create_point(branch_zone, Vec2::new(10.0, 20.0)).unwrap();
        objects.create_point(branch_zone, Vec2::new(0.0, 20.0)).unwrap();
        objects.complete_branching_zone(2, branch_zone, 3).unwrap();

        // Add another set of connected zones
        add_zone(&mut objects, &[(40.0, 0.0), (140.0, 0.0), (140.0, 100.0)]);
        let (branch_zone, _) = objects.create_branching_zone(8).unwrap();
        objects.create_point(branch_zone, Vec2::new(40.0, 100.0)).unwrap();
        objects.complete_branching_zone(8, branch_zone, 10).unwrap();

        let mut tool = EditorToolZone::init();
        let assert_selected_zones = |tool: &EditorToolZone, zone_ids: &[u16]| {
            let mut selection = vec![];
            for &zone_id in zone_ids {
                match zone_id {
                    0 => selection.extend_from_slice(&[0, 1, 2, 3]),
                    1 => selection.extend_from_slice(&[4, 5, 6, 7]),
                    2 => selection.extend_from_slice(&[8, 9, 10]),
                    3 => selection.extend_from_slice(&[11, 12, 13]),
                    _ => { }
                }
            }
            selection.sort();
            let mut actual_selection = Vec::from(tool.selection());
            actual_selection.sort();
            assert_eq!(actual_selection, &selection[..]);
        };

        // Start by selecting only zone 0
        let result = tool.handle_click(state(&objects), Vec2::new(10.5, 0.0), IM::ctrl()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert!(!result.normals_stale);
        assert_selected_zones(&tool, &[0]);

        // Ctrl+Shift to select a zone on the other graph
        tool.handle_click(state(&objects), Vec2::new(90.0, 1.0), IM::ctrl_shift()).unwrap(); // path
        assert_selected_zones(&tool, &[0, 2]);

        // Shift+Alt on second connected to toggle the whole first graph on
        tool.handle_click(state(&objects), Vec2::new(0.0, 20.0), IM::alt_shift()).unwrap();
        assert_selected_zones(&tool, &[0, 1, 2]);

        // Shift+Alt again to toggle the whole first graph off
        tool.handle_click(state(&objects), Vec2::new(0.0, 20.0), IM::alt_shift()).unwrap();
        assert_selected_zones(&tool, &[2]);

        // Shift to turn the whole first graph back on
        tool.handle_click(state(&objects), Vec2::new(0.0, 20.0), IM::shift()).unwrap();
        assert_selected_zones(&tool, &[0, 1, 2]);

        // Ctrl+Alt on second to deselect it
        tool.handle_click(state(&objects), Vec2::new(0.0, 20.0), IM::ctrl_alt()).unwrap();
        assert_selected_zones(&tool, &[0, 2]);

        // Alt on second to deselect whole first graph, even though the clicked zone isn't selected
        tool.handle_click(state(&objects), Vec2::new(0.0, 20.0), IM::alt()).unwrap();
        assert_selected_zones(&tool, &[2]);

        // Shift+Ctrl+Alt to toggle a zone on the first graph on
        tool.handle_click(state(&objects), Vec2::new(0.0, 20.0), IM::all()).unwrap();
        assert_selected_zones(&tool, &[1, 2]);

        // Shift+Ctrl on final zone to select full second graph
        tool.handle_click(state(&objects), Vec2::new(40.0, 100.0), IM::ctrl_shift()).unwrap();
        assert_selected_zones(&tool, &[1, 2, 3]);

        // Shift+Ctrl+Alt to toggle a zone on the second graph off
        tool.handle_click(state(&objects), Vec2::new(90.0, 00.0), IM::all()).unwrap();
        assert_selected_zones(&tool, &[1, 3]);

        // No modifier keys to select full first zone and deselect the other shape
        tool.handle_click(state(&objects), Vec2::new(0.0, 20.0), IM::none()).unwrap();
        assert_selected_zones(&tool, &[0, 1]);

        // Alt to deselect whole first zone
        tool.handle_click(state(&objects), Vec2::new(0.0, 20.0), IM::alt()).unwrap();
        assert!(tool.selection().is_empty())
    }

    /// Ctrl+clicking a point shared by two zones cycles through them
    #[test]
    fn click_cycles_shared_points() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (zone_id, _) = objects.create_branching_zone(1).unwrap();
        objects.create_point(zone_id, Vec2::new(20.0, 0.0)).unwrap();
        objects.complete_branching_zone(1, zone_id, 2).unwrap();
        let mut tool = EditorToolZone::init();

        tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), IM::ctrl()).unwrap();
        let first = tool.selection().to_vec();
        tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), IM::ctrl()).unwrap();
        let second = tool.selection().to_vec();
        assert_ne!(first, second);
        tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), IM::ctrl()).unwrap();
        assert_eq!(tool.selection(), &first[..]);

        // Check clicks on the shared path as well
        tool.handle_click(state(&objects), Vec2::new(10.0, 5.0), IM::ctrl()).unwrap();
        assert_eq!(tool.selection(), &second[..]);
        tool.handle_click(state(&objects), Vec2::new(10.0, 5.0), IM::ctrl()).unwrap();
        assert_eq!(tool.selection(), &first[..]);

        // Check a ctrl click on a non-shared point and path
        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), IM::ctrl()).unwrap();
        assert_eq!(tool.selection(), &first[..]);
        tool.handle_click(state(&objects), Vec2::new(5.0, 0.0), IM::ctrl()).unwrap();
        assert_eq!(tool.selection(), &first[..]);
    }

    /// Dragging a zone moves it by where it was grabbed, and marks the normals stale
    #[test]
    fn drag_moves_zone() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolZone::init();

        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 10.0), Default::default()).unwrap();
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(15.0, 12.0), Default::default()).unwrap();
        assert!(result.normals_stale);
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(5.0, 2.0));
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 12.0));

        // Releasing keeps the zone selected but stops the drag
        tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(tool.selection().len(), 4);
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(100.0, 100.0), Default::default()).unwrap();
        assert!(!result.normals_stale);
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 12.0));
    }

    /// Dragging with nothing selected does nothing
    #[test]
    fn drag_nothing() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolZone::init();

        tool.handle_drag_start(state(&objects), Vec2::new(50.0, 50.0), Default::default()).unwrap();
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(60.0, 60.0), Default::default()).unwrap();
        assert!(!result.normals_stale);
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::ZERO);
    }

    /// Dragging on an unselected zone, with another selected, drags the newly selected one instead
    #[test]
    fn drag_with_selected() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(40.0, 0.0), (140.0, 0.0), (140.0, 100.0)]);
        let mut tool = EditorToolZone::init();

        // Select the first zone
        tool.handle_click(state(&objects), Vec2::new(10.0, 10.0), Default::default()).unwrap();

        // Drag the second zone
        tool.handle_drag_start(state(&objects), Vec2::new(140.0, 100.0), Default::default()).unwrap();
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(145.0, 102.0), Default::default()).unwrap();
        assert!(result.normals_stale);
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(0.0, 0.0));
        assert_eq!(objects.get_point_info(4).unwrap().position(), Vec2::new(45.0, 2.0));
        assert_eq!(objects.get_point_info(6).unwrap().position(), Vec2::new(145.0, 102.0));

        // Releasing keeps the zone selected but stops the drag
        tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(tool.selection().len(), 3);
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(100.0, 100.0), Default::default()).unwrap();
        assert!(!result.normals_stale);
        assert_eq!(objects.get_point_info(6).unwrap().position(), Vec2::new(145.0, 102.0));
    }

    /// Dragging a selected zone doesn't change the selection regardless of modifier keys
    #[test]
    fn drag_selected_doesnt_change_selection() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(40.0, 0.0), (140.0, 0.0), (140.0, 100.0)]);
        let mut tool = EditorToolZone::init();

        // Select both zones
        tool.handle_click(state(&objects), Vec2::new(10.0, 10.0), Default::default()).unwrap();
        tool.handle_click(state(&objects), Vec2::new(90.0, 1.0), IM::shift()).unwrap();

        // Drag the second zone, without holding shift
        tool.handle_drag_start(state(&objects), Vec2::new(140.0, 100.0), Default::default()).unwrap();
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(145.0, 102.0), Default::default()).unwrap();
        assert!(result.normals_stale);
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(5.0, 2.0));
        assert_eq!(objects.get_point_info(4).unwrap().position(), Vec2::new(45.0, 2.0));
        assert_eq!(objects.get_point_info(6).unwrap().position(), Vec2::new(145.0, 102.0));

        // Releasing keeps the zone selected but stops the drag
        tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(tool.selection().len(), 7);
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(100.0, 100.0), Default::default()).unwrap();
        assert!(!result.normals_stale);
        assert_eq!(objects.get_point_info(6).unwrap().position(), Vec2::new(145.0, 102.0));
    }

    /// Dragging the selected zones doesn't apply to each sibling individually, as that would move each shared point multiple times.
    #[test]
    fn drag_moves_siblings_once() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (branch_zone, _) = objects.create_branching_zone(2).unwrap();
        objects.create_point(branch_zone, Vec2::new(10.0, 20.0)).unwrap();
        objects.create_point(branch_zone, Vec2::new(0.0, 20.0)).unwrap();
        objects.complete_branching_zone(2, branch_zone, 3).unwrap();
        let mut tool = EditorToolZone::init();

        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 10.0), Default::default()).unwrap();
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(15.0, 12.0), Default::default()).unwrap();
        assert!(result.normals_stale);

        // These points are two sets of siblings. Moving one propagates to the other, so the whole group should only be moved once.
        assert_eq!(objects.get_point_info(3).unwrap().position(), Vec2::new(5.0, 12.0));
        assert_eq!(objects.get_point_info(4).unwrap().position(), Vec2::new(5.0, 12.0));
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 12.0));
        assert_eq!(objects.get_point_info(7).unwrap().position(), Vec2::new(15.0, 12.0));

        // Releasing keeps the zone selected but stops the drag
        tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(tool.selection().len(), 8);
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(100.0, 100.0), Default::default()).unwrap();
        assert!(!result.normals_stale);
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 12.0));
    }

    /// Test selecting zones by dragging over selected points
    #[test]
    fn drag_to_select() {
        let mut objects = ObjectBuffers::headless();
        
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (20.0, 10.0)]);
        add_zone(&mut objects, &[(40.0, 10.0), (50.0, 10.0), (50.0, 0.0)]);

        let (branching_zone, _) = objects.create_branching_zone(9).unwrap();
        objects.create_point(branching_zone, Vec2::new(55.0, 5.0)).unwrap();
        objects.complete_branching_zone(9, branching_zone, 8).unwrap();

        let mut tool = EditorToolZone::init();

        let check_selection = |actual: &[u16], expected: &[u16]| 
        assert_eq!(
            actual.into_iter().map(|&i| i).collect::<std::collections::HashSet<u16>>(),
            expected.into_iter().map(|&i| i).collect::<std::collections::HashSet<u16>>(),
        );

        // Selection box should be empty when not dragging
        assert_eq!(tool.selection_box(), None);

        // Start dragging from above and between zones 2 and 3, downward and to the right over zone 3
        tool.handle_drag_start(state(&objects), Vec2::new(35.0, -7.0), Default::default()).unwrap();
        assert_eq!(tool.selection_box(), Some((Vec2::new(35.0, -7.0), Vec2::new(35.0, -7.0))));
        assert_eq!(tool.selection(), &[]);

        let result = tool.handle_dragging_to(state(&objects), Vec2::new(45.0, 17.0), Default::default()).unwrap();
        assert!(!result.normals_stale);
        check_selection(tool.selection(), &[7, 8, 9, 10, 11, 12]); // should include the connected zone 4 even though no points overlap
        assert_eq!(tool.selection_box(), Some((Vec2::new(35.0, -7.0), Vec2::new(45.0, 17.0))));

        // Drag far to the left, leaving zone 3 and selecting 1 and 2
        tool.handle_dragging_to(state(&objects), Vec2::new(5.0, 5.0), Default::default()).unwrap();
        check_selection(tool.selection(), &[0, 1, 2, 3, 4, 5, 6]);
        assert_eq!(tool.selection_box(), Some((Vec2::new(35.0, -7.0), Vec2::new(5.0, 5.0))));

        // Releasing keeps the zones selected but stops the drag
        tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        check_selection(tool.selection(), &[0, 1, 2, 3, 4, 5, 6]);
        assert_eq!(tool.selection_box(), None);
        tool.handle_dragging_to(state(&objects), Vec2::new(100.0, 100.0), Default::default()).unwrap();
        check_selection(tool.selection(), &[0, 1, 2, 3, 4, 5, 6]);
        assert_eq!(tool.selection_box(), None);

        // Ensure positions are all still correct
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(0.0, 0.0));
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(10.0, 10.0));
        assert_eq!(objects.get_point_info(4).unwrap().position(), Vec2::new(20.0, 0.0));
        assert_eq!(objects.get_point_info(7).unwrap().position(), Vec2::new(40.0, 10.0));
    }

    /// Cancel input should deselect everything
    #[test]
    fn cancel_deselects() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(40.0, 0.0), (50.0, 0.0), (50.0, 10.0)]);
        let mut tool = EditorToolZone::init();

        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
        tool.handle_click(state(&objects), Vec2::new(40.0, 0.0), IM::shift()).unwrap();
        assert_eq!(tool.selection().len(), 7);

        tool.handle_cancel(state(&objects)).unwrap();
        assert!(tool.selection().is_empty());
    }

    /// Deleting removes selected zones, and clears the selection
    #[test]
    fn delete_zones() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(40.0, 0.0), (50.0, 0.0), (50.0, 10.0)]);
        let mut tool = EditorToolZone::init();

        assert!(!tool.handle_delete(state(&objects)).unwrap().normals_stale);
        assert_eq!(objects.zone_count(), 2);

        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
        assert!(tool.handle_delete(state(&objects)).unwrap().normals_stale);
        assert_eq!(objects.object_counts(), (1, 3));
        assert!(tool.selection().is_empty());
    }

    /// Deselecting gives back the selected points
    #[test]
    fn deselect_returns_selection() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolZone::select(&[0], state(&objects));
        assert_eq!(tool.deselect(state(&objects)), &[0, 1, 2, 3]);
    }
}
