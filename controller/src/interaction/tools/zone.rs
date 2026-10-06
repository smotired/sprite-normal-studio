use studio_math::Vec2;

use super::{EditorTool, EditorToolKind, ControllerStateInput, SelectionType, utils};
use super::result::{ToolResult, DefaultResults};

/// The Zone tool allows mananging entire zones at once. 
#[derive(Default)]
pub struct EditorToolZone {
    /// The zone that is selected.
    selected_zones: Vec<u16>,

    /// Reference to selected points, generated when doing selection()
    /// TODO: should maybe regenerate when selection changes
    selected_points_reference: Vec<u16>,

    /// The reference point for if we are dragging a zone, relative to the position of the first point.
    reference_delta: Option<Vec2>,
}

impl EditorTool for EditorToolZone {
    fn init() -> Box<Self> where Self : Sized { Box::new(Default::default()) }

    fn select(selected_points: SelectionType, state: ControllerStateInput) -> Box<Self> where Self : Sized {
        // Get all zones with a point in the selected zone
        let selected_zones: std::collections::HashSet<_> = selected_points.iter().filter_map(|&point_id| {
            state.objects.get_point_info(point_id).map(|info| info.zone_id())
        }).collect();
        let selected_zones: Vec<_> = selected_zones.into_iter().collect();

        let selected_points_reference = points_from_zone_selection(&selected_zones[..], state);

        Box::new(Self { selected_zones, selected_points_reference, reference_delta: None })
    }

