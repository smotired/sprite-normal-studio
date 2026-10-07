use studio_math::Vec2;

use crate::InputModifiers;

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
    fn init() -> Box<Self> where Self : Sized { Box::default() }

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

    fn handle_click(&mut self, state: ControllerStateInput, pos: studio_math::Vec2, _modifiers: InputModifiers) -> ToolResult {
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

    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: studio_math::Vec2, _modifiers: InputModifiers) -> ToolResult {
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

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2, _modifiers: InputModifiers) -> ToolResult {
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

    fn handle_drag_released(&mut self, _state: ControllerStateInput, _modifiers: InputModifiers) -> ToolResult {
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
            #[allow(clippy::explicit_counter_loop)] // offset counts deletions, not iterations
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


#[cfg(test)]
mod tests {
    use crate::ObjectBuffers;
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

    /// Clicking a point shared by two zones cycles through them
    #[test]
    fn click_cycles_shared_points() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (zone_id, _) = objects.create_branching_zone(1).unwrap();
        objects.create_point(zone_id, Vec2::new(20.0, 0.0)).unwrap();
        let mut tool = EditorToolZone::init();

        tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), Default::default()).unwrap();
        let first = tool.selection().to_vec();
        tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), Default::default()).unwrap();
        let second = tool.selection().to_vec();
        assert_ne!(first, second);
        tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), Default::default()).unwrap();
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
        assert!(result.normals_stale);
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
