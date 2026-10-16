use studio_math::Vec2;

use crate::{Bounds, InputModifiers};
use crate::objects::{merge_bounds, ShapeParamChange, ShapeParams};

use super::{EditorTool, EditorToolKind, ControllerStateInput, SelectionType, SpecialAction, SpecialQuery, SpecialValue, utils};
use super::result::{ToolResult, DefaultResults};

/// The Shape tool allows selecting and manipulating the shapes in one zone, the active zone, by their vertices.
/// A shape counts as selected when all of its vertices are.
///
/// Clicking a vertex selects it. Clicking a shape selects every vertex in its group, or with CTRL, just the shape's own vertices.
/// Clicking something in another zone makes that zone active instead, selecting what was clicked.
///
/// Selection rules, as in the zone tool:
/// SHIFT: add to selection instead of replacing
/// ALT: remove from selection instead of replacing
///
/// Dragging a vertex or shape moves the selected vertices, selecting what was grabbed first if it wasn't selected.
/// SHIFT: snap movement to horizontal, vertical, or diagonal
/// CTRL: snap to the grid
///
/// While exactly one shape is selected, it has knobs for its inner radius and outer radius, which can be dragged.
/// The outer radius knob takes priority over everything, but vertices take priority over the inner radius knob, which sits on the hull while the inner radius is 0.
/// CTRL: snap the radius to whole pixels
///
/// Dragging anywhere else drags out a selection box of the active zone's vertices, using the selection rules against the selection from before the drag.
#[derive(Default, Clone)]
pub struct EditorToolShape {
    /// The zone whose shapes are being edited
    active_zone: Option<u16>,

    /// The selected shape vertices, all in the active zone
    selected_vertices: Vec<u16>,

    /// The points of the active zone, which are the selection passed between tools
    zone_points: Vec<u16>,

    /// The vertices being selected by an in-progress selection box, which replace the selection once the drag is released
    tentative_selection: Option<Vec<u16>>,

    /// What is being dragged, if anything
    drag: Option<ShapeDrag>,
}

/// Something being dragged in the shape tool
#[derive(Clone)]
enum ShapeDrag {
    /// The selected vertices, from where they were before the drag
    Move {
        /// The mouse position where the drag started
        origin: Vec2,

        /// The position of what was grabbed, which snapping applies to
        start: Vec2,

        /// Each selected vertex and its position before the drag
        originals: Vec<(u16, Vec2)>,
    },

    /// A radius knob of a shape
    Radius {
        shape_id: u16,

        /// True for the outer radius, false for the inner radius
        outer: bool,

        /// The shape's params before the drag
        original: ShapeParams,

        /// Where the knobs start from on the edge of the hull, and the direction they go out from it
        anchor: Vec2,
        direction: Vec2,
    },

    /// A selection box
    Box {
        origin: Vec2,
        end: Vec2,

        /// The selection from before the drag, which the selection rules apply to
        before: Vec<u16>,
    },
}

/// What is at a position in the shape tool, in order of priority
#[derive(Copy, Clone, PartialEq, Debug)]
enum Hit {
    /// A radius knob of the only selected shape. True for the outer radius.
    Radius(u16, bool),

    /// A shape vertex, and its zone
    Vertex(u16, u16),

    /// The core of a shape, and its zone
    Shape(u16, u16),

    /// Somewhere inside a zone, but not on any of its shapes
    Zone(u16),

    /// Nothing
    Nothing,
}

impl EditorTool for EditorToolShape {
    fn init() -> Box<Self> where Self : Sized { Box::default() }

    fn select(selection: SelectionType, state: ControllerStateInput) -> Box<Self> where Self : Sized {
        // The active zone is the zone of the first selected point
        let mut tool = Self::default();
        let zone_id = selection.first().and_then(|&point_id| state.objects.get_point_info(point_id).ok()).map(|point| point.zone_id());
        if let Some(zone_id) = zone_id {
            tool.set_active_zone(zone_id, &state);
        }
        Box::new(tool)
    }

    fn deselect(&mut self, _state: ControllerStateInput) -> SelectionType<'_> { self.selection() }

