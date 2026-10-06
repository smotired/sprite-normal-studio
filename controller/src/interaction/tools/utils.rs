use studio_math::Vec2;
use crate::interaction::tools::ControllerStateInput;
use crate::objects::ControlPointMode;

const CLICK_TOLERANCE: f32 = 6.0; // since of control point boxes in the overlay plus 3 pixels

/// Get the closest control point to a click position, if it's in the click range corrected for camera scale.
pub fn get_clicked_control_point(state: &ControllerStateInput, zone_id: Option<u16>, pos: Vec2) -> Option<(u16, u16)> {
    if let Some((zone_id, point_id)) = state.objects.get_closest_point(zone_id, pos) {
        let distance = state.objects.get_point_info(point_id).unwrap().absolute_axis_distance(pos);
        if distance <= CLICK_TOLERANCE * state.camera_inv_scale { // size of control point boxes in the overlay, plus 3 pixels
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
    if point.left_handle().distance(pos) <= CLICK_TOLERANCE * state.camera_inv_scale {
        return Some(false); // left handle clicked
    }

    // Check the right handle
    if point.right_handle().distance(pos) <= CLICK_TOLERANCE * state.camera_inv_scale {
        return Some(true); // right handle clicked
    }

    None
}

/// Get the ID of the zone path that was clicked, if applicable.
/// Returns the ID of the zone path, and the ID of the point that STARTS the clicked path segment.
/// Also returns the point corrected to the path and the t value of that curve segment.
/// If inserting a point into the zone path, it would go after the returned point.
pub fn get_clicked_zone_path(state: &ControllerStateInput, pos: Vec2) -> Option<(u16, u16, Vec2, f32)> {
    if let Some((zone_id, point_id, corrected, t)) = state.objects.get_closest_path_point(pos, state.camera_inv_scale)
        && corrected.distance(pos) <= CLICK_TOLERANCE * state.camera_inv_scale {
            return Some((zone_id, point_id, corrected, t));
        }
    None
}


#[cfg(test)]
mod tests {
    use crate::ObjectBuffers;
    use crate::objects::test_utils::add_square;

    use super::*;
    use super::super::test_utils::{state, state_scaled};

    /// Clicking near a control point finds it, and clicking far from everything does not
    #[test]
    fn clicked_control_point() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);

        assert_eq!(get_clicked_control_point(&state(&objects), None, Vec2::new(11.0, 1.0)), Some((0, 1)));
        assert_eq!(get_clicked_control_point(&state(&objects), None, Vec2::new(30.0, 30.0)), None);
    }

    /// The click tolerance is in screen pixels, so it grows when zoomed out
    #[test]
    fn clicked_control_point_scales_with_camera() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let click = Vec2::new(10.0, 20.0);

        assert_eq!(get_clicked_control_point(&state(&objects), None, click), None);
        assert_eq!(get_clicked_control_point(&state_scaled(&objects, 4.0), None, click), Some((0, 2)));
    }

    /// Handles can be clicked on broken points, but not on linear points
    #[test]
    fn clicked_handle() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        assert_eq!(get_clicked_handle(1, &state(&objects), Vec2::new(10.0, 0.0)), None);

        objects.update_point(1, None, Some(ControlPointMode::Broken), Some(Vec2::new(-20.0, 0.0)), Some(Vec2::new(0.0, 30.0))).unwrap();
        assert_eq!(get_clicked_handle(1, &state(&objects), Vec2::new(-10.0, 1.0)), Some(false));
        assert_eq!(get_clicked_handle(1, &state(&objects), Vec2::new(10.0, 31.0)), Some(true));
        assert_eq!(get_clicked_handle(1, &state(&objects), Vec2::new(50.0, 50.0)), None);
    }

    /// Clicking near a path finds the zone, the start of the segment, and the position on the path
    #[test]
    fn clicked_zone_path() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);

        let (zone_id, point_id, corrected, _) = get_clicked_zone_path(&state(&objects), Vec2::new(5.0, -2.0)).unwrap();
        assert_eq!((zone_id, point_id), (0, 0));
        assert!(corrected.distance(Vec2::new(5.0, 0.0)) < 1e-3);
        assert!(get_clicked_zone_path(&state(&objects), Vec2::new(5.0, -20.0)).is_none());
    }
}
