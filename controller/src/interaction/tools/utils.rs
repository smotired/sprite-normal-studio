use studio_math::Vec2;
use crate::InputModifiers;
use crate::interaction::tools::ControllerStateInput;
use crate::objects::ControlPointMode;

pub const CLICK_TOLERANCE: f32 = 6.0; // since of control point boxes in the overlay plus 3 pixels

/// Shortest edge of a transform box, in screen pixels, that has a midpoint handle.
/// Shorter edges would have the midpoint handle overlapping the corners. Must match TRANSFORM_MIN_MIDPOINT_EDGE in overlay.wgsl.
pub const MIN_MIDPOINT_EDGE: f32 = 4.0 * CLICK_TOLERANCE;

/// How far outside the corners of a transform box, in screen pixels, dragging rotates instead of scaling
pub const ROTATE_TOLERANCE: f32 = 15.0;

/// Increment in radians that rotation snaps to while holding ctrl
pub const ROTATE_SNAP: f32 = 5.0 * std::f32::consts::PI / 180.0;

/// tan(22.5 degrees), the slope that splits orthagonal from diagonal when snapping directions.
const TAN_HALF_OCTANT: f32 = 0.414_213_56;

/// Minimum length in world space of a handle set by the user.
/// Zero-length handles are reserved for linear points and sibling joins.
const MIN_HANDLE_LENGTH: f32 = 2.0;

/// Make sure a handle (relative to its point) set by the user is at least MIN_HANDLE_LENGTH long, by pushing it out along its own direction.
/// If it has no direction it is pushed out along `fallback` instead, or to the right if that has no direction either.
pub fn enforce_handle_length(handle: Vec2, fallback: Vec2) -> Vec2 {
    if handle.magnitude() >= MIN_HANDLE_LENGTH { return handle; }
    let direction = [handle, fallback, Vec2::hz(1.0)].into_iter()
        .map(Vec2::normalized)
        .find(|&direction| direction != Vec2::ZERO)
        .unwrap();
    direction * MIN_HANDLE_LENGTH
}

/// Apply the drag modifiers to the target position of whatever is being dragged.
/// `start` is where the target was when the drag began, and `origin` is where the mouse was.
/// SHIFT: snap movement to horizontal, vertical, or diagonal, whichever is closest to the direction moved
/// CTRL: snap to a grid of `grid` pixels
pub fn snap_drag_target(mut target: Vec2, start: Vec2, origin: Vec2, pos: Vec2, modifiers: InputModifiers, grid: f32) -> Vec2 {
    // Shift: only move orthagonally/diagonally
    if modifiers.shift {
        let delta = pos - origin;
        let (abs_x, abs_y) = (delta.x.abs(), delta.y.abs());

        // Snap delta to whichever of 8 directions is correct
        let snapped_delta = if abs_y < abs_x * TAN_HALF_OCTANT {
            Vec2::new(delta.x, 0.0)
        } else if abs_x < abs_y * TAN_HALF_OCTANT {
            Vec2::new(0.0, delta.y)
        } else {
            let diagonal = (abs_x + abs_y) / 2.0;
            Vec2::new(diagonal.copysign(delta.x), diagonal.copysign(delta.y))
        };
        target = start + snapped_delta;
    }

    // Ctrl: snap to the grid
    if modifiers.ctrl {
        target = snap_to_grid(target, grid);
    }
    target
}

/// Round a position to the nearest point on a grid of `grid` pixels
pub fn snap_to_grid(pos: Vec2, grid: f32) -> Vec2 {
    Vec2::new((pos.x / grid).round() * grid, (pos.y / grid).round() * grid)
}

