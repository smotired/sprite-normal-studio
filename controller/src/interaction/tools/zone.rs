use studio_math::Vec2;
use studio_math::transform::Affine2;

use crate::{Axis, Bounds, InputModifiers, ObjectsHandle, SavedObjects};
use crate::objects::{merge_bounds, ZonesSnapshot};

use super::{EditorTool, EditorToolKind, ControllerStateInput, PastePlacement, SelectionType, SpecialAction, TransformBox, utils};
use super::result::{ToolResult, DefaultResults};

/// The Zone tool allows mananging entire zones at once.
///
/// Selection rules:
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
///
/// Dragging zones:
/// SHIFT: snap movement to horizontal, vertical, or diagonal, whichever is closest to the direction moved
/// CTRL: snap to the grid (see the grid size setting)
///
/// Dragging a selection box:
/// Uses the same selection rules as above, applied against the selection from before the drag started.
/// The result is tentative until the drag is released, and can be cancelled.
///
/// Transforming selected zones:
/// The selected zones have a box around their control points, with a handle at each corner and edge midpoint.
/// Dragging a handle scales the zones, keeping the opposite side or corner in place.
/// Dragging just outside a corner rotates the zones around the center of the box.
/// Edges too short on screen for a midpoint handle to fit between the corners don't have one.
/// These take priority over dragging zones or a selection box. Clicking a handle does nothing, but clicking outside a corner selects as usual.
/// The box isn't shown or used while dragging out a selection box.
///
/// While scaling:
/// SHIFT: scale uniformly, to the largest size that fits where the handle was dragged
/// ALT: keep the center in place, moving the opposite side or corner instead
/// CTRL: snap the dragged handle to the grid
///
/// While rotating:
/// CTRL: snap to increments of utils::ROTATE_SNAP
#[derive(Default, Clone)]
pub struct EditorToolZone {
    /// The zone that is selected.
    selected_zones: Vec<u16>,

    /// Reference to selected points, generated when doing selection()
    selected_points_reference: Vec<u16>,

    /// The zones and their points being selected by an in-progress selection box.
    /// Replaces the selection once the drag is released.
    tentative_selection: Option<(Vec<u16>, Vec<u16>)>,

    /// The reference point for if we are dragging a zone, relative to the position of the first point.
    reference_delta: Option<Vec2>,

    /// The mouse position where we started dragging a zone.
    drag_origin: Option<Vec2>,

    /// The selected zones from before we started dragging them, which every step of the drag moves from
    drag_snapshot: Option<ZonesSnapshot>,

    /// The start point for our selection box if we are instead dragging out a box of zones
    selection_origin: Option<Vec2>,

    /// The end point for our selection box if we are instead dragging out a box of zones
    selection_box_end: Option<Vec2>,

    /// The transform in progress if we are instead dragging the transform box
    transform: Option<TransformDrag>,
}

/// The part of the transform box that was grabbed
#[derive(Copy, Clone, PartialEq, Debug)]
enum TransformHandle {
    /// Scale from a handle. Each axis is -1 for the min side, 1 for the max side, or 0 for the middle.
    Scale(i8, i8),

    /// Rotate around the center from outside a corner
    Rotate,
}

/// A drag on the transform box
#[derive(Clone)]
struct TransformDrag {
    /// What was grabbed
    handle: TransformHandle,

    /// The transform box when the drag started
    bounds: Bounds,

    /// The mouse position where the drag started
    origin: Vec2,

    /// The current rotation in radians, if rotating
    angle: f32,

    /// The zones being transformed, from before the drag
    snapshot: ZonesSnapshot,
}

impl TransformDrag {
    /// Get the transform for the mouse being dragged to a position, from where the points were before the drag
    fn update(&mut self, pos: Vec2, modifiers: InputModifiers, grid: f32) -> Affine2 {
        let (min, max) = self.bounds;
        let center = (min + max) * 0.5;
        let half = (max - min) * 0.5;

        match self.handle {
            TransformHandle::Rotate => {
                let angle_of = |pos: Vec2| (pos.y - center.y).atan2(pos.x - center.x);
                let mut angle = angle_of(pos) - angle_of(self.origin);
                if modifiers.ctrl { angle = (angle / utils::ROTATE_SNAP).round() * utils::ROTATE_SNAP; }
                self.angle = angle;
                Affine2::rotate_about(angle, center)
            },

            TransformHandle::Scale(handle_x, handle_y) => {
                // Move the grabbed handle along with the mouse
                let handle = handle_position(self.bounds, handle_x, handle_y);
                let mut target = handle + (pos - self.origin);
                if modifiers.ctrl { target = utils::snap_to_grid(target, grid); }

                // Scale each axis so the handle reaches the target, keeping the opposite side (or the center) in place.
                // Axes with no extent can't be scaled by a handle.
                let axis = |side: i8, center: f32, half: f32, handle: f32, target: f32| {
                    let anchor = if modifiers.alt { center } else { center - half * side as f32 };
                    let extent = handle - anchor;
                    if side == 0 || extent.abs() < 1e-6 { None }
                    else { Some(((target - anchor) / extent, anchor)) }
                };
                let x = axis(handle_x, center.x, half.x, handle.x, target.x);
                let y = axis(handle_y, center.y, half.y, handle.y, target.y);

                // Shift: scale every axis by the same amount, keeping the direction of each scaled axis.
                // Axes that aren't scaled by the handle scale around the center.
                let uniform = if modifiers.shift {
                    [x, y].into_iter().flatten().map(|(factor, _)| factor.abs()).reduce(f32::min)
                } else { None };
                let resolve = |axis: Option<(f32, f32)>, center: f32| match (axis, uniform) {
                    (Some((factor, anchor)), Some(uniform)) => (uniform.copysign(factor), anchor),
                    (Some((factor, anchor)), None) => (factor, anchor),
                    (None, uniform) => (uniform.unwrap_or(1.0), center),
                };
                let (factor_x, anchor_x) = resolve(x, center.x);
                let (factor_y, anchor_y) = resolve(y, center.y);
                Affine2::scale_about(Vec2::new(factor_x, factor_y), Vec2::new(anchor_x, anchor_y))
            },
        }
    }
}

/// Get the position of a handle on a box. Each axis is -1 for the min side, 1 for the max side, or 0 for the middle.
fn handle_position((min, max): Bounds, handle_x: i8, handle_y: i8) -> Vec2 {
    let center = (min + max) * 0.5;
    let half = (max - min) * 0.5;
    center + Vec2::new(half.x * handle_x as f32, half.y * handle_y as f32)
}

impl EditorTool for EditorToolZone {
    fn init() -> Box<Self> where Self : Sized { Box::default() }

    fn select(selected_points: SelectionType, state: ControllerStateInput) -> Box<Self> where Self : Sized {
        // Get all zones with a point selected
        let selected_zones: std::collections::HashSet<_> = selected_points.iter().filter_map(|&point_id| {
            state.objects.get_point_info(point_id).ok().map(|info| info.zone_id())
        }).collect();
        let selected_zones: Vec<_> = selected_zones.into_iter().collect();

        let selected_points_reference = points_from_zone_selection(&selected_zones[..], &state).unwrap();

        Box::new(Self { selected_zones, selected_points_reference, ..Default::default() })
    }

