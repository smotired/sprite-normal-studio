use studio_math::Vec2;
use studio_math::polygon::triangulate;

use crate::InputModifiers;

use super::{EditorTool, EditorToolKind, ControllerStateInput, SelectionType, SpecialAction, SpecialQuery, SpecialValue, utils};
use super::result::{ToolResult, DefaultResults, EditorToolActionResult};

/// The Shape pen allows drawing a path of shape vertices in a zone, like the pen but without handles.
/// The path is drawn with lines that don't affect normals until it is finished, when it becomes one group of shapes with the default params:
///
/// Clicking the first vertex again closes the path. A single vertex becomes a point, two become a line,
/// and more become triangles that fill the path, as one flat surface.
/// Clicking the latest vertex again finishes the path without closing it, as a chain of lines.
///
/// The first click must be inside a zone, which the shapes go in. Clicking an existing vertex in that zone uses it instead of adding one,
/// so the new shapes share it with the shapes already using it.
/// Dragging after adding a vertex moves it.
///
/// Modifier keys:
/// SHIFT: snap new vertices to the grid
#[derive(Default, Clone)]
pub struct EditorToolShapePen {
    /// The zone shapes are being drawn in, or were last drawn in
    zone: Option<u16>,

    /// The vertices of the path being drawn, in order
    path: Vec<u16>,

    /// Position of the latest vertex of the path, which a ghost line joins to the cursor
    ghost: Option<Vec2>,

    /// The vertex added by the latest click, which dragging moves. Existing vertices that were reused can't be dragged.
    new_vertex: Option<u16>,

    /// The points of the zone, which are the selection passed between tools
    zone_points: Vec<u16>,

    /// If the shape pen stays active after finishing a path, instead of switching to the shape tool
    stay_active: bool,
}

impl EditorTool for EditorToolShapePen {
    fn init() -> Box<Self> where Self : Sized { Box::default() }

    fn select(selection: SelectionType, state: ControllerStateInput) -> Box<Self> where Self : Sized {
        // Draw in the zone of the first selected point, if a path starts inside it
        let mut tool = Self::default();
        let zone_id = selection.first().and_then(|&point_id| state.objects.get_point_info(point_id).ok()).map(|point| point.zone_id());
        if let Some(zone_id) = zone_id {
            tool.set_zone(zone_id, &state);
        }
        Box::new(tool)
    }

    fn deselect(&mut self, mut state: ControllerStateInput) -> SelectionType<'_> {
        // Abandon the path being drawn
        if let Some(zone_id) = self.zone && !self.path.is_empty() {
            state.objects.cancel_pending_shapes(zone_id).unwrap();
            self.clear_path();
        }
        self.selection()
    }

    fn kind(&self) -> EditorToolKind { EditorToolKind::ShapePen }

    fn selection(&self) -> SelectionType<'_> { &self.zone_points }

    fn handle_click(&mut self, state: ControllerStateInput, pos: Vec2, modifiers: InputModifiers) -> ToolResult {
        self.add_vertex(state, pos, modifiers)
    }

    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: Vec2, modifiers: InputModifiers) -> ToolResult {
        self.add_vertex(state, pos, modifiers)
    }

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: Vec2, modifiers: InputModifiers) -> ToolResult {
        // Move the vertex that was just added. The path doesn't affect normals yet, so nothing is stale.
        if let Some(vertex_id) = self.new_vertex {
            let pos = if modifiers.shift { utils::snap_to_grid(pos) } else { pos };
            state.objects.set_vertex_position(vertex_id, pos)?;
            self.ghost = Some(pos);
        }
        self.ok()
    }

    fn handle_drag_released(&mut self, _state: ControllerStateInput, _modifiers: InputModifiers) -> ToolResult {
        self.new_vertex = None;
        self.ok()
    }

    fn handle_cancel(&mut self, mut state: ControllerStateInput) -> ToolResult {
        if let Some(zone_id) = self.zone && !self.path.is_empty() {
            state.objects.cancel_pending_shapes(zone_id)?;
            self.clear_path();
        }
        self.ok()
    }

    fn handle_delete(&mut self, mut state: ControllerStateInput) -> ToolResult {
        // Take the latest vertex off the path
        if let Some(zone_id) = self.zone && self.path.pop().is_some() {
            self.new_vertex = None;
            if self.path.is_empty() {
                state.objects.cancel_pending_shapes(zone_id)?;
                self.clear_path();
            } else {
                self.update_pending(&mut state, zone_id)?;
            }
        }
        self.ok()
    }

    fn handle_special_action(&mut self, _state: ControllerStateInput, action: SpecialAction) -> ToolResult {
        if action == SpecialAction::ToggleStayActive {
            self.stay_active = !self.stay_active;
        }
        self.ok()
    }

    fn handle_special_query(&self, query: SpecialQuery) -> Option<SpecialValue> {
        match query {
            SpecialQuery::ActiveZone => Some(SpecialValue::Zone(self.zone)),
            SpecialQuery::ShapeGhost => self.ghost.map(SpecialValue::Position),
            SpecialQuery::StayActive => Some(SpecialValue::Flag(self.stay_active)),
            _ => None,
        }
    }
}

