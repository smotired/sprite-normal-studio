use studio_math::Vec2;

use super::ObjectBuffers;

impl ObjectBuffers {
    /// Get the closest point to the given position within the specified zone.
    /// Returns the ID of the closest control point to the given position within the specified zone, or None if no points exist.
    /// If no zone is specified, returns the closest point across all zones.
    /// Returns a tuple containing the zone ID and the point ID of the closest control point, or None if no points exist.
    pub fn get_closest_point(&self, zone_id: Option<u16>, position: Vec2) -> Option<(u16, u16)> {
        // Get the range of points to search for
        let (start, count) = if let Some(zone_id) = zone_id {
            self.get_zone_info(zone_id)?.range()
        } else {
            (0, self.point_count())
        };
        if count == 0 { return None; }

        // Find the closest point within the specified range
        let mut closest_id = None;
        let mut min_distance = f32::MAX;
        for i in 0..count {
            let distance = self.get_point_info(start + i).unwrap().distance(position);
            if distance < min_distance {
                min_distance = distance;
                closest_id = Some(start + i);
            }
        }
        
        closest_id.map(|point_id| (self.get_point_info(point_id).unwrap().zone_id(), point_id))
    }

    /// Get the closest point on any path to the given position.
    /// Returns the ID of the zone path, and the ID of the point that STARTS the path segment.
    /// Also returns the corrected position on the path, and the t value for that segment
    /// If inserting a point into the zone path, it would go after the returned point.
    pub fn get_closest_path_point(&self, position: Vec2, scale: f32) -> Option<(u16, u16, Vec2, f32)> {
        let mut best = None as Option<(u16, u16)>;
        let mut corrected = Vec2::ZERO;
        let mut t = 0.5;
        let mut min_path_dist = f32::MAX;
        for i in 0..self.zone_count() {
            // Get the amount of points in this zone
            let (start, count) = self.get_zone_info(i).unwrap().range();
            if count == 0 { panic!("Zone {} has zero points!", i); } // should never hit. could just continue but i want to catch 0-len zones

            // Check each point to the previous point on the path
            let mut start_point = self.get_point_info(start + count - 1).unwrap();
            let mut start_id = start + count - 1;
            let mut best_start_id = start_id;
            let mut min_point_dist = f32::MAX;
            let mut corrected_to_path= Vec2::ZERO;
            let mut path_t = 0.5;
            for j in 0..count {
                let end_id = start + j;
                let end_point = self.get_point_info(end_id).unwrap();
                let (distance, corrected_to_curve, curve_t) = correct_to_bezier(
                    position, 
                    start_point.position(), start_point.right_handle(),
                    end_point.left_handle(), end_point.position(),
                    scale
                );

                if distance < min_point_dist {
                    min_point_dist = distance;
                    corrected_to_path = corrected_to_curve;
                    path_t = curve_t;
                    best_start_id = start_id;
                }

                start_point = end_point;
                start_id = end_id;
            }

            // Check if this is the closest path
            if min_point_dist < min_path_dist {
                min_path_dist = min_point_dist;
                corrected = corrected_to_path;
                t = path_t;
                best = Some((i, best_start_id));
            }
        }

        // Fold in corrected position
        best.map(|(z, p)| (z, p, corrected, t))
    }
}

/// Return scalar distance to a line segment, the closest point on that line segment, and t for that point.
fn correct_to_line_segment(pos: Vec2, pos0: Vec2, pos1: Vec2) -> (f32, Vec2, f32) {
    // Check endpoints
    let relative = pos - pos0;
    let line_dir = (pos1 - pos0).normalized();
    let length = pos0.distance(pos1);
    let t = if length == 0.0 { 0.0 } else { relative.dot(line_dir) / length };

    // If it's past an endpoint, distance = distance to endpoint
    if t < 0.0 { (relative.magnitude(), pos0, 0.0) }
    else if t > 1.0 { (pos.distance(pos1), pos1, 1.0) }

    // If it can be projected onto the line segment,
    // distance = magnitude of projection onto orthag. vector
    else { (relative.dot(line_dir.right()).abs(), pos0 + t * length * line_dir, t) }
}

