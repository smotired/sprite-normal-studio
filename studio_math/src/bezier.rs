use crate::Vec2;

/// Use de Castlejau's method to find six reference control points
/// for t on a cubic bezier curve. Returns in the order of interpolating.
pub fn bezier_de_casteljau(pos0: Vec2, pos1: Vec2, pos2: Vec2, pos3: Vec2, t: f32) -> (Vec2, Vec2, Vec2, Vec2, Vec2, Vec2) {
    let a0 = Vec2::lerp(pos0, pos1, t);
    let a1 = Vec2::lerp(pos1, pos2, t);
    let a2 = Vec2::lerp(pos2, pos3, t);

    let b0 = Vec2::lerp(a0, a1, t);
    let b1 = Vec2::lerp(a1, a2, t);

    let c0 = Vec2::lerp(b0, b1, t);

    (a0, a1, a2, b0, b1, c0)
}

/// Find the point on a bezier curve at a given t value
pub fn bezier_point_at(pos0: Vec2, pos1: Vec2, pos2: Vec2, pos3: Vec2, t: f32) -> Vec2 {
    // Last interpolation is the target point.
    bezier_de_casteljau(pos0, pos1, pos2, pos3, t).5
}

/// Split a bezier curve at a given t value and return control points for the two new curves.
/// Returns the points in the order along the curve.
pub fn bezier_split_at(pos0: Vec2, pos1: Vec2, pos2: Vec2, pos3: Vec2, t: f32) -> (Vec2, Vec2, Vec2, Vec2, Vec2) {
    let (a1, _, b2, a2, b1, mid) = bezier_de_casteljau(pos0, pos1, pos2, pos3, t);
    (a1, a2, mid, b1, b2)
}

/// Find the signed area of a bezier curve via closed form of Green's theorem for a cubic
pub fn bezier_signed_area(pos0: Vec2, pos1: Vec2, pos2: Vec2, pos3: Vec2) -> f32 {
    (6.0 * pos0.cross(pos1)
        + 3.0 * pos0.cross(pos2)
        + pos0.cross(pos3)
        + 3.0 * pos1.cross(pos2)
        + 3.0 * pos1.cross(pos3)
        + 6.0 * pos2.cross(pos3))
        / 20.0
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Control points for a typical S-shaped curve
    fn curve() -> (Vec2, Vec2, Vec2, Vec2) {
        (Vec2::new(0.0, 0.0), Vec2::new(1.0, 2.0), Vec2::new(3.0, -2.0), Vec2::new(4.0, 0.0))
    }

    fn assert_close(a: Vec2, b: Vec2) {
        assert!(a.distance(b) < 1e-5, "{a} is not close to {b}");
    }

    /// The curve starts at the first point and ends at the last
    #[test]
    fn point_at_endpoints() {
        let (p0, p1, p2, p3) = curve();
        assert_close(bezier_point_at(p0, p1, p2, p3, 0.0), p0);
        assert_close(bezier_point_at(p0, p1, p2, p3, 1.0), p3);
    }

    /// Midpoint of a cubic is (p0 + 3p1 + 3p2 + p3) / 8
    #[test]
    fn point_at_midpoint() {
        let (p0, p1, p2, p3) = curve();
        let expected = (p0 + 3.0 * p1 + 3.0 * p2 + p3) * 0.125;
        assert_close(bezier_point_at(p0, p1, p2, p3, 0.5), expected);
    }

    /// A straight line with evenly spaced controls should be traversed linearly
    #[test]
    fn point_at_straight_line() {
        let point = bezier_point_at(Vec2::ZERO, Vec2::hz(1.0), Vec2::hz(2.0), Vec2::hz(3.0), 0.25);
        assert_close(point, Vec2::hz(0.75));
    }

    /// De Casteljau intermediate points are successive lerps
    #[test]
    fn de_casteljau_points() {
        let (p0, p1, p2, p3) = curve();
        let (a0, a1, a2, b0, b1, c0) = bezier_de_casteljau(p0, p1, p2, p3, 0.5);
        assert_close(a0, Vec2::lerp(p0, p1, 0.5));
        assert_close(a1, Vec2::lerp(p1, p2, 0.5));
        assert_close(a2, Vec2::lerp(p2, p3, 0.5));
        assert_close(b0, Vec2::lerp(a0, a1, 0.5));
        assert_close(b1, Vec2::lerp(a1, a2, 0.5));
        assert_close(c0, Vec2::lerp(b0, b1, 0.5));
    }

    /// Each half of a split should trace the same curve as the original
    #[test]
    fn split_matches_original() {
        let (p0, p1, p2, p3) = curve();
        let t = 0.3;
        let (a1, a2, mid, b1, b2) = bezier_split_at(p0, p1, p2, p3, t);
        assert_close(mid, bezier_point_at(p0, p1, p2, p3, t));

        // The first half covers t in [0, t], the second half covers [t, 1]
        for u in [0.0, 0.25, 0.5, 0.75, 1.0] {
            assert_close(bezier_point_at(p0, a1, a2, mid, u), bezier_point_at(p0, p1, p2, p3, u * t));
            assert_close(bezier_point_at(mid, b1, b2, p3, u), bezier_point_at(p0, p1, p2, p3, t + u * (1.0 - t)));
        }
    }

    /// A line segment has no area
    #[test]
    fn signed_area_line() {
        let area = bezier_signed_area(Vec2::ZERO, Vec2::hz(1.0), Vec2::hz(2.0), Vec2::hz(3.0));
        assert!(area.abs() < 1e-6);
    }

    /// A line from (1, 0) to (0, 1) sweeps half of the unit square from the origin
    #[test]
    fn signed_area_triangle() {
        let (a, b) = (Vec2::RIGHT, Vec2::UP);
        let area = bezier_signed_area(a, Vec2::lerp(a, b, 1.0 / 3.0), Vec2::lerp(a, b, 2.0 / 3.0), b);
        assert!((area - 0.5).abs() < 1e-5);
    }

    /// Reversing a curve flips the sign of the area
    #[test]
    fn signed_area_reversed() {
        let (p0, p1, p2, p3) = curve();
        let forward = bezier_signed_area(p0, p1, p2, p3);
        let backward = bezier_signed_area(p3, p2, p1, p0);
        assert!((forward + backward).abs() < 1e-5);
    }
}