impl EditorToolShapePen {
    /// Make a zone the one shapes are drawn in
    fn set_zone(&mut self, zone_id: u16, state: &ControllerStateInput) {
        self.zone = Some(zone_id);
        self.zone_points = state.objects.get_zone_info(zone_id).map(|zone| {
            let (start, count) = zone.range();
            (start..start + count).collect()
        }).unwrap_or_default();
    }

    /// Forget the path being drawn, without changing the objects
    fn clear_path(&mut self) {
        self.path.clear();
        self.ghost = None;
        self.new_vertex = None;
    }

    /// Add a vertex to the path at a position, starting a path if there isn't one, or finish the path if the first or latest vertex was clicked
    fn add_vertex(&mut self, mut state: ControllerStateInput, pos: Vec2, modifiers: InputModifiers) -> ToolResult {
        self.new_vertex = None;

        // Start a path in the zone that was clicked, preferring the zone last drawn in
        let zone_id = match (self.zone, self.path.is_empty()) {
            (Some(zone_id), false) => zone_id,
            _ => {
                let Some(zone_id) = state.objects.zone_at(pos, self.zone)? else { anyhow::bail!("Shapes must be placed inside a zone!"); };
                self.set_zone(zone_id, &state);
                zone_id
            },
        };

        // Use an existing vertex in the zone if one was clicked, or finish the path if it's the first or latest vertex
        let tolerance = utils::CLICK_TOLERANCE * state.camera_inv_scale;
        let clicked = state.objects.get_closest_vertex(Some(zone_id), pos)?
            .map(|(_, vertex_id)| vertex_id)
            .filter(|&vertex_id| state.objects.get_vertex_info(vertex_id).is_ok_and(|vertex| vertex.position().distance(pos) <= tolerance));
        let vertex_id = match clicked {
            Some(vertex_id) if self.path.first() == Some(&vertex_id) => return self.finish(&mut state, zone_id, true),
            Some(vertex_id) if self.path.last() == Some(&vertex_id) => return self.finish(&mut state, zone_id, false),
            Some(vertex_id) if self.path.contains(&vertex_id) => anyhow::bail!("The path already uses that vertex!"),
            Some(vertex_id) => vertex_id,
            None => {
                let pos = if modifiers.shift { utils::snap_to_grid(pos) } else { pos };
                let vertex_id = state.objects.add_shape_vertex(zone_id, pos)?;
                self.new_vertex = Some(vertex_id);
                vertex_id
            },
        };

        self.path.push(vertex_id);
        self.update_pending(&mut state, zone_id)?;
        self.ok()
    }

    /// Replace the pending shapes with ones that draw the path so far: a point for its first vertex, then lines between its vertices
    fn update_pending(&mut self, state: &mut ControllerStateInput, zone_id: u16) -> anyhow::Result<()> {
        let shapes: Vec<Vec<u16>> = match self.path[..] {
            [vertex_id] => vec![vec![vertex_id]],
            _ => self.path.windows(2).map(|pair| pair.to_vec()).collect(),
        };

        // Only vertices the path no longer uses are removed, which were added after every vertex left in it, so its IDs stay the same
        state.objects.replace_pending_shapes(zone_id, &shapes, state.shape_defaults, true)?;
        let latest = *self.path.last().expect("the path has vertices");
        self.ghost = Some(state.objects.get_vertex_info(latest)?.position());
        Ok(())
    }

    /// Turn the path into one group of shapes. A closed path of three or more vertices is filled with triangles,
    /// and an open one becomes a chain of lines.
    fn finish(&mut self, state: &mut ControllerStateInput, zone_id: u16, closed: bool) -> ToolResult {
        let shapes: Vec<Vec<u16>> = match self.path.len() {
            1 | 2 => vec![self.path.clone()],
            _ if !closed => self.path.windows(2).map(|pair| pair.to_vec()).collect(),
            _ => {
                let positions = self.path.iter()
                    .map(|&vertex_id| Ok(state.objects.get_vertex_info(vertex_id)?.position()))
                    .collect::<anyhow::Result<Vec<Vec2>>>()?;
                let Some(triangles) = triangulate(&positions) else { anyhow::bail!("A closed path can't cross itself or have no area!"); };
                triangles.iter().map(|triangle| triangle.iter().map(|&i| self.path[i]).collect()).collect()
            },
        };

        let created = state.objects.replace_pending_shapes(zone_id, &shapes, state.shape_defaults, false)?;
        let bounds = state.objects.shapes_reach_bounds(&created)?;
        self.clear_path();

        let next_tool = if self.stay_active { EditorToolKind::ShapePen } else { EditorToolKind::Shape };
        Ok(EditorToolActionResult::new(next_tool, bounds))
    }
}