    fn deselect(&mut self, _state: ControllerStateInput) -> SelectionType<'_> {
        // Just return selection
        self.selection()
    }

    fn kind(&self) -> EditorToolKind { EditorToolKind::Zone }
    fn selection(&self) -> SelectionType<'_> {
        match &self.tentative_selection {
            Some((_, points)) => &points[..],
            None => &self.selected_points_reference[..],
        }
    }
    fn selection_box(&self) -> Option<(Vec2, Vec2)> {
        if let (Some(start), Some(end)) = (self.selection_origin, self.selection_box_end) {
            Some((start, end))
        } else {
            None
        }
    }
    fn transform_box(&self, objects: &ObjectsHandle) -> Option<TransformBox> {
        // While rotating, the box turns with the zones
        if let Some(drag) = &self.transform && drag.handle == TransformHandle::Rotate {
            let (min, max) = drag.bounds;
            return Some(TransformBox { center: (min + max) * 0.5, half_size: (max - min) * 0.5, angle: drag.angle });
        }

        // Otherwise it's around the selection, unless a selection box is being dragged out
        if self.selection_origin.is_some() { return None; }
        let (min, max) = objects.zones_bounds(&self.selected_zones).ok().flatten()?;
        Some(TransformBox { center: (min + max) * 0.5, half_size: (max - min) * 0.5, angle: 0.0 })
    }

    fn handle_click(&mut self, state: ControllerStateInput, pos: studio_math::Vec2, modifiers: InputModifiers) -> ToolResult {
        // Clicking a transform box handle does nothing, rather than changing the selection
        if let Some((TransformHandle::Scale(..), _)) = self.transform_hit(&state, pos)? { return self.ok(); }

        let (new_selection, _) = self.get_updated_selection(&state, pos, false, modifiers)?;
        self.set_selection(new_selection, &state)?;
        self.ok()
    }

    fn handle_drag_start(&mut self, state: ControllerStateInput, pos: studio_math::Vec2, modifiers: InputModifiers) -> ToolResult {
        // Dragging the transform box takes priority over everything else
        if let Some((handle, bounds)) = self.transform_hit(&state, pos)? {
            self.abandon_drag();
            let snapshot = state.objects.snapshot_zones(&self.selected_zones)?;
            self.transform = Some(TransformDrag { handle, bounds, origin: pos, angle: 0.0, snapshot });
            return self.ok();
        }

        let clicked_zone_info = self.zone_from_pos(&state, pos)?;
        let (new_selection, reference_delta) = self.get_updated_selection(&state, pos, true, modifiers)?;
        self.drag_origin = Some(pos);

        // If we clicked a zone, only update selection if the clicked zone isn't selected.
        if let Some(clicked_zone_id) = clicked_zone_info.0 {
            // Determine if this (or a sibling) is in the selected zone
            let mut selected = self.selected_zones.contains(&clicked_zone_id);
            if !selected && let Some(clicked_point_id) = clicked_zone_info.1 {
                for &zone_id in &self.selected_zones {
                    if let Some(sibling_id) = state.objects.sibling_in_zone(clicked_point_id, zone_id) { // can prolly make a helper for this
                        // If we clicked a point rather than a path, its sibling being here is enough
                        let Some(clicked_end_id) = clicked_zone_info.2 else {
                            selected = true;
                            break;
                        };
                        if let Some(end_id) = state.objects.sibling_in_zone(clicked_end_id, zone_id) {
                            let (start, count) = state.objects.get_zone_info(zone_id)?.range();
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
                self.set_selection(new_selection, &state)?;
                self.reference_delta = reference_delta;
            }

            // Otherwise keep selection as is and recalculate reference delta
            else {
                let first_zone_id = *self.selected_zones.iter().min().unwrap();
                let (first_point_id, _) = state.objects.get_zone_info(first_zone_id)?.range();
                let first_point_position = state.objects.get_point_info(first_point_id)?.position();
                self.reference_delta = Some(pos - first_point_position);
            }
        }

        // If we didn't click a zone, start dragging a box instead of the reference delta.
        // The selection is tentative until released, when it will select everything within the box.
        else {
            self.selection_origin = Some(pos);
            self.selection_box_end = Some(pos);
            self.set_tentative_selection(&[], &state, modifiers)?;
        }

        // Every step of dragging zones moves them from where they started
        if self.reference_delta.is_some() && !self.selected_zones.is_empty() {
            self.drag_snapshot = Some(state.objects.snapshot_zones(&self.selected_zones)?);
        }
        self.ok()
    }

    fn handle_dragging_to(&mut self, mut state: ControllerStateInput, pos: studio_math::Vec2, modifiers: InputModifiers) -> ToolResult {
        // Transform the selected zones from where they started if we are dragging the transform box
        if let Some(drag) = self.transform.as_mut() {
            let transform = drag.update(pos, modifiers, state.grid_size);
            let before = state.objects.bounds_around_points(&self.selected_points_reference)?;
            state.objects.apply_transform(&drag.snapshot, &transform)?;
            let after = state.objects.bounds_around_points(&self.selected_points_reference)?;
            return self.stale_in(merge_bounds(before, after));
        }

        // Drag selected zones if we are doing that, moving the first point of the first zone to the target
        else if let (Some(reference_delta), Some(origin), Some(snapshot)) = (self.reference_delta, self.drag_origin, &self.drag_snapshot) {
            let start = origin - reference_delta;
            let target = utils::snap_drag_target(pos - reference_delta, start, origin, pos, modifiers, state.grid_size);
            return self.transform_from_snapshot(&mut state, snapshot, &Affine2::translate(target - start));
        }
        // Or if we are dragging a selection box, tentatively select the zones in the box
        else if let Some(selection_origin) = self.selection_origin {
            self.selection_box_end = Some(pos);
            let selected_points = state.objects.get_points_in_rect(selection_origin, pos)?;
            self.set_tentative_selection(&selected_points[..], &state, modifiers)?;
        }
        self.ok()
    }

    fn handle_drag_released(&mut self, _state: ControllerStateInput, _modifiers: InputModifiers) -> ToolResult {
        // Commit the tentative selection, if any
        if let Some((zones, points)) = self.tentative_selection.take() {
            self.selected_zones = zones;
            self.selected_points_reference = points;
        }

        // Release drag on selected zones, but keep them selected
        self.transform = None;
        self.reference_delta = None;
        self.drag_origin = None;
        self.drag_snapshot = None;
        self.selection_origin = None;
        self.selection_box_end = None;
        self.ok()
    }

    fn handle_cancel(&mut self, mut state: ControllerStateInput) -> ToolResult {
        // If transforming zones, put them back how they started
        if let Some(drag) = self.transform.take() {
            let before = state.objects.bounds_around_points(&self.selected_points_reference)?;
            state.objects.restore_snapshot(&drag.snapshot)?;
            let after = state.objects.bounds_around_points(&self.selected_points_reference)?;
            return self.stale_in(merge_bounds(before, after));
        }

        // If dragging zones, put them back where they started
        if let (Some(_), Some(_)) = (self.reference_delta.take(), self.drag_origin.take()) {
            if let Some(snapshot) = self.drag_snapshot.take() {
                let before = state.objects.bounds_around_points(&self.selected_points_reference)?;
                state.objects.restore_snapshot(&snapshot)?;
                let after = state.objects.bounds_around_points(&self.selected_points_reference)?;
                return self.stale_in(merge_bounds(before, after));
            }
        }
        // If dragging a selection box, just abandon the box and keep the previous selection
        else if self.selection_origin.is_some() {
            self.tentative_selection = None;
            self.selection_origin = None;
            self.selection_box_end = None;
        }
        // Otherwise clear selection
        else {
            self.set_selection(vec![], &state)?;
        }
        self.ok()
    }

    fn handle_delete(&mut self, mut state: ControllerStateInput) -> ToolResult {
        // Stop dragging out selection box
        if self.selection_origin.is_some() {
            self.tentative_selection = None;
            self.selection_origin = None;
            self.selection_box_end = None;
        }
        // Or Delete selected zones
        else if !self.selected_zones.is_empty() {
            self.selected_zones.sort();

            // Deleting a zone changes the IDs of every zone after it, so they all need to be reassigned
            let bounds = state.objects.bounds_from_zone(self.selected_zones[0])?;
            let mut offset = 0;
            #[allow(clippy::explicit_counter_loop)] // offset counts deletions, not iterations
            for zone_id in &self.selected_zones[..] {
                state.objects.delete_zone(*zone_id - offset)?;
                offset += 1;
            }

            self.set_selection(Vec::new(), &state)?;
            self.reference_delta = None;

            return self.stale_in(bounds);
        }
        self.ok()
    }

    fn handle_select_all(&mut self, state: ControllerStateInput) -> ToolResult {
        // Abandon any drag, then select every zone
        self.abandon_drag();
        self.set_selection((0..state.objects.zone_count()).collect(), &state)?;
        self.ok()
    }

    fn handle_paste(&mut self, mut state: ControllerStateInput, objects: &SavedObjects, placement: PastePlacement) -> ToolResult {
        // Abandon any drag, then add the zones where they were copied from
        self.abandon_drag();
        let zones = state.objects.paste_zones(objects)?;

        // Move them into place along with their shapes, and select them
        if let Some(center) = state.objects.zones_center(&zones)? {
            let target = placement.target(center, state.camera_inv_scale);
            let snapshot = state.objects.snapshot_zones(&zones)?;
            state.objects.apply_transform(&snapshot, &Affine2::translate(target - center))?;
        }
        self.set_selection(zones, &state)?;
        self.stale_in(state.objects.zones_bounds(&self.selected_zones)?)
    }

    fn handle_special_action(&mut self, mut state: ControllerStateInput, action: SpecialAction) -> ToolResult {
        if self.selected_zones.is_empty() { return self.ok(); }
        match action {
            SpecialAction::NormalSet(normal, _) => {
                state.objects.set_zone_normals(&self.selected_zones, normal)?;
                self.stale_in(state.objects.zones_bounds(&self.selected_zones)?)
            },
            SpecialAction::SelectionSeparated => {
                state.objects.separate_points(&self.selected_points_reference)?;
                self.ok()
            },
            // World space has +y down, so turning from +x towards +y is clockwise
            SpecialAction::SelectionFlipped(axis) => {
                let factors = match axis {
                    Axis::Vertical => Vec2::new(-1.0, 1.0),
                    Axis::Horizontal => Vec2::new(1.0, -1.0),
                };
                self.transform_selected_zones(&mut state, |center| Affine2::scale_about(factors, center))
            },
            SpecialAction::SelectionRotated(clockwise) => {
                let angle = if clockwise { std::f32::consts::FRAC_PI_2 } else { -std::f32::consts::FRAC_PI_2 };
                self.transform_selected_zones(&mut state, |center| Affine2::rotate_about(angle, center))
            },
            // Every ID changes in the new order, so the selection follows its zones to their new IDs
            SpecialAction::ZonesReordered(change) => {
                if self.drag_snapshot.is_some() || self.transform.is_some() || self.selection_origin.is_some() { return self.ok(); }
                // Zones already as far as they can go stay where they are
                let Some(new_order) = state.objects.changed_zone_order(&self.selected_zones, change)? else { return self.ok(); };
                let bounds = state.objects.zones_bounds(&self.selected_zones)?;
                state.objects.reorder_zones(&new_order)?;
                let mut selected: Vec<u16> = self.selected_zones.iter()
                    .filter_map(|zone| new_order.iter().position(|old| old == zone).map(|new| new as u16))
                    .collect();
                selected.sort();
                self.set_selection(selected, &state)?;
                self.stale_in(bounds)
            },
            // Arrow keys don't fight the mouse over a drag in progress
            SpecialAction::SelectionNudged(delta, _) => {
                if self.drag_snapshot.is_some() || self.transform.is_some() || self.selection_origin.is_some() { return self.ok(); }
                self.transform_selected_zones(&mut state, |_| Affine2::translate(delta))
            },
            SpecialAction::ShapesCleared => {
                let mut shape_ids = vec![];
                for &zone_id in &self.selected_zones {
                    shape_ids.extend(state.objects.zone_shape_ids(zone_id)?);
                }
                let bounds = state.objects.shapes_reach_bounds(&shape_ids)?;
                state.objects.clear_zone_shapes(&self.selected_zones)?;
                self.stale_in(bounds)
            },
            _ => self.ok(),
        }
    }
}

impl EditorToolZone {
    /// Stop dragging zones or a selection box, keeping the committed selection
    fn abandon_drag(&mut self) {
        self.tentative_selection = None;
        self.transform = None;
        self.reference_delta = None;
        self.drag_origin = None;
        self.drag_snapshot = None;
        self.selection_origin = None;
        self.selection_box_end = None;
    }

    /// Transform the selected zones from a snapshot of them, marking normals stale where the zones were and where they are now.
    /// Siblings move along with the zones, so their zones are included.
    fn transform_from_snapshot(&self, state: &mut ControllerStateInput, snapshot: &ZonesSnapshot, transform: &Affine2) -> ToolResult {
        let before = state.objects.bounds_around_points(&self.selected_points_reference)?;
        state.objects.apply_transform(snapshot, transform)?;
        let after = state.objects.bounds_around_points(&self.selected_points_reference)?;
        self.stale_in(merge_bounds(before, after))
    }

    /// Transform the selected zones around the center of their bounds, marking normals stale where they were and where they are now.
    /// Siblings move along with the zones, as when dragging the transform box.
    fn transform_selected_zones(&mut self, state: &mut ControllerStateInput, transform: impl FnOnce(Vec2) -> Affine2) -> ToolResult {
        self.abandon_drag();
        let Some((min, max)) = state.objects.zones_bounds(&self.selected_zones)? else { return self.ok(); };
        let snapshot = state.objects.snapshot_zones(&self.selected_zones)?;
        self.transform_from_snapshot(state, &snapshot, &transform((min + max) * 0.5))
    }

    /// Get the part of the transform box around the selected zones at a position, and the box itself, if any.
    /// Handles take priority over rotating, and corners over edges.
    fn transform_hit(&self, state: &ControllerStateInput, pos: Vec2) -> anyhow::Result<Option<(TransformHandle, Bounds)>> {
        if self.selected_zones.is_empty() { return Ok(None); }
        let Some(bounds) = state.objects.zones_bounds(&self.selected_zones)? else { return Ok(None); };

        // Scale handles, at the same size as control points. Short edges don't have midpoint handles.
        let tolerance = utils::CLICK_TOLERANCE * state.camera_inv_scale;
        let min_edge = utils::MIN_MIDPOINT_EDGE * state.camera_inv_scale;
        let size = bounds.1 - bounds.0;
        const HANDLES: [(i8, i8); 8] = [(-1, -1), (1, -1), (1, 1), (-1, 1), (0, -1), (1, 0), (0, 1), (-1, 0)];
        for (handle_x, handle_y) in HANDLES {
            if (handle_x == 0 && size.x < min_edge) || (handle_y == 0 && size.y < min_edge) { continue; }
            let delta = handle_position(bounds, handle_x, handle_y) - pos;
            if delta.x.abs().max(delta.y.abs()) <= tolerance {
                return Ok(Some((TransformHandle::Scale(handle_x, handle_y), bounds)));
            }
        }

        // Rotating, just outside the corners
        let (min, max) = bounds;
        let inside = pos.x >= min.x && pos.x <= max.x && pos.y >= min.y && pos.y <= max.y;
        let rotate_tolerance = utils::ROTATE_TOLERANCE * state.camera_inv_scale;
        if !inside && HANDLES[..4].iter().any(|&(x, y)| handle_position(bounds, x, y).distance(pos) <= rotate_tolerance) {
            return Ok(Some((TransformHandle::Rotate, bounds)));
        }
        Ok(None)
    }

    fn set_selection(&mut self, selected_zones: Vec<u16>, state: &ControllerStateInput) -> anyhow::Result<()> {
        self.selected_zones = selected_zones;
        self.selected_points_reference = points_from_zone_selection(&self.selected_zones[..], state)?;
        Ok(())
    }

    /// Set the tentative selection from the points inside a selection box, applying modifiers against the committed selection.
    /// SHIFT adds, ALT removes, SHIFT+ALT toggles, and CTRL only selects the exact zones instead of all connected zones.
    fn set_tentative_selection(&mut self, boxed_points: &[u16], state: &ControllerStateInput, modifiers: InputModifiers) -> anyhow::Result<()> {
        let mut boxed = std::collections::HashSet::new();
        for &point_id in boxed_points {
            let zone_id = state.objects.get_point_info(point_id)?.zone_id();
            if modifiers.ctrl {
                boxed.insert(zone_id);
            } else {
                boxed.extend(get_connected_zones(zone_id, state)?);
            }
        }

        let boxed: Vec<u16> = boxed.into_iter().collect();
        let zones = utils::combine_selection(&self.selected_zones[..], &boxed[..], modifiers, false);
        let points = points_from_zone_selection(&zones[..], state)?;
        self.tentative_selection = Some((zones, points));
        Ok(())
    }

    /// Get a single zone selection from a click position.
    /// Returns the ID of the zone that was clicked, and the ID of the clicked point or its path start/end
    fn zone_from_pos(&self, state: &ControllerStateInput, pos: Vec2) -> anyhow::Result<(Option<u16>, Option<u16>, Option<u16>)> {
        // Check the point
        if let Some((zone_id, point_id)) = utils::get_clicked_control_point(state, None, pos)? {
            Ok((Some(zone_id), Some(point_id), None))
        }

        // Check the curve
        else if let Some((zone_id, start_id, _, _)) = utils::get_clicked_zone_path(state, pos)? {
            let (start, count) = state.objects.get_zone_info(zone_id)?.range();
            Ok((Some(zone_id), Some(start_id), Some(start + (start_id - start + 1) % count)))
        }

        // Notihing is clicked
        else {
            Ok((None, None, None))
        }
    }

    /// Given a click (or drag start) point and input info, and whether or not this is a drag action,
    /// and using the state of the current selection, return a new selection.
    /// Also return delta from the lowest ID selected point to the click position, in case this is a drag start.
    fn get_updated_selection(&self, state: &ControllerStateInput, pos: Vec2, dragging: bool, modifiers: InputModifiers) -> anyhow::Result<(Vec<u16>, Option<Vec2>)> {
        let (zone_id, point_id, neighbor_id) = self.zone_from_pos(state, pos)?;

        // If a zone is clicked, determine point/path/etc
        if let Some(zone_id) = zone_id {
            // If ctrl is clicked, only operate on the exact clicked zone
            let new_selection = if modifiers.ctrl {
                // If a modifier key is selected do what you would expect
                if modifiers.shift || modifiers.alt {
                    utils::combine_selection(&self.selected_zones[..], &[zone_id], modifiers, true)
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
                                    let (start, count) = state.objects.get_zone_info(selected_zone_id)?.range();
                                    let projected_next_id = start + (point_id - start + 1) % count;
                                    if neighbor_id == projected_next_id {
                                        let sibling_id = state.objects.get_sibling(point_id)?;
                                        vec![state.objects.get_point_info(sibling_id)?.zone_id()]
                                    } else { vec![zone_id] }
                                } else { vec![zone_id] } // return selected zone
                            } else {
                                // Otherwise, just return the next zone.
                                let sibling_id = state.objects.get_sibling(point_id)?;
                                vec![state.objects.get_point_info(sibling_id)?.zone_id()]
                            }
                        }

                        // Otherwise it's a different point in the selected zone
                        else { vec![zone_id] }
                    }

                    // Otherwise just return the zone
                    else { vec![zone_id] }
                }
            }

            // Otherwise, add/remove/toggle all connected zones
            else {
                let all_connected = get_connected_zones(zone_id, state)?;
                utils::combine_selection(&self.selected_zones[..], &all_connected[..], modifiers, true)
            };

            let reference_delta = if !dragging || new_selection.is_empty() { None } else {
                // Get the difference in position with the first point in the first zone
                let first_zone_id = *new_selection.iter().min().unwrap();
                let (first_point_id, _) = state.objects.get_zone_info(first_zone_id)?.range();
                let first_point_position = state.objects.get_point_info(first_point_id)?.position();
                Some(pos - first_point_position)
            };

            Ok((new_selection, reference_delta))
        }

        // If no zone is clicked, deselect or return selection unmodified
        else {
            if modifiers.shift || modifiers.alt {
                // We are adding to/removing from/toggling selected zones, so leave unmodified
                Ok((self.selected_zones.clone(), None))
            } else {
                Ok((vec![], None))
            }
        }
    }
}