    fn kind(&self) -> EditorToolKind { EditorToolKind::Shape }

    fn selection(&self) -> SelectionType<'_> { &self.zone_points }

    fn shape_selection(&self) -> SelectionType<'_> {
        match &self.tentative_selection {
            Some(vertices) => vertices,
            None => &self.selected_vertices,
        }
    }

    fn selection_box(&self) -> Option<(Vec2, Vec2)> {
        match &self.drag {
            Some(ShapeDrag::Box { origin, end, .. }) => Some((*origin, *end)),
            _ => None,
        }
    }

    fn handle_click(&mut self, state: ControllerStateInput, pos: Vec2, modifiers: InputModifiers) -> ToolResult {
        match self.hit(&state, pos)? {
            // Clicking a radius knob does nothing, rather than changing the selection
            Hit::Radius(..) => {},
            Hit::Vertex(zone_id, vertex_id) => self.select_unit(zone_id, &[vertex_id], modifiers, &state),
            Hit::Shape(zone_id, shape_id) => {
                let unit = self.shape_unit(shape_id, modifiers, &state)?;
                self.select_unit(zone_id, &unit, modifiers, &state);
            },
            Hit::Zone(zone_id) if Some(zone_id) != self.active_zone => {
                self.set_active_zone(zone_id, &state);
            },
            Hit::Zone(_) | Hit::Nothing => if !modifiers.shift && !modifiers.alt {
                self.selected_vertices.clear();
            },
        }
        self.ok()
    }

    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: Vec2, modifiers: InputModifiers) -> ToolResult {
        self.abandon_drag();
        let hit = self.hit(&state, pos)?;

        // Grab a radius knob
        if let Hit::Radius(shape_id, outer) = hit {
            let (anchor, direction) = state.objects.shape_radius_frame(shape_id)?;
            let original = state.objects.get_shape_info(shape_id)?.params();
            self.drag = Some(ShapeDrag::Radius { shape_id, outer, original, anchor, direction });
            return self.ok();
        }

        // Grab vertices, selecting what was grabbed first if it isn't all selected already
        let grabbed = match hit {
            Hit::Vertex(zone_id, vertex_id) => Some((zone_id, vec![vertex_id], state.objects.get_vertex_info(vertex_id)?.position())),
            Hit::Shape(zone_id, shape_id) => Some((zone_id, self.shape_unit(shape_id, modifiers, &state)?, pos)),
            _ => None,
        };
        if let Some((zone_id, unit, start)) = grabbed {
            let selected = Some(zone_id) == self.active_zone && unit.iter().all(|vertex_id| self.selected_vertices.contains(vertex_id));
            if !selected {
                self.select_unit(zone_id, &unit, modifiers, &state);
            }
            let originals = self.selected_vertices.iter()
                .map(|&vertex_id| Ok((vertex_id, state.objects.get_vertex_info(vertex_id)?.position())))
                .collect::<anyhow::Result<_>>()?;
            self.drag = Some(ShapeDrag::Move { origin: pos, start, originals });
            return self.ok();
        }

        // Otherwise drag out a selection box, in the zone the drag started in if there is one
        if let Hit::Zone(zone_id) = hit && Some(zone_id) != self.active_zone {
            self.set_active_zone(zone_id, &state);
        }
        let before = self.selected_vertices.clone();
        self.tentative_selection = Some(before.clone());
        self.drag = Some(ShapeDrag::Box { origin: pos, end: pos, before });
        self.ok()
    }

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: Vec2, modifiers: InputModifiers) -> ToolResult {
        match self.drag.clone() {
            Some(ShapeDrag::Move { origin, start, originals }) => {
                let target = utils::snap_drag_target(start + (pos - origin), start, origin, pos, modifiers);
                let delta = target - start;
                let vertex_ids: Vec<u16> = originals.iter().map(|&(vertex_id, _)| vertex_id).collect();
                let shape_ids = state.objects.shapes_using_vertices(&vertex_ids)?;

                let before = state.objects.shapes_reach_bounds(&shape_ids)?;
                for &(vertex_id, original) in &originals {
                    state.objects.set_vertex_position(vertex_id, original + delta)?;
                }
                self.stale_in(merge_bounds(before, state.objects.shapes_reach_bounds(&shape_ids)?))
            },

            Some(ShapeDrag::Radius { shape_id, outer, original, anchor, direction }) => {
                let mut distance = (pos - anchor).dot(direction);
                if modifiers.ctrl { distance = distance.round(); }
                let change = if outer {
                    ShapeParamChange::OuterRadius(distance - original.inner_radius)
                } else {
                    ShapeParamChange::InnerRadius(distance)
                };
                self.change_params(&mut state, &[shape_id], &[change])
            },

            Some(ShapeDrag::Box { origin, before, .. }) => {
                let boxed = match self.active_zone {
                    Some(zone_id) => state.objects.vertices_in_rect(zone_id, origin, pos)?,
                    None => vec![],
                };
                self.tentative_selection = Some(utils::combine_selection(&before, &boxed, modifiers, false));
                self.drag = Some(ShapeDrag::Box { origin, end: pos, before });
                self.ok()
            },

            None => self.ok(),
        }
    }

    fn handle_drag_released(&mut self, _state: ControllerStateInput, _modifiers: InputModifiers) -> ToolResult {
        if let Some(vertices) = self.tentative_selection.take() {
            self.selected_vertices = vertices;
        }
        self.drag = None;
        self.ok()
    }

    fn handle_cancel(&mut self, mut state: ControllerStateInput) -> ToolResult {
        match self.drag.take() {
            // Put dragged vertices back where they started
            Some(ShapeDrag::Move { originals, .. }) => {
                let vertex_ids: Vec<u16> = originals.iter().map(|&(vertex_id, _)| vertex_id).collect();
                let shape_ids = state.objects.shapes_using_vertices(&vertex_ids)?;
                let before = state.objects.shapes_reach_bounds(&shape_ids)?;
                for (vertex_id, original) in originals {
                    state.objects.set_vertex_position(vertex_id, original)?;
                }
                self.stale_in(merge_bounds(before, state.objects.shapes_reach_bounds(&shape_ids)?))
            },

            // Put dragged radii back how they started
            Some(ShapeDrag::Radius { shape_id, original, .. }) => {
                let changes = [ShapeParamChange::InnerRadius(original.inner_radius), ShapeParamChange::OuterRadius(original.outer_radius)];
                self.change_params(&mut state, &[shape_id], &changes)
            },

            // Abandon a selection box, keeping the previous selection
            Some(ShapeDrag::Box { .. }) => {
                self.tentative_selection = None;
                self.ok()
            },

            // Otherwise clear the selection
            None => {
                self.selected_vertices.clear();
                self.ok()
            },
        }
    }

    fn handle_delete(&mut self, mut state: ControllerStateInput) -> ToolResult {
        self.abandon_drag();
        if self.selected_vertices.is_empty() { return self.ok(); }

        // Delete whole selected shapes, which keeps the vertices they share with other shapes.
        // If no whole shapes are selected, delete the vertices, and every shape made from them.
        let whole = state.objects.shapes_with_all_vertices(&self.selected_vertices)?;
        let bounds = if !whole.is_empty() {
            let bounds = state.objects.shapes_reach_bounds(&whole)?;
            state.objects.delete_shapes(&whole)?;
            bounds
        } else {
            let bounds = state.objects.shapes_reach_bounds(&state.objects.shapes_using_vertices(&self.selected_vertices)?)?;
            state.objects.delete_shape_vertices(&self.selected_vertices)?;
            bounds
        };
        self.selected_vertices.clear();
        self.stale_in(bounds)
    }

    fn handle_select_all(&mut self, state: ControllerStateInput) -> ToolResult {
        self.abandon_drag();
        if let Some(zone_id) = self.active_zone {
            self.selected_vertices = state.objects.zone_vertex_ids(zone_id)?;
        }
        self.ok()
    }

    fn handle_special_action(&mut self, mut state: ControllerStateInput, action: SpecialAction) -> ToolResult {
        match action {
            SpecialAction::ShapeParamSet(change, _) => {
                let shape_ids = state.objects.shapes_with_all_vertices(&self.selected_vertices)?;
                self.change_params(&mut state, &shape_ids, &[change])
            },
            SpecialAction::ShapeVerticesMerged => {
                self.abandon_drag();
                let before = state.objects.shapes_reach_bounds(&state.objects.shapes_using_vertices(&self.selected_vertices)?)?;
                let merged = state.objects.merge_shape_vertices(&self.selected_vertices)?;
                self.selected_vertices = vec![merged];
                let after = state.objects.shapes_reach_bounds(&state.objects.shapes_using_vertices(&[merged])?)?;
                self.stale_in(merge_bounds(before, after))
            },
            _ => self.ok(),
        }
    }

    fn handle_special_query(&self, query: SpecialQuery) -> Option<SpecialValue> {
        match query {
            SpecialQuery::ActiveZone => Some(SpecialValue::Zone(self.active_zone)),
            _ => None,
        }
    }
}