/// Return scalar distance to a bezier curve based on shortest distance to a line segment.
fn correct_to_bezier(pos: Vec2, pos0: Vec2, pos1: Vec2, pos2: Vec2, pos3: Vec2, scale: f32) -> (f32, Vec2, f32) {
    // Split the curve into individual lines with de Casteljau's method.
    // Determine segment count from curvature. For a cubic, deviation is at most M / 8n^2.
    // Chord error = 0.25px means largest distance from polyline to curve is at most 0.25px.
    // M is the largest magnitude of d^2B(t)/dt^2 where B is the curve. Acceleration/tightness.
    let tightness = {
        let a = (pos0 - 2.0 * pos1 + pos2).magnitude();
        let b = (pos1 - 2.0 * pos2 + pos3).magnitude();
        6.0 * a.max(b)
    };
    let tolerance = 0.25 * scale;
    let segment_count = {
        let adjusted = tightness / (8.0 * tolerance);
        let true_count = adjusted.sqrt().ceil() as u32;
        true_count.clamp(1, 64)
    };

    // Draw line segments between each point and its previous point
    let mut last_point = pos0;
    let mut min_dist = (pos - pos0).magnitude(); // minimum distance to the curve
    let mut corrected = pos0;
    let mut path_t = 0.0;
    let segment_length = 1.0 / segment_count as f32;
    for i in 1..=segment_count {
        let t = i as f32 * segment_length;
        let point = studio_math::bezier::bezier_point_at(pos0, pos1, pos2, pos3, t);

        let (distance, corrected_to_line, corrected_t) = correct_to_line_segment(pos, last_point, point);
        if distance < min_dist {
            min_dist = distance;
            corrected = corrected_to_line;
            path_t = t - segment_length + corrected_t * segment_length;
        }

        last_point = point;
    }

    (min_dist, corrected, path_t)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn assert_close(a: Vec2, b: Vec2) {
        assert!(a.distance(b) < 1e-3, "{a} is not close to {b}");
    }

    /// A point beside a segment projects onto it
    #[test]
    fn line_segment_projects_onto_interior() {
        let (distance, corrected, t) = correct_to_line_segment(Vec2::new(5.0, 3.0), Vec2::ZERO, Vec2::new(10.0, 0.0));
        assert!((distance - 3.0).abs() < 1e-5);
        assert_close(corrected, Vec2::new(5.0, 0.0));
        assert!((t - 0.5).abs() < 1e-5);
    }

    /// A point past either end of a segment snaps to that end
    #[test]
    fn line_segment_snaps_to_endpoints() {
        let (pos0, pos1) = (Vec2::ZERO, Vec2::new(10.0, 0.0));

        let (distance, corrected, t) = correct_to_line_segment(Vec2::new(-3.0, 4.0), pos0, pos1);
        assert!((distance - 5.0).abs() < 1e-5);
        assert_close(corrected, pos0);
        assert_eq!(t, 0.0);

        let (distance, corrected, t) = correct_to_line_segment(Vec2::new(13.0, 4.0), pos0, pos1);
        assert!((distance - 5.0).abs() < 1e-5);
        assert_close(corrected, pos1);
        assert_eq!(t, 1.0);
    }

    /// A point on a straight curve corrects to itself with the matching t
    #[test]
    fn bezier_straight_curve() {
        let (pos0, pos1, pos2, pos3) = (Vec2::ZERO, Vec2::hz(10.0), Vec2::hz(20.0), Vec2::hz(30.0));
        let (distance, corrected, t) = correct_to_bezier(Vec2::new(15.0, 2.0), pos0, pos1, pos2, pos3, 1.0);
        assert!((distance - 2.0).abs() < 1e-3);
        assert_close(corrected, Vec2::new(15.0, 0.0));
        assert!((t - 0.5).abs() < 1e-3);
    }

    /// A point on a curved path should be found on the curve at the right t
    #[test]
    fn bezier_curved_path() {
        let (pos0, pos1, pos2, pos3) = (Vec2::ZERO, Vec2::new(0.0, 40.0), Vec2::new(40.0, 40.0), Vec2::new(40.0, 0.0));
        for t in [0.2, 0.5, 0.8] {
            let on_curve = studio_math::bezier::bezier_point_at(pos0, pos1, pos2, pos3, t);
            let (distance, corrected, found_t) = correct_to_bezier(on_curve, pos0, pos1, pos2, pos3, 1.0);
            assert!(distance < 0.5, "distance {distance} at t = {t}");
            assert!(corrected.distance(on_curve) < 0.5);
            assert!((found_t - t).abs() < 0.05, "found t {found_t} for t = {t}");
        }
    }

    /// Points far from a curve report their distance from the closest end
    #[test]
    fn bezier_far_point() {
        let (pos0, pos1, pos2, pos3) = (Vec2::ZERO, Vec2::hz(10.0), Vec2::hz(20.0), Vec2::hz(30.0));
        let (distance, corrected, _) = correct_to_bezier(Vec2::new(-4.0, 3.0), pos0, pos1, pos2, pos3, 1.0);
        assert!((distance - 5.0).abs() < 1e-3);
        assert_close(corrected, pos0);
    }
}