#[cfg(test)]
mod tests {
    use crate::{ObjectBuffers, InputModifiers as IM};
    use crate::objects::test_utils::add_square;

    use super::*;
    use super::super::test_utils::state;

    /// A 100 pixel square zone, with no shapes
    fn sample() -> ObjectBuffers {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 100.0);
        objects
    }

    /// Click each position in turn, returning the result of the last click
    fn click_all(tool: &mut EditorToolShapePen, objects: &ObjectBuffers, positions: &[(f32, f32)]) -> ToolResult {
        let mut result = tool.ok();
        for &pos in positions {
            result = tool.handle_click(state(objects), Vec2::from(pos), IM::none());
        }
        result
    }

    /// The vertex counts of each shape, and the shapes still being drawn
    fn shapes(objects: &ObjectBuffers) -> (Vec<usize>, usize) {
        let ids: Vec<u16> = (0..objects.shape_count()).collect();
        let counts = ids.iter().map(|&id| objects.shape_vertex_ids(id).unwrap().len()).collect();
        let pending = ids.iter().filter(|&&id| objects.get_shape_info(id).unwrap().is_pending()).count();
        (counts, pending)
    }

    /// Paths must start inside a zone
    #[test]
    fn start_outside_zone() {
        let objects = sample();
        let mut tool = EditorToolShapePen::init();
        assert!(tool.handle_click(state(&objects), Vec2::new(150.0, 50.0), IM::none()).is_err());
        assert_eq!(tool.handle_special_query(SpecialQuery::ActiveZone), Some(SpecialValue::Zone(None)));
    }

    /// The path is drawn with pending shapes, and a ghost from its latest vertex
    #[test]
    fn drawing_is_pending() {
        let objects = sample();
        let mut tool = EditorToolShapePen::init();
        click_all(&mut tool, &objects, &[(10.0, 10.0)]).unwrap();
        assert_eq!(shapes(&objects), (vec![1], 1));
        assert_eq!(tool.handle_special_query(SpecialQuery::ActiveZone), Some(SpecialValue::Zone(Some(0))));

        click_all(&mut tool, &objects, &[(50.0, 10.0), (50.0, 50.0)]).unwrap();
        assert_eq!(shapes(&objects), (vec![2, 2], 2));
        assert_eq!(tool.handle_special_query(SpecialQuery::ShapeGhost), Some(SpecialValue::Position(Vec2::new(50.0, 50.0))));
        assert_eq!(tool.path, vec![0, 1, 2]);
    }

    /// Clicking the same place twice makes a point, and finishing switches to the shape tool
    #[test]
    fn point() {
        let objects = sample();
        let mut tool = EditorToolShapePen::init();
        let result = click_all(&mut tool, &objects, &[(10.0, 10.0), (11.0, 10.0)]).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Shape);
        assert!(result.normals_stale.is_some());
        assert_eq!(shapes(&objects), (vec![1], 0));
        assert_eq!(tool.handle_special_query(SpecialQuery::ShapeGhost), None);
    }

    /// Two vertices make a line whether the path is closed or not
    #[test]
    fn line() {
        for last in [(10.0, 10.0), (50.0, 10.0)] {
            let objects = sample();
            let mut tool = EditorToolShapePen::init();
            click_all(&mut tool, &objects, &[(10.0, 10.0), (50.0, 10.0), last]).unwrap();
            assert_eq!(shapes(&objects), (vec![2], 0));
        }
    }

    /// Closing a path of four vertices fills it with two triangles in one group, with the default params
    #[test]
    fn closed_path() {
        let objects = sample();
        let mut tool = EditorToolShapePen::init();
        click_all(&mut tool, &objects, &[(10.0, 10.0), (50.0, 10.0), (50.0, 50.0), (10.0, 50.0), (10.0, 10.0)]).unwrap();
        assert_eq!(shapes(&objects), (vec![3, 3], 0));
        assert_eq!(objects.vertex_count(), 4);
        assert_eq!(objects.get_shape_info(0).unwrap().group(), objects.get_shape_info(1).unwrap().group());
        assert_eq!(objects.get_shape_info(0).unwrap().params(), crate::objects::ShapeParams::default());

        // Six vertices make four triangles
        let objects = sample();
        let mut tool = EditorToolShapePen::init();
        click_all(&mut tool, &objects, &[(10.0, 10.0), (90.0, 10.0), (90.0, 50.0), (50.0, 50.0), (50.0, 90.0), (10.0, 90.0), (10.0, 10.0)]).unwrap();
        assert_eq!(shapes(&objects), (vec![3, 3, 3, 3], 0));
    }

    /// Finishing at the latest vertex makes a chain of lines sharing vertices
    #[test]
    fn open_path() {
        let objects = sample();
        let mut tool = EditorToolShapePen::init();
        click_all(&mut tool, &objects, &[(10.0, 10.0), (50.0, 10.0), (50.0, 50.0), (50.0, 50.0)]).unwrap();
        assert_eq!(shapes(&objects), (vec![2, 2], 0));
        assert_eq!(objects.shape_vertex_ids(0).unwrap()[1], objects.shape_vertex_ids(1).unwrap()[0]);
    }

    /// A closed path that crosses itself is an error, and drawing continues
    #[test]
    fn crossing_path() {
        let objects = sample();
        let mut tool = EditorToolShapePen::init();
        click_all(&mut tool, &objects, &[(10.0, 10.0), (50.0, 50.0), (50.0, 10.0), (10.0, 50.0)]).unwrap();
        assert!(tool.handle_click(state(&objects), Vec2::new(10.0, 10.0), IM::none()).is_err());
    }

    /// Clicking an existing vertex in the zone uses it, so new shapes share it
    #[test]
    fn reuses_vertices() {
        let objects = sample();
        let mut tool = EditorToolShapePen::init();
        tool.stay_active = true;
        let result = click_all(&mut tool, &objects, &[(10.0, 10.0), (50.0, 10.0), (50.0, 10.0)]).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::ShapePen);

        click_all(&mut tool, &objects, &[(50.0, 11.0), (50.0, 50.0), (50.0, 50.0)]).unwrap();
        assert_eq!(objects.vertex_count(), 3);
        assert_eq!(objects.shapes_using_vertices(&[1]).unwrap(), vec![0, 1]);
        assert_ne!(objects.get_shape_info(0).unwrap().group(), objects.get_shape_info(1).unwrap().group());

        // A vertex already in the path can't be used again
        click_all(&mut tool, &objects, &[(10.0, 10.0), (50.0, 10.0)]).unwrap();
        assert!(tool.handle_click(state(&objects), Vec2::new(10.0, 10.0), IM::shift()).is_ok()); // closes as a line
        click_all(&mut tool, &objects, &[(10.0, 10.0), (50.0, 10.0), (50.0, 50.0)]).unwrap();
        assert!(tool.handle_click(state(&objects), Vec2::new(50.0, 10.0), IM::none()).is_err());
    }

    /// Dragging after adding a vertex moves it, but not a reused one
    #[test]
    fn drag_new_vertex() {
        let objects = sample();
        let mut tool = EditorToolShapePen::init();
        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 10.0), IM::none()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(20.3, 10.0), IM::shift()).unwrap();
        tool.handle_drag_released(state(&objects), IM::none()).unwrap();
        assert_eq!(objects.get_vertex_info(0).unwrap().position(), Vec2::new(20.0, 10.0));
        assert_eq!(tool.handle_special_query(SpecialQuery::ShapeGhost), Some(SpecialValue::Position(Vec2::new(20.0, 10.0))));
    }

    /// Deleting takes the latest vertex off the path, and cancelling or switching tools abandons it
    #[test]
    fn delete_and_cancel() {
        let objects = sample();
        let mut tool = EditorToolShapePen::init();
        click_all(&mut tool, &objects, &[(10.0, 10.0), (50.0, 10.0), (50.0, 50.0)]).unwrap();
        tool.handle_delete(state(&objects)).unwrap();
        assert_eq!(shapes(&objects), (vec![2], 1));
        assert_eq!(objects.vertex_count(), 2);
        assert_eq!(tool.handle_special_query(SpecialQuery::ShapeGhost), Some(SpecialValue::Position(Vec2::new(50.0, 10.0))));

        tool.handle_cancel(state(&objects)).unwrap();
        assert_eq!((objects.shape_count(), objects.vertex_count()), (0, 0));

        click_all(&mut tool, &objects, &[(10.0, 10.0), (50.0, 10.0)]).unwrap();
        tool.deselect(state(&objects));
        assert_eq!((objects.shape_count(), objects.vertex_count()), (0, 0));
    }
}