impl EditorToolShape {
    /// Stop any drag, keeping the committed selection and leaving whatever was dragged where it is
    fn abandon_drag(&mut self) {
        self.tentative_selection = None;
        self.drag = None;
    }

    /// Make a zone the active zone, clearing the selection
    fn set_active_zone(&mut self, zone_id: u16, state: &ControllerStateInput) {
        self.active_zone = Some(zone_id);
        self.selected_vertices.clear();
        self.zone_points = state.objects.get_zone_info(zone_id).map(|zone| {
            let (start, count) = zone.range();
            (start..start + count).collect()
        }).unwrap_or_default();
    }

    /// Select some vertices in a zone using the selection rules, or select just them if the zone isn't active, making it active
    fn select_unit(&mut self, zone_id: u16, unit: &[u16], modifiers: InputModifiers, state: &ControllerStateInput) {
        if Some(zone_id) != self.active_zone {
            self.set_active_zone(zone_id, state);
            self.selected_vertices = unit.to_vec();
        } else {
            self.selected_vertices = utils::combine_selection(&self.selected_vertices, unit, modifiers, true);
        }
    }

    /// Get the vertices clicking a shape selects: its whole group, or with CTRL just the shape
    fn shape_unit(&self, shape_id: u16, modifiers: InputModifiers, state: &ControllerStateInput) -> anyhow::Result<Vec<u16>> {
        let shape_ids = if modifiers.ctrl { vec![shape_id] } else { state.objects.shape_groups(&[shape_id])? };
        let mut vertex_ids = vec![];
        for shape_id in shape_ids {
            vertex_ids.extend(state.objects.shape_vertex_ids(shape_id)?);
        }
        vertex_ids.sort();
        vertex_ids.dedup();
        Ok(vertex_ids)
    }

