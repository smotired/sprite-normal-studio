use studio_math::Vec2;
use crate::interaction::tools::ControllerStateInput;
use crate::objects::ControlPointMode;

/// Get the closest control point to a click position, if it's in the click range corrected for camera scale.
pub fn get_clicked_control_point(state: &ControllerStateInput, zone_id: Option<u16>, pos: Vec2) -> Option<(u16, u16)> {
    if let Some((zone_id, point_id)) = state.objects.get_closest_point(zone_id, pos) {
        let distance = state.objects.get_point_info(point_id).unwrap().absolute_axis_distance(pos);
        if distance <= 6.0 * state.camera_inv_scale { // size of control point boxes in the overlay, plus 3 pixels
            return Some((zone_id, point_id));
        }
    }
    None
}

/// Get the handle that was clicked for the selected control point, if applicable.
/// Returns true if the right handle was clicked, and None if no handle was clicked.
pub fn get_clicked_handle(point_id: u16, state: &ControllerStateInput, pos: Vec2) -> Option<bool> {
    let point = state.objects.get_point_info(point_id).unwrap();
    if let ControlPointMode::Linear = point.mode() { return None; }

    // Check the left handle
    if point.left_handle().distance(pos) <= 6.0 * state.camera_inv_scale {
        return Some(false); // left handle clicked
    }

    // Check the right handle
    if point.right_handle().distance(pos) <= 6.0 * state.camera_inv_scale {
        return Some(true); // right handle clicked
    }

    None
}

/// Get the ID of the zone path that was clicked, if applicable.
/// Returns the ID of the zone path, and the ID of the point that STARTS the clicked path segment.
/// Also returns the point corrected to the path and the t value of that curve segment.
/// If inserting a point into the zone path, it would go after the returned point.
pub fn get_clicked_zone_path(state: &ControllerStateInput, pos: Vec2) -> Option<(u16, u16, Vec2, f32)> {
    if let Some((zone_id, point_id, corrected, t)) = state.objects.get_closest_path_point(pos, state.camera_inv_scale) {
        if corrected.distance(pos) <= 6.0 * state.camera_inv_scale {
            return Some((zone_id, point_id, corrected, t));
        }
    }
    None
}