    fn deselect(&mut self, _state: ControllerStateInput) -> SelectionType<'_> {
        // Just return selection
        &self.selected_points_reference[..]
    }

    fn kind(&self) -> EditorToolKind { EditorToolKind::Zone }
    fn selection(&self) -> SelectionType<'_> { &self.selected_points_reference[..] }

    fn handle_click(&mut self, state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // Select the zone if a control point or the path was clicked
        // TODO: Move to like "select clicked control point" and "select clicked path" helpers, like in the pen tool.
        // TODO: This should put all connected zones to the selection.
        //       Shift+click or Ctrl+click should just add all connected zones.
        //       Ctrl+click should remove all connected zones if they are already selected.
        //       Alt+click should replace the selection with only the exact zone.
        //       Shift+alt+click adds only the exact zone to the selection.
        //       Ctrl+alt+click removes the exact zone from the selection if it is selected.

        let new_selection =
            if let Some((zone_id, point_id)) = utils::get_clicked_control_point(&state, None, pos) {
                // If the clicked point has a sibling in the selected zone, select that point's sibling's zone instead.
                if let Some(selected_id) = self.selected_zones.first() && let Some(sibling_id) = state.objects.sibling_in_zone(point_id, *selected_id) {
                    let next_sibling_id = state.objects.get_sibling(sibling_id)?;
                    let next_zone_id = state.objects.get_point_info(next_sibling_id).unwrap().zone_id();
                    vec![next_zone_id]
                } else {
                    vec![zone_id]
                }
            } else if let Some((zone_id, point_id, _, _)) = utils::get_clicked_zone_path(&state, pos) {
                // If the start point has a sibling in the selected zone, possibly select that point's sibling's zone instead.
                if let Some(selected_id) = self.selected_zones.first() && let Some(sibling_id) = state.objects.sibling_in_zone(point_id, *selected_id) {
                    let next_sibling_id = state.objects.get_sibling(sibling_id)?;
                    let next_zone_id = state.objects.get_point_info(next_sibling_id).unwrap().zone_id();

                    // Only select the next sibling if the full path is shared
                    let (start, count) = state.objects.get_zone_info(*selected_id).unwrap().range();
                    let path_end_id = start + (sibling_id - start + 1) % count;
                    if state.objects.sibling_in_zone(path_end_id, next_zone_id).is_some() {
                        vec![next_zone_id]
                    }

                    // Otherwise this path is not shared at all
                    else { vec![zone_id] }
                } else {
                    vec![zone_id]
                }
            } else {
                vec![]
            };
        self.set_selection(new_selection, state);
        self.ok()
    }

    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // TODO: If not clicking on a zone, drag out a box and selecting everything within the box.
        // SHIFT: Add to selection
        // CTRL: Box is sized to square
        // ALT: Only the exact zones are added to the selection, not all connected shapes.
        // This should depend on which modifier keys are pressed when drag is *released*, so keep a backup of selection when drag starts.

        // Select the zone if a control point or the path was clicked
        let (new_selection, reference_delta) = 
            if let Some((mut zone_id, mut point_id)) = utils::get_clicked_control_point(&state, None, pos) {
                // If the point has a sibling in the selected zone, keep the zone selected
                if let Some(selected_id) = self.selected_zones.first() && let Some(sibling_id) = state.objects.sibling_in_zone(point_id, *selected_id) {
                    zone_id = *selected_id;
                    point_id = sibling_id;
                }

                let point = state.objects.get_point_info(point_id).unwrap();

                let (start_id, _) = state.objects.get_zone_info(zone_id).unwrap().range();
                let first_point = state.objects.get_point_info(start_id).unwrap();

                (vec![zone_id], Some(point.position() - first_point.position()))
            } else if let Some((mut zone_id, point_id, corrected, _)) = utils::get_clicked_zone_path(&state, pos) {
                // If the point has a sibling in the selected zone, keep the zone selected
                if let Some(selected_id) = self.selected_zones.first() && state.objects.sibling_in_zone(point_id, *selected_id).is_some() {
                    zone_id = *selected_id;
                }

                let (start_id, _) = state.objects.get_zone_info(zone_id).unwrap().range();
                let first_point = state.objects.get_point_info(start_id).unwrap();
                
                (vec![zone_id], Some(corrected - first_point.position()))
            } else {
                (vec![], None)
            };
        self.set_selection(new_selection, state);
        self.reference_delta = reference_delta;
        self.ok()
    }

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2) -> ToolResult {
        // Drag selected zones
        if !self.selected_zones.is_empty() {
            for zone_id in &self.selected_zones[..] {
                if let Some(reference_delta) = self.reference_delta {
                    // Move the first point to the position
                    state.objects.update_zone_position(*zone_id, pos - reference_delta)?;

                    // TODO: Must not move siblings multiple times! Maybe handle this in the objects side, changing the method to update_zones_positions, and have our first_point_pos be tracking the very first position ID
                }
            }
            return self.stale();
        }
        self.ok()
    }

    fn handle_drag_released(&mut self, _state: ControllerStateInput) -> ToolResult {
        // Release drag on selected zones, but keep them selected
        self.reference_delta = None;
        self.ok()
    }

    fn handle_cancel(&mut self, _state: ControllerStateInput) -> ToolResult { self.ok() }

    fn handle_delete(&mut self, mut state: ControllerStateInput) -> ToolResult {
        // Delete selected zones
        if !self.selected_zones.is_empty() {
            self.selected_zones.sort();
            let mut offset = 0;
            for zone_id in &self.selected_zones[..] {
                state.objects.delete_zone(*zone_id - offset)?;
                offset += 1;
            }

            self.set_selection(Vec::new(), state);
            self.reference_delta = None;

            return self.stale();
        }
        self.ok()
    }
}

impl EditorToolZone {
    fn set_selection(&mut self, selected_zones: Vec<u16>, state: ControllerStateInput) {
        self.selected_zones = selected_zones;
        self.selected_points_reference = points_from_zone_selection(&self.selected_zones[..], state);
    }
}

/// Get the points we have selected from the zone we have selected
fn points_from_zone_selection(selected_zones: &[u16], state: ControllerStateInput) -> Vec<u16> {
    let mut points = vec![];
    for zone_id in selected_zones {
        let (start, count) = state.objects.get_zone_info(*zone_id).unwrap().range();
        points.extend((start..start + count).collect::<Vec<u16>>());
    }
    points
}