    /// Change params of shapes and the rest of their groups, marking normals stale where they affected before and after
    fn change_params(&self, state: &mut ControllerStateInput, shape_ids: &[u16], changes: &[ShapeParamChange]) -> ToolResult {
        let grouped = state.objects.shape_groups(shape_ids)?;
        let before: Option<Bounds> = state.objects.shapes_reach_bounds(&grouped)?;
        for &change in changes {
            state.objects.set_shape_params(&grouped, change)?;
        }
        self.stale_in(merge_bounds(before, state.objects.shapes_reach_bounds(&grouped)?))
    }

    /// Find what is at a position. See Hit for the priority.
    fn hit(&self, state: &ControllerStateInput, pos: Vec2) -> anyhow::Result<Hit> {
        let tolerance = utils::CLICK_TOLERANCE * state.camera_inv_scale;

        // Radius knobs of the only selected shape
        let knobs = match state.objects.shapes_with_all_vertices(&self.selected_vertices)?[..] {
            [shape_id] => Some((shape_id, state.objects.shape_radius_handles(shape_id)?)),
            _ => None,
        };
        if let Some((shape_id, (_, outer))) = knobs && outer.distance(pos) <= tolerance {
            return Ok(Hit::Radius(shape_id, true));
        }

        // Vertices, preferring the active zone's
        let searches = if self.active_zone.is_some() { vec![self.active_zone, None] } else { vec![None] };
        for zone_id in searches {
            if let Some((zone_id, vertex_id)) = state.objects.get_closest_vertex(zone_id, pos)?
                && state.objects.get_vertex_info(vertex_id)?.position().distance(pos) <= tolerance {
                return Ok(Hit::Vertex(zone_id, vertex_id));
            }
        }

        if let Some((shape_id, (inner, _))) = knobs && inner.distance(pos) <= tolerance {
            return Ok(Hit::Radius(shape_id, false));
        }

        // Shapes, preferring the active zone's
        let mut shape = None;
        if let Some(zone_id) = self.active_zone {
            shape = state.objects.get_shape_at(Some(zone_id), pos, tolerance)?;
        }
        if shape.is_none() {
            shape = state.objects.get_shape_at(None, pos, tolerance)?;
        }
        if let Some(shape_id) = shape {
            return Ok(Hit::Shape(state.objects.get_shape_info(shape_id)?.zone_id(), shape_id));
        }

        Ok(match state.objects.zone_at(pos, self.active_zone)? {
            Some(zone_id) => Hit::Zone(zone_id),
            None => Hit::Nothing,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::{ObjectBuffers, InputModifiers as IM};
    use crate::objects::ShapeParams;
    use crate::objects::test_utils::add_square;

    use super::*;
    use super::super::test_utils::state;

    /// Two 100 pixel squares side by side, with a point and a line in the first and a point in the second
    fn sample() -> ObjectBuffers {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 100.0);
        let second = objects.create_zone().unwrap();
        for pos in [(200.0, 0.0), (300.0, 0.0), (300.0, 100.0), (200.0, 100.0)] {
            objects.create_point(second, Vec2::from(pos)).unwrap();
        }
        let add = |objects: &mut ObjectBuffers, zone_id: u16, positions: &[(f32, f32)]| {
            let vertex_ids: Vec<u16> = positions.iter().map(|&pos| objects.add_shape_vertex(zone_id, Vec2::from(pos)).unwrap()).collect();
            objects.create_shape(zone_id, &vertex_ids, ShapeParams::default(), 0).unwrap()
        };
        add(&mut objects, 0, &[(20.0, 20.0)]);
        add(&mut objects, 0, &[(50.0, 50.0), (80.0, 50.0)]);
        add(&mut objects, 1, &[(250.0, 50.0)]);
        objects
    }

    /// The tool, with zone 0 active
    fn tool(objects: &ObjectBuffers) -> Box<EditorToolShape> {
        EditorToolShape::select(&[0], state(objects))
    }

    /// Selecting from points makes their zone active, and the zone's points are what is passed on
    #[test]
    fn select_from_points() {
        let objects = sample();
        let tool = EditorToolShape::select(&[5], state(&objects));
        assert_eq!(tool.active_zone, Some(1));
        assert_eq!(tool.selection(), &[4, 5, 6, 7]);
        assert_eq!(tool.handle_special_query(SpecialQuery::ActiveZone), Some(SpecialValue::Zone(Some(1))));
        assert_eq!(EditorToolShape::init().handle_special_query(SpecialQuery::ActiveZone), Some(SpecialValue::Zone(None)));
    }

    /// Clicking selects vertices, shapes, and with modifiers adds or removes them
    #[test]
    fn click_selects() {
        let objects = sample();
        let mut tool = tool(&objects);
        tool.handle_click(state(&objects), Vec2::new(21.0, 20.0), IM::none()).unwrap();
        assert_eq!(tool.shape_selection(), &[0]);

        tool.handle_click(state(&objects), Vec2::new(65.0, 52.0), IM::shift()).unwrap();
        assert_eq!(tool.shape_selection(), &[0, 1, 2]);

        tool.handle_click(state(&objects), Vec2::new(80.0, 50.0), IM::alt()).unwrap();
        assert_eq!(tool.shape_selection(), &[0, 1]);

        // Clicking empty space in the zone deselects
        tool.handle_click(state(&objects), Vec2::new(50.0, 90.0), IM::none()).unwrap();
        assert!(tool.shape_selection().is_empty());
        assert_eq!(tool.active_zone, Some(0));
    }

    /// Clicking in another zone makes it active, selecting what was clicked
    #[test]
    fn click_other_zone() {
        let objects = sample();
        let mut tool = tool(&objects);
        tool.handle_click(state(&objects), Vec2::new(20.0, 20.0), IM::none()).unwrap();
        tool.handle_click(state(&objects), Vec2::new(250.0, 51.0), IM::shift()).unwrap();
        assert_eq!(tool.active_zone, Some(1));
        assert_eq!(tool.shape_selection(), &[3]);

        tool.handle_click(state(&objects), Vec2::new(20.0, 80.0), IM::none()).unwrap();
        assert_eq!(tool.active_zone, Some(0));
        assert!(tool.shape_selection().is_empty());
    }

    /// Clicking a shape selects its whole group, or with ctrl just the shape
    #[test]
    fn click_selects_group() {
        let mut objects = sample();
        let vertex = objects.add_shape_vertex(0, Vec2::new(80.0, 80.0)).unwrap();
        let group = objects.get_shape_info(1).unwrap().group();
        objects.create_shape_in_group(0, &[2, vertex], group, 0).unwrap();

        let mut tool = tool(&objects);
        tool.handle_click(state(&objects), Vec2::new(65.0, 50.0), IM::none()).unwrap();
        assert_eq!(tool.shape_selection(), &[1, 2, 3]);
        tool.handle_click(state(&objects), Vec2::new(65.0, 50.0), IM::ctrl()).unwrap();
        assert_eq!(tool.shape_selection(), &[1, 2]);
    }

    /// Dragging moves the selected vertices with snapping, and cancelling puts them back
    #[test]
    fn drag_moves_selection() {
        let objects = sample();
        let mut tool = tool(&objects);
        tool.handle_click(state(&objects), Vec2::new(65.0, 50.0), IM::none()).unwrap();

        // Away from the middle of the line, where its inner radius knob is
        tool.handle_drag_start(state(&objects), Vec2::new(55.0, 50.0), IM::none()).unwrap();
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(65.0, 53.0), IM::shift()).unwrap();
        assert!(result.normals_stale.is_some());
        assert_eq!(objects.shape_positions(1).unwrap(), vec![Vec2::new(60.0, 50.0), Vec2::new(90.0, 50.0)]);
        assert_eq!(objects.shape_positions(0).unwrap(), vec![Vec2::new(20.0, 20.0)]);

        tool.handle_cancel(state(&objects)).unwrap();
        assert_eq!(objects.shape_positions(1).unwrap(), vec![Vec2::new(50.0, 50.0), Vec2::new(80.0, 50.0)]);
        assert_eq!(tool.shape_selection(), &[1, 2]);
    }

    /// Dragging an unselected vertex selects it first
    #[test]
    fn drag_unselected() {
        let objects = sample();
        let mut tool = tool(&objects);
        tool.handle_click(state(&objects), Vec2::new(65.0, 50.0), IM::none()).unwrap();
        tool.handle_drag_start(state(&objects), Vec2::new(20.0, 20.0), IM::none()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(25.0, 20.0), IM::none()).unwrap();
        tool.handle_drag_released(state(&objects), IM::none()).unwrap();
        assert_eq!(tool.shape_selection(), &[0]);
        assert_eq!(objects.shape_positions(0).unwrap(), vec![Vec2::new(25.0, 20.0)]);
        assert_eq!(objects.shape_positions(1).unwrap()[0], Vec2::new(50.0, 50.0));
    }

    /// Dragging a radius knob of the only selected shape changes its radius, with ctrl snapping to whole pixels
    #[test]
    fn drag_radius() {
        let objects = sample();
        let mut tool = tool(&objects);
        tool.handle_click(state(&objects), Vec2::new(20.0, 20.0), IM::none()).unwrap();

        // The outer knob of the point is 10 pixels to its right
        tool.handle_drag_start(state(&objects), Vec2::new(30.0, 20.0), IM::none()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(35.4, 26.0), IM::ctrl()).unwrap();
        assert_eq!(objects.get_shape_info(0).unwrap().params().outer_radius, 15.0);
        tool.handle_cancel(state(&objects)).unwrap();
        assert_eq!(objects.get_shape_info(0).unwrap().params().outer_radius, 10.0);

        // The inner knob sits on the vertex while the inner radius is 0, so dragging there moves the vertex instead
        tool.handle_drag_start(state(&objects), Vec2::new(20.0, 20.0), IM::none()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(22.0, 20.0), IM::none()).unwrap();
        assert_eq!(objects.shape_positions(0).unwrap(), vec![Vec2::new(22.0, 20.0)]);
        tool.handle_drag_released(state(&objects), IM::none()).unwrap();

        // On a line, the inner knob is on the middle of the line
        tool.handle_click(state(&objects), Vec2::new(65.0, 52.0), IM::none()).unwrap();
        tool.handle_drag_start(state(&objects), Vec2::new(65.0, 50.0), IM::none()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(65.0, 54.0), IM::none()).unwrap();
        assert_eq!(objects.get_shape_info(1).unwrap().params().inner_radius, 4.0);
        assert_eq!(objects.get_shape_info(1).unwrap().params().outer_radius, 10.0);
    }

    /// Dragging a box selects the active zone's vertices inside it, using the selection rules
    #[test]
    fn drag_box() {
        let objects = sample();
        let mut tool = tool(&objects);
        tool.handle_click(state(&objects), Vec2::new(20.0, 20.0), IM::none()).unwrap();
        tool.handle_drag_start(state(&objects), Vec2::new(40.0, 40.0), IM::shift()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(70.0, 60.0), IM::shift()).unwrap();
        assert_eq!(tool.shape_selection(), &[0, 1]);
        assert_eq!(tool.selection_box(), Some((Vec2::new(40.0, 40.0), Vec2::new(70.0, 60.0))));

        tool.handle_drag_released(state(&objects), IM::shift()).unwrap();
        assert_eq!(tool.shape_selection(), &[0, 1]);
        assert_eq!(tool.selection_box(), None);
    }

    /// Deleting removes whole selected shapes, or the selected vertices and the shapes made from them
    #[test]
    fn delete() {
        let mut objects = sample();
        objects.create_shape(0, &[0, 1], ShapeParams::default(), 0).unwrap();
        let mut tool = tool(&objects);

        // The line is selected whole, so it goes but the new shape sharing its first vertex stays
        tool.handle_click(state(&objects), Vec2::new(65.0, 50.0), IM::ctrl()).unwrap();
        tool.handle_delete(state(&objects)).unwrap();
        assert_eq!(objects.shape_count(), 3);
        assert_eq!(objects.vertex_count(), 3);
        assert!(tool.shape_selection().is_empty());

        // A lone vertex takes every shape made from it
        tool.handle_click(state(&objects), Vec2::new(20.0, 20.0), IM::none()).unwrap();
        tool.handle_click(state(&objects), Vec2::new(50.0, 50.0), IM::none()).unwrap();
        tool.handle_delete(state(&objects)).unwrap();
        assert_eq!((objects.shape_count(), objects.vertex_count()), (2, 2));
    }

    /// Changing params changes the selected shapes, and merging vertices selects the merged one
    #[test]
    fn special_actions() {
        let objects = sample();
        let mut tool = tool(&objects);
        tool.handle_click(state(&objects), Vec2::new(65.0, 50.0), IM::none()).unwrap();
        tool.handle_special_action(state(&objects), SpecialAction::ShapeParamSet(ShapeParamChange::Steepness(2.0), false)).unwrap();
        assert_eq!(objects.get_shape_info(1).unwrap().params().steepness, 2.0);
        assert_eq!(objects.get_shape_info(0).unwrap().params().steepness, 1.0);

        tool.handle_special_action(state(&objects), SpecialAction::ShapeVerticesMerged).unwrap();
        assert_eq!(tool.shape_selection(), &[1]);
        assert_eq!(objects.shape_positions(1).unwrap(), vec![Vec2::new(65.0, 50.0)]);
    }

    /// Select all selects every vertex in the active zone
    #[test]
    fn select_all() {
        let objects = sample();
        let mut tool = tool(&objects);
        tool.handle_select_all(state(&objects)).unwrap();
        assert_eq!(tool.shape_selection(), &[0, 1, 2]);
    }
}