/// Get the points we have selected from the zone we have selected
fn points_from_zone_selection(selected_zones: &[u16], state: &ControllerStateInput) -> anyhow::Result<Vec<u16>> {
    let mut points = vec![];
    for zone_id in selected_zones {
        let (start, count) = state.objects.get_zone_info(*zone_id)?.range();
        points.extend((start..start + count).collect::<Vec<u16>>());
    }
    Ok(points)
}

fn get_connected_zones(zone_id: u16, state: &ControllerStateInput) -> anyhow::Result<Vec<u16>> {
    let (point_id, _) = state.objects.get_zone_info(zone_id)?.range();
    let connected_points = state.objects.get_connected_points(point_id, true)?;
    let connected_zones: std::collections::HashSet<_> = connected_points.iter().filter_map(|&point_id| {
        state.objects.get_point_info(point_id).ok().map(|info| info.zone_id())
    }).collect(); // Collect as hash set first to remove duplicates
    Ok(connected_zones.into_iter().collect())
}

/* ******************************** */
/*           TESTS MODULE           */
/* ******************************** */

#[cfg(test)]
mod tests {
use crate::{ObjectsHandle, InputModifiers as IM};
    use crate::objects::test_utils::{add_square, add_zone};

    use super::*;
    use super::super::test_utils::{state, state_scaled};

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
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);

        let tool = EditorToolZone::select(&[5], state(&objects));
        assert_eq!(tool.selection(), &[4, 5, 6]);
    }

    /// Clicking a control point or a path selects the zone, and clicking nothing deselects
    #[test]
    fn click_selects_zone() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(40.0, 0.0), (140.0, 0.0), (140.0, 100.0)]);
        let mut tool = EditorToolZone::init();

        let result = tool.handle_click(state(&objects), Vec2::new(10.5, 0.0), Default::default()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert!(result.normals_stale.is_none());
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
                a.iter().copied().collect::<std::collections::HashSet<u16>>(),
                b.iter().copied().collect::<std::collections::HashSet<u16>>(),
            );
        };

        let mut objects = ObjectsHandle::headless();

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
        assert!(result.normals_stale.is_none());
        assert_eq_contents(tool.selection(), &[0, 1, 2, 3, 4, 5, 6, 7]);

        tool.handle_click(state(&objects), Vec2::new(90.0, 1.0), Default::default()).unwrap(); // path
        assert_eq_contents(tool.selection(), &[8, 9, 10, 11, 12, 13]);

        tool.handle_click(state(&objects), Vec2::new(25.0, 25.0), Default::default()).unwrap();
        assert!(tool.selection().is_empty());
    }

    /// Clicking a control point or a path selects the all connected zones
    #[test]
    fn ctrl_click_selects_only_zone() {
        let mut objects = ObjectsHandle::headless();

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
        assert!(result.normals_stale.is_none());
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
        let mut objects = ObjectsHandle::headless();
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
        assert!(result.normals_stale.is_none());
        assert_selected_zones(&tool, &[0]);

        tool.handle_click(state(&objects), Vec2::new(90.0, 1.0), IM::shift()).unwrap(); // path
        assert_selected_zones(&tool, &[0, 1]);

        tool.handle_click(state(&objects), Vec2::new(25.0, 25.0), IM::shift()).unwrap();
        assert_selected_zones(&tool, &[0, 1]);

        tool.handle_click(state(&objects), Vec2::new(25.0, 25.0), IM::alt()).unwrap();
        assert_selected_zones(&tool, &[0, 1]);

        tool.handle_click(state(&objects), Vec2::new(0.0, 7.0), IM::alt()).unwrap(); // path
        assert_selected_zones(&tool, &[1]);

        tool.handle_click(state(&objects), Vec2::new(140.0, 7.0), IM::alt()).unwrap();
        assert!(tool.selection().is_empty());
    }

    /// Test different combinations with multiple modifier keys
    #[test]
    fn multiple_modifier_keys() {
            let mut objects = ObjectsHandle::headless();

        // Add one set of connected zones
        add_square(&mut objects, 10.0);
        let (branch_zone, _) = objects.create_branching_zone(2).unwrap();
        objects.create_point(branch_zone, Vec2::new(10.0, 20.0)).unwrap();
        objects.create_point(branch_zone, Vec2::new(0.0, 13.0)).unwrap();
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
        let result = tool.handle_click(state_scaled(&objects, 0.25), Vec2::new(10.5, 0.0), IM::ctrl()).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert!(result.normals_stale.is_none());
        assert_selected_zones(&tool, &[0]);

        // Ctrl+Shift to select a zone on the other graph
        tool.handle_click(state_scaled(&objects, 0.25), Vec2::new(90.0, 1.0), IM::shift_ctrl()).unwrap(); // path
        assert_selected_zones(&tool, &[0, 2]);

        // Shift+Alt on second connected to toggle the whole first graph on
        tool.handle_click(state_scaled(&objects, 0.25), Vec2::new(0.0, 13.0), IM::shift_alt()).unwrap();
        assert_selected_zones(&tool, &[0, 1, 2]);

        // Shift+Alt again to toggle the whole first graph off
        tool.handle_click(state_scaled(&objects, 0.25), Vec2::new(0.0, 13.0), IM::shift_alt()).unwrap();
        assert_selected_zones(&tool, &[2]);

        // Shift to turn the whole first graph back on
        tool.handle_click(state_scaled(&objects, 0.25), Vec2::new(0.0, 13.0), IM::shift()).unwrap();
        assert_selected_zones(&tool, &[0, 1, 2]);

        // Ctrl+Alt on second to deselect it
        tool.handle_click(state_scaled(&objects, 0.25), Vec2::new(0.0, 13.0), IM::ctrl_alt()).unwrap();
        assert_selected_zones(&tool, &[0, 2]);

        // Alt on second to deselect whole first graph, even though the clicked zone isn't selected
        tool.handle_click(state_scaled(&objects, 0.25), Vec2::new(0.0, 13.0), IM::alt()).unwrap();
        assert_selected_zones(&tool, &[2]);

        // Shift+Ctrl+Alt to toggle a zone on the first graph on
        tool.handle_click(state_scaled(&objects, 0.25), Vec2::new(0.0, 13.0), IM::all()).unwrap();
        assert_selected_zones(&tool, &[1, 2]);

        // Shift+Ctrl on final zone to select full second graph
        tool.handle_click(state_scaled(&objects, 0.25), Vec2::new(40.0, 100.0), IM::shift_ctrl()).unwrap();
        assert_selected_zones(&tool, &[1, 2, 3]);

        // Shift+Ctrl+Alt to toggle a zone on the second graph off
        tool.handle_click(state_scaled(&objects, 0.25), Vec2::new(90.0, 00.0), IM::all()).unwrap();
        assert_selected_zones(&tool, &[1, 3]);

        // No modifier keys to select full first zone and deselect the other shape
        tool.handle_click(state_scaled(&objects, 0.25), Vec2::new(0.0, 13.0), IM::none()).unwrap();
        assert_selected_zones(&tool, &[0, 1]);

        // Alt to deselect whole first zone
        tool.handle_click(state_scaled(&objects, 0.25), Vec2::new(0.0, 13.0), IM::alt()).unwrap();
        assert!(tool.selection().is_empty())
    }

    /// Ctrl+clicking a point shared by two zones cycles through them
    #[test]
    fn click_cycles_shared_points() {
        // The shared points and path are away from the corners and edge midpoints of both zones, where clicks would hit the transform box
        let mut objects = ObjectsHandle::headless();
        add_zone(&mut objects, &[(0.0, 0.0), (10.0, 0.0), (40.0, 0.0), (40.0, 10.0), (0.0, 10.0)]);
        let (zone_id, _) = objects.create_branching_zone(1).unwrap();
        objects.create_point(zone_id, Vec2::new(0.0, -10.0)).unwrap();
        objects.create_point(zone_id, Vec2::new(30.0, -10.0)).unwrap();
        objects.complete_branching_zone(1, zone_id, 2).unwrap();
        let state = |objects: &ObjectsHandle| state_scaled(objects, 0.25);
        let mut tool = EditorToolZone::init();

        tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), IM::ctrl()).unwrap();
        let first = tool.selection().to_vec();
        tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), IM::ctrl()).unwrap();
        let second = tool.selection().to_vec();
        assert_ne!(first, second);
        tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), IM::ctrl()).unwrap();
        assert_eq!(tool.selection(), &first[..]);

        // Check clicks on the shared path as well
        tool.handle_click(state(&objects), Vec2::new(30.0, 0.0), IM::ctrl()).unwrap();
        assert_eq!(tool.selection(), &second[..]);
        tool.handle_click(state(&objects), Vec2::new(30.0, 0.0), IM::ctrl()).unwrap();
        assert_eq!(tool.selection(), &first[..]);

        // Check a ctrl click on a non-shared path of the first zone, from either zone
        let first_zone = objects.get_point_info(first[0]).unwrap().zone_id();
        let unshared = if first_zone == 0 { Vec2::new(5.0, 10.0) } else { Vec2::new(15.0, -10.0) };
        tool.handle_click(state(&objects), unshared, IM::ctrl()).unwrap();
        assert_eq!(tool.selection(), &first[..]);
        tool.handle_click(state(&objects), Vec2::new(10.0, 0.0), IM::ctrl()).unwrap();
        tool.handle_click(state(&objects), unshared, IM::ctrl()).unwrap();
        assert_eq!(tool.selection(), &first[..]);
    }

    /// Dragging a zone moves it by where it was grabbed, and marks the normals stale where it was and where it is now
    #[test]
    fn drag_moves_zone() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolZone::init();

        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 10.0), Default::default()).unwrap();
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(15.0, 12.0), Default::default()).unwrap();
        assert_eq!(result.normals_stale, Some((Vec2::ZERO, Vec2::new(15.0, 12.0))));
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(5.0, 2.0));
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 12.0));

        // Releasing keeps the zone selected but stops the drag
        tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(tool.selection().len(), 4);
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(100.0, 100.0), Default::default()).unwrap();
        assert!(result.normals_stale.is_none());
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 12.0));
    }

    /// Dragging with nothing selected does nothing
    #[test]
    fn drag_nothing() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolZone::init();

        tool.handle_drag_start(state(&objects), Vec2::new(50.0, 50.0), Default::default()).unwrap();
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(60.0, 60.0), Default::default()).unwrap();
        assert!(result.normals_stale.is_none());
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::ZERO);
    }

    /// Dragging on an unselected zone, with another selected, drags the newly selected one instead
    #[test]
    fn drag_with_selected() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(40.0, 0.0), (140.0, 0.0), (140.0, 100.0)]);
        let mut tool = EditorToolZone::init();

        // Select the first zone
        tool.handle_click(state(&objects), Vec2::new(10.0, 10.0), Default::default()).unwrap();

        // Drag the second zone
        tool.handle_drag_start(state(&objects), Vec2::new(140.0, 100.0), Default::default()).unwrap();
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(145.0, 102.0), Default::default()).unwrap();
        assert!(result.normals_stale.is_some());
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(0.0, 0.0));
        assert_eq!(objects.get_point_info(4).unwrap().position(), Vec2::new(45.0, 2.0));
        assert_eq!(objects.get_point_info(6).unwrap().position(), Vec2::new(145.0, 102.0));

        // Releasing keeps the zone selected but stops the drag
        tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(tool.selection().len(), 3);
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(100.0, 100.0), Default::default()).unwrap();
        assert!(result.normals_stale.is_none());
        assert_eq!(objects.get_point_info(6).unwrap().position(), Vec2::new(145.0, 102.0));
    }

    /// Dragging a selected zone doesn't change the selection regardless of modifier keys
    #[test]
    fn drag_selected_doesnt_change_selection() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(40.0, 0.0), (140.0, 0.0), (140.0, 100.0)]);
        let mut tool = EditorToolZone::init();

        // Select both zones
        tool.handle_click(state(&objects), Vec2::new(10.0, 10.0), Default::default()).unwrap();
        tool.handle_click(state(&objects), Vec2::new(90.0, 1.0), IM::shift()).unwrap();

        // Drag the second zone, without holding shift, away from the transform box handles
        tool.handle_drag_start(state(&objects), Vec2::new(40.0, 0.0), Default::default()).unwrap();
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(45.0, 2.0), Default::default()).unwrap();
        assert!(result.normals_stale.is_some());
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(5.0, 2.0));
        assert_eq!(objects.get_point_info(4).unwrap().position(), Vec2::new(45.0, 2.0));
        assert_eq!(objects.get_point_info(6).unwrap().position(), Vec2::new(145.0, 102.0));

        // Releasing keeps the zone selected but stops the drag
        tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(tool.selection().len(), 7);
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(100.0, 100.0), Default::default()).unwrap();
        assert!(result.normals_stale.is_none());
        assert_eq!(objects.get_point_info(6).unwrap().position(), Vec2::new(145.0, 102.0));
    }

    /// Dragging the selected zones doesn't apply to each sibling individually, as that would move each shared point multiple times.
    #[test]
    fn drag_moves_siblings_once() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let (branch_zone, _) = objects.create_branching_zone(2).unwrap();
        objects.create_point(branch_zone, Vec2::new(10.0, 20.0)).unwrap();
        objects.create_point(branch_zone, Vec2::new(0.0, 20.0)).unwrap();
        objects.complete_branching_zone(2, branch_zone, 3).unwrap();
        let mut tool = EditorToolZone::init();

        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 10.0), Default::default()).unwrap();
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(15.0, 12.0), Default::default()).unwrap();
        assert!(result.normals_stale.is_some());

        // These points are two sets of siblings. Moving one propagates to the other, so the whole group should only be moved once.
        assert_eq!(objects.get_point_info(3).unwrap().position(), Vec2::new(5.0, 12.0));
        assert_eq!(objects.get_point_info(4).unwrap().position(), Vec2::new(5.0, 12.0));
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 12.0));
        assert_eq!(objects.get_point_info(7).unwrap().position(), Vec2::new(15.0, 12.0));

        // Releasing keeps the zone selected but stops the drag
        tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(tool.selection().len(), 8);
        let result = tool.handle_dragging_to(state(&objects), Vec2::new(100.0, 100.0), Default::default()).unwrap();
        assert!(result.normals_stale.is_none());
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 12.0));
    }

    /// Test selecting zones by dragging over selected points
    #[test]
    fn drag_to_select() {
        let mut objects = ObjectsHandle::headless();
        
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (20.0, 10.0)]);
        add_zone(&mut objects, &[(40.0, 10.0), (50.0, 10.0), (50.0, 0.0)]);

        let (branching_zone, _) = objects.create_branching_zone(9).unwrap();
        objects.create_point(branching_zone, Vec2::new(55.0, 5.0)).unwrap();
        objects.complete_branching_zone(9, branching_zone, 8).unwrap();

        let mut tool = EditorToolZone::init();

        let check_selection = |actual: &[u16], expected: &[u16]| 
        assert_eq!(
            actual.iter().copied().collect::<std::collections::HashSet<u16>>(),
            expected.iter().copied().collect::<std::collections::HashSet<u16>>(),
        );

        // Selection box should be empty when not dragging
        assert_eq!(tool.selection_box(), None);

        // Start dragging from above and between zones 2 and 3, downward and to the right over zone 3
        tool.handle_drag_start(state(&objects), Vec2::new(35.0, -7.0), Default::default()).unwrap();
        assert_eq!(tool.selection_box(), Some((Vec2::new(35.0, -7.0), Vec2::new(35.0, -7.0))));
        assert_eq!(tool.selection(), &[] as &[u16]);

        let result = tool.handle_dragging_to(state(&objects), Vec2::new(45.0, 17.0), Default::default()).unwrap();
        assert!(result.normals_stale.is_none());
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

    /// Shift constrains dragging to an axis, and ctrl snaps to the grid
    #[test]
    fn drag_modifiers() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolZone::init();

        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 10.0), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(15.0, 12.0), IM::shift()).unwrap();
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(5.0, 0.0));

        tool.handle_dragging_to(state(&objects), Vec2::new(11.0, 18.0), IM::shift()).unwrap();
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(0.0, 8.0));

        // Diagonal: mouse moved (+4, -6), which is closer to the diagonal than the vertical
        tool.handle_dragging_to(state(&objects), Vec2::new(14.0, 4.0), IM::shift()).unwrap();
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(5.0, -5.0));

        tool.handle_dragging_to(state(&objects), Vec2::new(15.4, 12.6), IM::ctrl()).unwrap();
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(5.0, 3.0));
    }

    /// Box selection is tentative until released, follows modifiers, and can be cancelled
    #[test]
    fn drag_box_modifiers() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(40.0, 0.0), (140.0, 0.0), (140.0, 100.0)]);
        let mut tool = EditorToolZone::init();

        tool.handle_click(state(&objects), Vec2::new(10.0, 10.0), Default::default()).unwrap();
        assert_eq!(tool.selection().len(), 4);

        // Shift adds to the selection, but only tentatively
        tool.handle_drag_start(state(&objects), Vec2::new(35.0, -7.0), IM::shift()).unwrap();
        assert_eq!(tool.selection().len(), 4);
        tool.handle_dragging_to(state(&objects), Vec2::new(60.0, 12.0), IM::shift()).unwrap();
        assert_eq!(tool.selection().len(), 7);
        tool.handle_cancel(state(&objects)).unwrap();
        assert_eq!(tool.selection().len(), 4);
        tool.handle_drag_released(state(&objects), IM::shift()).unwrap();
        assert_eq!(tool.selection().len(), 4);

        // Alt removes from the selection once released
        tool.handle_drag_start(state(&objects), Vec2::new(-40.0, -40.0), IM::alt()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(5.0, 5.0), IM::alt()).unwrap();
        assert!(tool.selection().is_empty());
        tool.handle_drag_released(state(&objects), IM::alt()).unwrap();
        assert!(tool.selection().is_empty());
    }

    /// Cancel input should deselect everything
    #[test]
    fn cancel_deselects() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(40.0, 0.0), (50.0, 0.0), (50.0, 10.0)]);
        let mut tool = EditorToolZone::init();

        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
        tool.handle_click(state(&objects), Vec2::new(40.0, 0.0), IM::shift()).unwrap();
        assert_eq!(tool.selection().len(), 7);

        tool.handle_cancel(state(&objects)).unwrap();
        assert!(tool.selection().is_empty());
    }

    /// Cancel while dragging zones puts them back and keeps the selection
    #[test]
    fn cancel_drag_restores_position() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolZone::init();

        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 10.0), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(25.0, 32.0), Default::default()).unwrap();
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(15.0, 22.0));

        let result = tool.handle_cancel(state(&objects)).unwrap();
        assert!(result.normals_stale.is_some());
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::ZERO);
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(10.0, 10.0));
        assert_eq!(tool.selection().len(), 4);

        // The drag is over, so releasing or moving more does nothing
        tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(50.0, 50.0), Default::default()).unwrap();
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::ZERO);

        // A second cancel deselects
        tool.handle_cancel(state(&objects)).unwrap();
        assert!(tool.selection().is_empty());
    }

    /// Cancel while dragging a selection box stops the box and restores the previous selection
    #[test]
    fn cancel_box_restores_selection() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(40.0, 0.0), (50.0, 0.0), (50.0, 10.0)]);
        let mut tool = EditorToolZone::init();

        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
        assert_eq!(tool.selection().len(), 4);

        // Drag a box over the other zone, replacing the selection tentatively
        tool.handle_drag_start(state(&objects), Vec2::new(30.0, -30.0), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(60.0, 20.0), Default::default()).unwrap();
        assert_eq!(tool.selection().len(), 3);
        assert!(tool.selection_box().is_some());

        tool.handle_cancel(state(&objects)).unwrap();
        assert_eq!(tool.selection().len(), 4);
        assert_eq!(tool.selection_box(), None);

        // Releasing afterwards doesn't change anything
        tool.handle_drag_released(state(&objects), Default::default()).unwrap();
        assert_eq!(tool.selection().len(), 4);
    }

    /// Deleting removes selected zones, and clears the selection
    #[test]
    fn delete_zones() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(40.0, 0.0), (50.0, 0.0), (50.0, 10.0)]);
        let mut tool = EditorToolZone::init();

        assert!(tool.handle_delete(state(&objects)).unwrap().normals_stale.is_none());
        assert_eq!(objects.zone_count(), 2);

        // Every zone after the deleted one changes ID, so they are all stale
        tool.handle_click(state(&objects), Vec2::new(0.0, 0.0), Default::default()).unwrap();
        let stale = tool.handle_delete(state(&objects)).unwrap().normals_stale;
        assert_eq!(stale, Some((Vec2::ZERO, Vec2::new(50.0, 10.0))));
        assert_eq!(objects.object_counts(), (1, 3));
        assert!(tool.selection().is_empty());
    }

    /// Deselecting gives back the selected points
    #[test]
    fn deselect_returns_selection() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolZone::select(&[0], state(&objects));
        assert_eq!(tool.deselect(state(&objects)), &[0, 1, 2, 3]);
    }

    /// Setting the normal sets it for the selected zones
    #[test]
    fn special_normal_set() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);
        let mut tool = EditorToolZone::select(&[0, 1, 2, 3], state(&objects));

        let result = tool.handle_special_action(state(&objects), SpecialAction::NormalSet(studio_math::Vec3::UP, false)).unwrap();
        assert!(result.normals_stale.is_some());
        assert_eq!(objects.get_zone_info(0).unwrap().normal(), studio_math::Vec3::UP);
        assert_eq!(objects.get_zone_info(1).unwrap().normal(), studio_math::Vec3::FORWARD);
    }

    /// Separating splits the selected zones' points from their siblings in other zones
    #[test]
    fn special_separate() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let (_, branch_id) = objects.create_branching_zone(1).unwrap();
        let mut tool = EditorToolZone::select(&[branch_id], state(&objects));

        tool.handle_special_action(state(&objects), SpecialAction::SelectionSeparated).unwrap();
        assert!(objects.get_siblings(1).is_empty());
    }

    /// Flipping mirrors the selected zones around the center of their bounds, leaving other zones alone
    #[test]
    fn special_flip() {
        let mut objects = ObjectsHandle::headless();
        add_zone(&mut objects, &[(20.0, 0.0), (40.0, 0.0), (40.0, 10.0)]);
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolZone::select(&[0], state(&objects));

        let result = tool.handle_special_action(state(&objects), SpecialAction::SelectionFlipped(Axis::Vertical)).unwrap();
        assert!(result.normals_stale.is_some());
        assert_eq!(position(&objects, 0), Vec2::new(40.0, 0.0));
        assert_eq!(position(&objects, 1), Vec2::new(20.0, 0.0));
        assert_eq!(position(&objects, 2), Vec2::new(20.0, 10.0));
        assert_eq!(position(&objects, 4), Vec2::new(10.0, 0.0));

        tool.handle_special_action(state(&objects), SpecialAction::SelectionFlipped(Axis::Horizontal)).unwrap();
        assert_eq!(position(&objects, 0), Vec2::new(40.0, 10.0));
        assert_eq!(position(&objects, 2), Vec2::new(20.0, 0.0));
    }

    /// Rotating turns the selected zones a quarter turn around the center of their bounds, and turning back undoes it
    #[test]
    fn special_rotate() {
        let mut objects = ObjectsHandle::headless();
        add_zone(&mut objects, &[(0.0, 0.0), (20.0, 0.0), (20.0, 10.0)]);
        let mut tool = EditorToolZone::select(&[0], state(&objects));

        // Clockwise on screen, where +y is down: the right side goes to the bottom
        let result = tool.handle_special_action(state(&objects), SpecialAction::SelectionRotated(true)).unwrap();
        assert!(result.normals_stale.is_some());
        assert_near(position(&objects, 0), Vec2::new(15.0, -5.0));
        assert_near(position(&objects, 1), Vec2::new(15.0, 15.0));
        assert_near(position(&objects, 2), Vec2::new(5.0, 15.0));

        tool.handle_special_action(state(&objects), SpecialAction::SelectionRotated(false)).unwrap();
        assert_near(position(&objects, 0), Vec2::new(0.0, 0.0));
        assert_near(position(&objects, 1), Vec2::new(20.0, 0.0));
        assert_near(position(&objects, 2), Vec2::new(20.0, 10.0));
    }

    /// Flipping and rotating do nothing without a selection
    #[test]
    fn special_transform_unselected() {
        let mut objects = ObjectsHandle::headless();
        add_zone(&mut objects, &[(0.0, 0.0), (20.0, 0.0), (20.0, 10.0)]);
        let mut tool = EditorToolZone::init();

        tool.handle_special_action(state(&objects), SpecialAction::SelectionFlipped(Axis::Vertical)).unwrap();
        tool.handle_special_action(state(&objects), SpecialAction::SelectionRotated(true)).unwrap();
        assert_eq!(position(&objects, 1), Vec2::new(20.0, 0.0));
    }

    /// Selecting all selects every zone, whether or not anything was selected before
    #[test]
    fn select_all() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);

        let mut tool = EditorToolZone::init();
        tool.handle_select_all(state(&objects)).unwrap();
        assert_eq!(tool.selected_zones, vec![0, 1]);
        assert_eq!(tool.selection().len(), 7);

        let mut tool = EditorToolZone::select(&[0], state(&objects));
        tool.handle_select_all(state(&objects)).unwrap();
        assert_eq!(tool.selected_zones, vec![0, 1]);
    }

    /// Pasting adds the zones after the others and selects them, with their center where the placement says
    #[test]
    fn paste_placements() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let copied = objects.copy_zones(&[0]).unwrap();
        let mut tool = EditorToolZone::select(&[0], state(&objects));

        // In place
        let result = tool.handle_paste(state(&objects), &copied, PastePlacement::InPlace).unwrap();
        assert_eq!(result.normals_stale, Some((Vec2::ZERO, Vec2::new(10.0, 10.0))));
        assert_eq!(tool.selected_zones, vec![1]);
        assert_eq!(tool.selection(), &[4, 5, 6, 7]);
        assert_eq!(objects.zones_center(&tool.selected_zones).unwrap(), Some(Vec2::new(5.0, 5.0)));

        // At the mouse
        tool.handle_paste(state(&objects), &copied, PastePlacement::At(Vec2::new(50.0, 20.0))).unwrap();
        assert_eq!(tool.selected_zones, vec![2]);
        assert_eq!(objects.get_point_info(8).unwrap().position(), Vec2::new(45.0, 15.0));

        // Offset from where they were copied, scaled to the camera
        tool.handle_paste(state_scaled(&objects, 0.5), &copied, PastePlacement::Offset(None)).unwrap();
        assert_eq!(objects.zones_center(&tool.selected_zones).unwrap(), Some(Vec2::new(10.0, 10.0)));

        // Offset from somewhere else, i.e. the last paste
        tool.handle_paste(state(&objects), &copied, PastePlacement::Offset(Some(Vec2::new(50.0, 20.0)))).unwrap();
        assert_eq!(objects.zones_center(&tool.selected_zones).unwrap(), Some(Vec2::new(60.0, 30.0)));
        assert_eq!(objects.object_counts(), (5, 20));
    }

    /// Pasting stops a drag in progress
    #[test]
    fn paste_abandons_drag() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let copied = objects.copy_zones(&[0]).unwrap();
        let mut tool = EditorToolZone::init();
        tool.handle_drag_start(state(&objects), Vec2::new(50.0, 50.0), IM::none()).unwrap();
        assert!(tool.selection_box().is_some());

        tool.handle_paste(state(&objects), &copied, PastePlacement::InPlace).unwrap();
        assert!(tool.selection_box().is_none());
        assert_eq!(tool.selected_zones, vec![1]);
    }

    /// Get the position of a point
    fn position(objects: &ObjectsHandle, point_id: u16) -> Vec2 {
        objects.get_point_info(point_id).unwrap().position()
    }

    /// Assert two positions are nearly the same, for transforms that aren't exact
    fn assert_near(actual: Vec2, expected: Vec2) {
        assert!(actual.distance(expected) < 1e-4, "{actual} is not {expected}");
    }

    /// Drag the transform box of a selected square from one position to another.
    /// Zoomed in so the square is big enough on screen to have midpoint handles.
    fn transform_square(start: Vec2, end: Vec2, modifiers: IM) -> ObjectsHandle {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let zoomed = |objects: &ObjectsHandle| state_scaled(objects, 0.25);
        let mut tool = EditorToolZone::select(&[0], zoomed(&objects));
        tool.handle_drag_start(zoomed(&objects), start, modifiers).unwrap();
        let result = tool.handle_dragging_to(zoomed(&objects), end, modifiers).unwrap();
        assert!(result.normals_stale.is_some());
        tool.handle_drag_released(zoomed(&objects), modifiers).unwrap();
        objects
    }

    /// Edges too short on screen don't have midpoint handles, so dragging there does what it would without the box
    #[test]
    fn small_box_has_no_midpoints() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 20.0);
        let mut tool = EditorToolZone::select(&[0], state(&objects));

        // Dragging the middle of the right edge moves the square instead of scaling it
        tool.handle_drag_start(state(&objects), Vec2::new(20.0, 10.0), IM::none()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(30.0, 10.0), IM::none()).unwrap();
        assert_eq!(position(&objects, 0), Vec2::new(10.0, 0.0));
        assert_eq!(position(&objects, 2), Vec2::new(30.0, 20.0));
        tool.handle_drag_released(state(&objects), IM::none()).unwrap();

        // Corners still scale
        tool.handle_drag_start(state(&objects), Vec2::new(30.0, 20.0), IM::none()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(40.0, 30.0), IM::none()).unwrap();
        assert_eq!(position(&objects, 0), Vec2::new(10.0, 0.0));
        assert_eq!(position(&objects, 2), Vec2::new(40.0, 30.0));
    }

    /// The transform box isn't shown or used while dragging out a selection box
    #[test]
    fn no_transform_box_while_box_selecting() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolZone::select(&[0], state(&objects));
        assert!(tool.transform_box(&objects).is_some());

        tool.handle_drag_start(state(&objects), Vec2::new(-30.0, -30.0), IM::shift()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(-20.0, -20.0), IM::shift()).unwrap();
        assert!(tool.selection_box().is_some());
        assert_eq!(tool.transform_box(&objects), None);

        tool.handle_drag_released(state(&objects), IM::shift()).unwrap();
        assert!(tool.transform_box(&objects).is_some());
    }

    /// The transform box is around the control points of the selected zones, and only exists if zones are selected
    #[test]
    fn transform_box_bounds() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 4.0)]);

        let tool = EditorToolZone::init();
        assert_eq!(tool.transform_box(&objects), None);

        let tool = EditorToolZone::select(&[0, 4], state(&objects));
        let transform_box = tool.transform_box(&objects).unwrap();
        assert_eq!(transform_box.center, Vec2::new(15.0, 5.0));
        assert_eq!(transform_box.half_size, Vec2::new(15.0, 5.0));
        assert_eq!(transform_box.angle, 0.0);
    }

    /// Dragging an edge scales only that axis, keeping the opposite edge in place
    #[test]
    fn scale_edge() {
        let objects = transform_square(Vec2::new(10.0, 5.0), Vec2::new(20.0, 5.0), IM::none());
        assert_eq!(position(&objects, 0), Vec2::new(0.0, 0.0));
        assert_eq!(position(&objects, 1), Vec2::new(20.0, 0.0));
        assert_eq!(position(&objects, 2), Vec2::new(20.0, 10.0));
        assert_eq!(position(&objects, 3), Vec2::new(0.0, 10.0));

        // The top edge, past the bottom edge, flips the square
        let objects = transform_square(Vec2::new(5.0, 0.0), Vec2::new(5.0, 15.0), IM::none());
        assert_eq!(position(&objects, 0), Vec2::new(0.0, 15.0));
        assert_eq!(position(&objects, 3), Vec2::new(0.0, 10.0));
    }

    /// Dragging a corner scales both axes, keeping the opposite corner in place
    #[test]
    fn scale_corner() {
        let objects = transform_square(Vec2::new(10.0, 10.0), Vec2::new(20.0, 30.0), IM::none());
        assert_eq!(position(&objects, 0), Vec2::new(0.0, 0.0));
        assert_eq!(position(&objects, 1), Vec2::new(20.0, 0.0));
        assert_eq!(position(&objects, 2), Vec2::new(20.0, 30.0));

        // Grabbing the handle off center keeps the same offset from the mouse
        let objects = transform_square(Vec2::new(-1.0, -1.0), Vec2::new(4.0, 4.0), IM::none());
        assert_eq!(position(&objects, 0), Vec2::new(5.0, 5.0));
        assert_eq!(position(&objects, 2), Vec2::new(10.0, 10.0));
    }

    /// Shift scales uniformly, to the largest size that fits where the handle was dragged
    #[test]
    fn scale_uniform() {
        let objects = transform_square(Vec2::new(10.0, 10.0), Vec2::new(20.0, 30.0), IM::shift());
        assert_eq!(position(&objects, 0), Vec2::new(0.0, 0.0));
        assert_eq!(position(&objects, 2), Vec2::new(20.0, 20.0));

        // Dragging into another quadrant flips the square that way
        let objects = transform_square(Vec2::new(10.0, 10.0), Vec2::new(-20.0, 5.0), IM::shift());
        assert_eq!(position(&objects, 2), Vec2::new(-5.0, 5.0));

        // Edges scale the other axis around the center
        let objects = transform_square(Vec2::new(10.0, 5.0), Vec2::new(20.0, 5.0), IM::shift());
        assert_eq!(position(&objects, 1), Vec2::new(20.0, -5.0));
        assert_eq!(position(&objects, 2), Vec2::new(20.0, 15.0));
    }

    /// Alt keeps the center in place, moving the opposite side or corner instead
    #[test]
    fn scale_from_center() {
        let objects = transform_square(Vec2::new(10.0, 10.0), Vec2::new(15.0, 20.0), IM::alt());
        assert_eq!(position(&objects, 0), Vec2::new(-5.0, -10.0));
        assert_eq!(position(&objects, 2), Vec2::new(15.0, 20.0));

        let objects = transform_square(Vec2::new(0.0, 5.0), Vec2::new(-5.0, 5.0), IM::alt());
        assert_eq!(position(&objects, 0), Vec2::new(-5.0, 0.0));
        assert_eq!(position(&objects, 1), Vec2::new(15.0, 0.0));
    }

    /// Ctrl snaps the dragged handle to the grid
    #[test]
    fn scale_snapped() {
        let objects = transform_square(Vec2::new(10.0, 10.0), Vec2::new(14.6, 12.2), IM::ctrl());
        assert_eq!(position(&objects, 0), Vec2::new(0.0, 0.0));
        assert_eq!(position(&objects, 2), Vec2::new(15.0, 12.0));
    }

    /// The scaling modifiers all work together
    #[test]
    fn scale_combined_modifiers() {
        let objects = transform_square(Vec2::new(10.0, 10.0), Vec2::new(14.6, 20.2), IM::all());
        assert_eq!(position(&objects, 0), Vec2::new(-5.0, -5.0));
        assert_eq!(position(&objects, 2), Vec2::new(15.0, 15.0));

        let objects = transform_square(Vec2::new(10.0, 10.0), Vec2::new(14.6, 20.2), IM::shift_ctrl());
        assert_eq!(position(&objects, 0), Vec2::new(0.0, 0.0));
        assert_eq!(position(&objects, 2), Vec2::new(15.0, 15.0));
    }

    /// Handles are scaled along with their points
    #[test]
    fn scale_handles() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        objects.update_point(1, None, Some(crate::ControlPointMode::Broken), Some(Vec2::new(-2.0, 0.0)), Some(Vec2::new(0.0, 3.0))).unwrap();
        let mut tool = EditorToolZone::select(&[0], state(&objects));

        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 10.0), IM::none()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(20.0, 30.0), IM::none()).unwrap();
        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.position(), Vec2::new(20.0, 0.0));
        assert_eq!(point.left_handle(), Vec2::new(16.0, 0.0));
        assert_eq!(point.right_handle(), Vec2::new(20.0, 9.0));
    }

    /// Siblings in other zones move along with the transformed points
    #[test]
    fn transform_moves_siblings() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let (branch_zone, branch_id) = objects.create_branching_zone(1).unwrap();
        objects.create_point(branch_zone, Vec2::new(20.0, 5.0)).unwrap();
        let mut tool = EditorToolZone::select(&[0], state(&objects));
        assert_eq!(tool.selected_zones, vec![0]);

        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 10.0), IM::none()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(20.0, 20.0), IM::none()).unwrap();
        assert_eq!(position(&objects, 1), Vec2::new(20.0, 0.0));
        assert_eq!(position(&objects, branch_id), Vec2::new(20.0, 0.0));
        assert_eq!(position(&objects, branch_id + 1), Vec2::new(20.0, 5.0));
    }

    /// Dragging just outside a corner rotates around the center, and the box rotates along with it until released
    #[test]
    fn rotate() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolZone::select(&[0], state(&objects));

        // Start outside the top right corner, and go a quarter turn around to the bottom right
        tool.handle_drag_start(state(&objects), Vec2::new(17.0, -7.0), IM::none()).unwrap();
        assert!(tool.selection_box().is_none());
        tool.handle_dragging_to(state(&objects), Vec2::new(17.0, 17.0), IM::none()).unwrap();
        assert_near(position(&objects, 0), Vec2::new(10.0, 0.0));
        assert_near(position(&objects, 1), Vec2::new(10.0, 10.0));
        assert_near(position(&objects, 2), Vec2::new(0.0, 10.0));

        let transform_box = tool.transform_box(&objects).unwrap();
        assert_eq!(transform_box.center, Vec2::new(5.0, 5.0));
        assert_eq!(transform_box.half_size, Vec2::new(5.0, 5.0));
        assert!((transform_box.angle - std::f32::consts::FRAC_PI_2).abs() < 1e-5);

        tool.handle_drag_released(state(&objects), IM::none()).unwrap();
        assert_eq!(tool.transform_box(&objects).unwrap().angle, 0.0);
    }

    /// Ctrl snaps rotation to increments of 5 degrees
    #[test]
    fn rotate_snapped() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let mut tool = EditorToolZone::select(&[0], state(&objects));

        // Starting at -45 degrees from the center and going to 2 degrees turns 47 degrees, which snaps to 45
        let angle = 2f32.to_radians();
        tool.handle_drag_start(state(&objects), Vec2::new(17.0, -7.0), IM::ctrl()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(5.0, 5.0) + Vec2::new(angle.cos(), angle.sin()) * 12.0, IM::ctrl()).unwrap();
        assert!((tool.transform_box(&objects).unwrap().angle - 45f32.to_radians()).abs() < 1e-5);
        assert_near(position(&objects, 0), Vec2::new(5.0, 5.0 - 50f32.sqrt()));
    }

    /// Transforming takes priority over dragging other zones or a selection box
    #[test]
    fn transform_priority() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(10.0, 5.0), (20.0, 5.0), (20.0, 15.0)]);
        let mut tool = EditorToolZone::select(&[0], state(&objects));

        // The other zone's point is under the right edge handle, which only exists zoomed in
        let zoomed = state_scaled(&objects, 0.25);
        tool.handle_drag_start(zoomed, Vec2::new(10.0, 5.0), IM::none()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(20.0, 5.0), IM::none()).unwrap();
        assert_eq!(tool.selected_zones, vec![0]);
        assert_eq!(position(&objects, 1), Vec2::new(20.0, 0.0));
        assert_eq!(position(&objects, 4), Vec2::new(10.0, 5.0));
        tool.handle_drag_released(state(&objects), IM::none()).unwrap();

        // Empty space outside a corner rotates rather than starting a selection box
        tool.handle_drag_start(state(&objects), Vec2::new(-7.0, -7.0), IM::none()).unwrap();
        assert!(tool.selection_box().is_none());
        assert_eq!(tool.selected_zones, vec![0]);
    }

    /// Clicking a transform box handle doesn't change the selection, but clicking outside a corner does
    #[test]
    fn click_transform_box() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(17.0, -7.0), (30.0, -7.0), (30.0, -20.0)]);
        let mut tool = EditorToolZone::select(&[0], state(&objects));

        // Even if a zone is under the handle
        tool.handle_click(state(&objects), Vec2::new(10.0, 10.0), IM::alt()).unwrap();
        assert_eq!(tool.selected_zones, vec![0]);
        tool.handle_click(state(&objects), Vec2::new(-2.0, -2.0), IM::none()).unwrap();
        assert_eq!(tool.selected_zones, vec![0]);

        // Outside a corner selects a zone there, or deselects if there is none
        tool.handle_click(state(&objects), Vec2::new(17.0, -7.0), IM::none()).unwrap();
        assert_eq!(tool.selected_zones, vec![1]);
        tool.handle_click(state(&objects), Vec2::new(-7.0, -7.0), IM::none()).unwrap();
        assert!(tool.selected_zones.is_empty());
    }

    /// Cancelling a transform puts everything back and keeps the selection
    #[test]
    fn cancel_transform() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        objects.update_point(1, None, Some(crate::ControlPointMode::Broken), Some(Vec2::new(-2.0, 0.0)), Some(Vec2::new(0.0, 3.0))).unwrap();
        let mut tool = EditorToolZone::select(&[0], state(&objects));

        tool.handle_drag_start(state(&objects), Vec2::new(17.0, -7.0), IM::none()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(17.0, 17.0), IM::none()).unwrap();
        let result = tool.handle_cancel(state(&objects)).unwrap();
        assert!(result.normals_stale.is_some());
        assert_eq!(position(&objects, 1), Vec2::new(10.0, 0.0));
        assert_eq!(objects.get_point_info(1).unwrap().right_handle(), Vec2::new(10.0, 3.0));
        assert_eq!(tool.selected_zones, vec![0]);
        assert_eq!(tool.transform_box(&objects).unwrap().angle, 0.0);

        // The drag is over
        tool.handle_dragging_to(state(&objects), Vec2::new(50.0, 50.0), IM::none()).unwrap();
        assert_eq!(position(&objects, 1), Vec2::new(10.0, 0.0));
    }

    /// A 10 pixel square with a point shape in it at (3, 4), returning the vertex
    fn square_with_shape(objects: &mut ObjectsHandle) -> u16 {
        add_square(objects, 10.0);
        let vertex = objects.add_shape_vertex(0, Vec2::new(3.0, 4.0)).unwrap();
        objects.create_shape(0, &[vertex], crate::objects::ShapeParams::default(), 0).unwrap();
        vertex
    }

    /// Dragging a zone moves its shapes along with it, and cancelling puts them back
    #[test]
    fn drag_moves_shapes() {
        let mut objects = ObjectsHandle::headless();
        let vertex = square_with_shape(&mut objects);
        let mut tool = EditorToolZone::init();

        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 10.0), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(15.0, 12.0), Default::default()).unwrap();
        assert_eq!(objects.get_vertex_info(vertex).unwrap().position(), Vec2::new(8.0, 6.0));

        tool.handle_cancel(state(&objects)).unwrap();
        assert_eq!(objects.get_vertex_info(vertex).unwrap().position(), Vec2::new(3.0, 4.0));
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(10.0, 10.0));
    }

    /// Pasted zones bring their shapes to where they are placed
    #[test]
    fn paste_moves_shapes() {
        let mut objects = ObjectsHandle::headless();
        square_with_shape(&mut objects);
        let copied = objects.copy_zones(&[0]).unwrap();
        let mut tool = EditorToolZone::select(&[0], state(&objects));

        tool.handle_paste(state(&objects), &copied, PastePlacement::At(Vec2::new(50.0, 20.0))).unwrap();
        assert_eq!(objects.shape_positions(1).unwrap(), vec![Vec2::new(48.0, 19.0)]);
        assert_eq!(objects.shape_positions(0).unwrap(), vec![Vec2::new(3.0, 4.0)]);
    }

    /// Flipping and scaling zones transforms their shapes, scaling radii with the zones
    #[test]
    fn transform_moves_shapes() {
        let mut objects = ObjectsHandle::headless();
        let vertex = square_with_shape(&mut objects);
        let mut tool = EditorToolZone::select(&[0], state(&objects));

        tool.handle_special_action(state(&objects), SpecialAction::SelectionFlipped(Axis::Vertical)).unwrap();
        assert_eq!(objects.get_vertex_info(vertex).unwrap().position(), Vec2::new(7.0, 4.0));

        // Dragging the bottom right corner out to (30, 30) triples the zone
        tool.handle_drag_start(state(&objects), Vec2::new(10.0, 10.0), Default::default()).unwrap();
        tool.handle_dragging_to(state(&objects), Vec2::new(30.0, 30.0), Default::default()).unwrap();
        assert_near(objects.get_vertex_info(vertex).unwrap().position(), Vec2::new(21.0, 12.0));
        assert!((objects.get_shape_info(0).unwrap().params().outer_radius - 30.0).abs() < 1e-4);
    }

    /// Clearing shapes removes them from the selected zones only
    #[test]
    fn clear_shapes() {
        let mut objects = ObjectsHandle::headless();
        square_with_shape(&mut objects);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);
        let vertex = objects.add_shape_vertex(1, Vec2::new(28.0, 2.0)).unwrap();
        objects.create_shape(1, &[vertex], crate::objects::ShapeParams::default(), 0).unwrap();

        let mut tool = EditorToolZone::select(&[0], state(&objects));
        let result = tool.handle_special_action(state(&objects), SpecialAction::ShapesCleared).unwrap();
        assert!(result.normals_stale.is_some());
        assert_eq!((objects.shape_count(), objects.vertex_count()), (1, 1));
        assert_eq!(objects.get_shape_info(0).unwrap().zone_id(), 1);
    }

    /// Paste centers on the center of the bounding box, not the average of the points
    #[test]
    fn paste_uses_bounds_center() {
        let mut objects = ObjectsHandle::headless();
        add_zone(&mut objects, &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (9.0, 10.0)]);
        let copied = objects.copy_zones(&[0]).unwrap();
        let mut tool = EditorToolZone::init();

        tool.handle_paste(state(&objects), &copied, PastePlacement::At(Vec2::new(50.0, 50.0))).unwrap();
        assert_eq!(objects.get_point_info(4).unwrap().position(), Vec2::new(45.0, 45.0));
    }

    /// Nudging moves the selected zones and their shapes by the delta, but not in the middle of a drag
    #[test]
    fn special_nudge() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let vertex = objects.add_shape_vertex(0, Vec2::new(5.0, 5.0)).unwrap();
        objects.create_shape(0, &[vertex], crate::objects::ShapeParams::default(), 0).unwrap();
        let mut tool = EditorToolZone::select(&[0], state(&objects));

        let result = tool.handle_special_action(state(&objects), SpecialAction::SelectionNudged(Vec2::new(16.0, -2.0), false)).unwrap();
        assert!(result.normals_stale.is_some());
        assert_eq!(position(&objects, 2), Vec2::new(26.0, 8.0));
        assert_eq!(objects.get_vertex_info(vertex).unwrap().position(), Vec2::new(21.0, 3.0));
        assert_eq!(tool.selection().len(), 4);

        tool.handle_drag_start(state(&objects), Vec2::new(26.0, 8.0), Default::default()).unwrap();
        tool.handle_special_action(state(&objects), SpecialAction::SelectionNudged(Vec2::new(16.0, 0.0), true)).unwrap();
        assert_eq!(position(&objects, 2), Vec2::new(26.0, 8.0));
    }

    /// Moving forward and backward hops past the next overlapping zone, skipping ones that don't overlap, and keeps the moved zone selected
    #[test]
    fn special_reorder() {
        use crate::objects::ZoneOrderChange as Order;
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(50.0, 50.0), (60.0, 50.0), (60.0, 60.0)]);
        add_zone(&mut objects, &[(5.0, 5.0), (15.0, 5.0), (15.0, 15.0), (5.0, 15.0)]);
        let mut tool = EditorToolZone::select(&[0], state(&objects));
        let first_of = |objects: &ObjectsHandle, zone_id: u16| position(objects, objects.get_zone_info(zone_id).unwrap().range().0);

        // The square hops over the triangle, which doesn't overlap it, to in front of the other square
        let result = tool.handle_special_action(state(&objects), SpecialAction::ZonesReordered(Order::Forward)).unwrap();
        assert_eq!(result.normals_stale, Some((Vec2::ZERO, Vec2::new(10.0, 10.0))));
        assert_eq!(first_of(&objects, 2), Vec2::ZERO);
        assert_eq!(first_of(&objects, 0), Vec2::new(50.0, 50.0));
        assert_eq!(tool.selection(), &[7, 8, 9, 10]);

        // Nothing is in front of it now, so it can't go further, and nothing changes
        let result = tool.handle_special_action(state(&objects), SpecialAction::ZonesReordered(Order::Forward)).unwrap();
        assert!(result.normals_stale.is_none());
        assert_eq!(first_of(&objects, 2), Vec2::ZERO);

        // Back behind the other square, then all the way to the back
        tool.handle_special_action(state(&objects), SpecialAction::ZonesReordered(Order::Backward)).unwrap();
        assert_eq!(first_of(&objects, 1), Vec2::ZERO);
        tool.handle_special_action(state(&objects), SpecialAction::ZonesReordered(Order::ToBack)).unwrap();
        assert_eq!(first_of(&objects, 0), Vec2::ZERO);
        tool.handle_special_action(state(&objects), SpecialAction::ZonesReordered(Order::ToFront)).unwrap();
        assert_eq!(first_of(&objects, 2), Vec2::ZERO);
        assert_eq!(tool.selection().len(), 4);
    }
}