/// Combine a unit of IDs with the current selection, based on modifier keys.
/// SHIFT adds, ALT removes, SHIFT+ALT toggles, and no modifiers replaces the selection.
/// If `toggle_all` is true, toggling removes the unit only if all of it is selected, and adds all of it otherwise.
/// If it is false, every ID in the unit is toggled on its own.
/// Returns the new selection, sorted.
pub fn combine_selection(selected: &[u16], unit: &[u16], modifiers: InputModifiers, toggle_all: bool) -> Vec<u16> {
    let mut selected: std::collections::HashSet<u16> = selected.iter().copied().collect();
    match (modifiers.shift, modifiers.alt) {
        (true, true) => if toggle_all {
            if unit.iter().all(|id| selected.contains(id)) {
                for id in unit { selected.remove(id); }
            } else {
                selected.extend(unit);
            }
        } else {
            for &id in unit {
                if !selected.insert(id) { selected.remove(&id); } // if it was already present, remove it
            }
        },
        (true, false) => selected.extend(unit),
        (false, true) => for id in unit { selected.remove(id); },
        (false, false) => selected = unit.iter().copied().collect(),
    }

    let mut selected: Vec<u16> = selected.into_iter().collect();
    selected.sort();
    selected
}

/// Get the closest control point to a click position, if it's in the click range corrected for camera scale.
pub fn get_clicked_control_point(state: &ControllerStateInput, zone_id: Option<u16>, pos: Vec2) -> anyhow::Result<Option<(u16, u16)>> {
    if let Some((zone_id, point_id)) = state.objects.get_closest_point(zone_id, pos)? {
        let distance = state.objects.get_point_info(point_id)?.absolute_axis_distance(pos);
        if distance <= CLICK_TOLERANCE * state.camera_inv_scale { // size of control point boxes in the overlay, plus 3 pixels
            return Ok(Some((zone_id, point_id)));
        }
    }
    Ok(None)
}

/// Get the handle that was clicked for the selected control point, if applicable.
/// Returns true if the right handle was clicked, and None if no handle was clicked.
/// Zero-length handles are on top of the point itself, so they can't be clicked and the point is clicked instead.
pub fn get_clicked_handle(point_id: u16, state: &ControllerStateInput, pos: Vec2) -> anyhow::Result<Option<bool>> {
    let point = state.objects.get_point_info(point_id)?;
    if let ControlPointMode::Linear = point.mode() { return Ok(None); }

    // Check the left handle
    if point.left_handle() != point.position() && point.left_handle().distance(pos) <= CLICK_TOLERANCE * state.camera_inv_scale {
        return Ok(Some(false)); // left handle clicked
    }

    // Check the right handle
    if point.right_handle() != point.position() && point.right_handle().distance(pos) <= CLICK_TOLERANCE * state.camera_inv_scale {
        return Ok(Some(true)); // right handle clicked
    }

    Ok(None)
}

/// Get the ID of the zone path that was clicked, if applicable.
/// Returns the ID of the zone path, and the ID of the point that STARTS the clicked path segment.
/// Also returns the point corrected to the path and the t value of that curve segment.
/// If inserting a point into the zone path, it would go after the returned point.
pub fn get_clicked_zone_path(state: &ControllerStateInput, pos: Vec2) -> anyhow::Result<Option<(u16, u16, Vec2, f32)>> {
    if let Some((zone_id, point_id, corrected, t)) = state.objects.get_closest_path_point(pos, state.camera_inv_scale)?
        && corrected.distance(pos) <= CLICK_TOLERANCE * state.camera_inv_scale {
            return Ok(Some((zone_id, point_id, corrected, t)));
        }
    Ok(None)
}


#[cfg(test)]
mod tests {
    use crate::ObjectsHandle;
    use crate::objects::test_utils::add_square;

    use super::*;
    use super::super::test_utils::{state, state_scaled};

    /// Clicking near a control point finds it, and clicking far from everything does not
    #[test]
    fn clicked_control_point() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);

        assert_eq!(get_clicked_control_point(&state(&objects), None, Vec2::new(11.0, 1.0)).unwrap(), Some((0, 1)));
        assert_eq!(get_clicked_control_point(&state(&objects), None, Vec2::new(30.0, 30.0)).unwrap(), None);
    }

    /// The click tolerance is in screen pixels, so it grows when zoomed out
    #[test]
    fn clicked_control_point_scales_with_camera() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let click = Vec2::new(10.0, 20.0);

        assert_eq!(get_clicked_control_point(&state(&objects), None, click).unwrap(), None);
        assert_eq!(get_clicked_control_point(&state_scaled(&objects, 4.0), None, click).unwrap(), Some((0, 2)));
    }

    /// Handles can be clicked on broken points, but not on linear points
    #[test]
    fn clicked_handle() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        assert_eq!(get_clicked_handle(1, &state(&objects), Vec2::new(10.0, 0.0)).unwrap(), None);

        objects.update_point(1, None, Some(ControlPointMode::Broken), Some(Vec2::new(-20.0, 0.0)), Some(Vec2::new(0.0, 30.0))).unwrap();
        assert_eq!(get_clicked_handle(1, &state(&objects), Vec2::new(-10.0, 1.0)).unwrap(), Some(false));
        assert_eq!(get_clicked_handle(1, &state(&objects), Vec2::new(10.0, 31.0)).unwrap(), Some(true));
        assert_eq!(get_clicked_handle(1, &state(&objects), Vec2::new(50.0, 50.0)).unwrap(), None);
    }

    /// Zero-length handles sit on top of their point, so they can't be clicked
    #[test]
    fn clicked_handle_zero_length() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        objects.update_point(1, None, Some(ControlPointMode::Broken), Some(Vec2::ZERO), Some(Vec2::new(0.0, 30.0))).unwrap();
        assert_eq!(get_clicked_handle(1, &state(&objects), Vec2::new(10.0, 0.0)).unwrap(), None);
        assert_eq!(get_clicked_handle(1, &state(&objects), Vec2::new(10.0, 30.0)).unwrap(), Some(true));
    }

    /// Handles shorter than the minimum are pushed out along their own direction, or the fallback if they have none
    #[test]
    fn enforce_handle_length_minimum() {
        assert_eq!(enforce_handle_length(Vec2::new(0.0, 5.0), Vec2::ZERO), Vec2::new(0.0, 5.0));
        assert_eq!(enforce_handle_length(Vec2::new(0.0, -0.5), Vec2::hz(1.0)), Vec2::new(0.0, -2.0));
        assert_eq!(enforce_handle_length(Vec2::ZERO, Vec2::new(-3.0, 0.0)), Vec2::new(-2.0, 0.0));
        assert_eq!(enforce_handle_length(Vec2::ZERO, Vec2::ZERO), Vec2::new(2.0, 0.0));
    }

    /// Clicking near a path finds the zone, the start of the segment, and the position on the path
    #[test]
    fn clicked_zone_path() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);

        let (zone_id, point_id, corrected, _) = get_clicked_zone_path(&state(&objects), Vec2::new(5.0, -2.0)).unwrap().unwrap();
        assert_eq!((zone_id, point_id), (0, 0));
        assert!(corrected.distance(Vec2::new(5.0, 0.0)) < 1e-3);
        assert!(get_clicked_zone_path(&state(&objects), Vec2::new(5.0, -20.0)).unwrap().is_none());
    }

    /// Positions snap to the nearest point on a grid of the given size
    #[test]
    fn snap_to_grid_sizes() {
        assert_eq!(snap_to_grid(Vec2::new(5.4, 6.6), 1.0), Vec2::new(5.0, 7.0));
        assert_eq!(snap_to_grid(Vec2::new(5.0, 6.1), 4.0), Vec2::new(4.0, 8.0));
        assert_eq!(snap_to_grid(Vec2::new(-9.0, 25.0), 16.0), Vec2::new(-16.0, 32.0));
    }

    /// Ctrl snaps a drag target to the grid it is given
    #[test]
    fn snap_drag_target_grid() {
        let target = snap_drag_target(Vec2::new(13.0, 2.0), Vec2::ZERO, Vec2::ZERO, Vec2::new(13.0, 2.0), InputModifiers::ctrl(), 8.0);
        assert_eq!(target, Vec2::new(16.0, 0.0));
    }
}
