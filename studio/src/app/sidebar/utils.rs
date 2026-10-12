use std::f32::consts::PI;

use studio_math::{Vec2, Vec3};

/// Radii where the rings of the faceted ball change, from the pole outward, as a fraction of the ball's radius.
/// Ring i (0 is the pole) has a normal i * FACET_ANGLE_STEP away from straight out, so the last ring is the rim.
/// The rings are drawn wider than their angle would project to, so they're easy to click. Update these if the faceted ball texture changes.
pub const FACET_BREAKPOINTS: [f32; 4] = [0.2, 0.5, 0.7, 0.875];

/// Angle covered by each facet of the faceted ball, both around the ball and outward from the pole.
/// Facets around the ball are centered on multiples of this, starting at 0.
pub const FACET_ANGLE_STEP: f32 = PI / 8.0;

/// How close a normal's angles have to be to a facet's center to count as that facet.
const FACET_ANGLE_TOLERANCE: f32 = 1e-3;

// Positions on the ball are relative to its center, as a fraction of its radius, with +Y up.

/// Get the normal at a position on the smooth ball. Positions off the ball use the closest point on its edge.
pub fn smooth_normal(pos: Vec2) -> Vec3 {
    let magnitude = pos.magnitude();
    if magnitude >= 1.0 {
        let edge = pos * (1.0 / magnitude);
        return Vec3::new(edge.x, edge.y, 0.0);
    }
    Vec3::new(pos.x, pos.y, (1.0 - pos.x * pos.x - pos.y * pos.y).max(0.0).sqrt())
}

/// Get the position of a normal on the smooth ball, or None if it faces away.
pub fn smooth_position(normal: Vec3) -> Option<Vec2> {
    let normal = normal.normalized();
    (normal.z >= 0.0).then_some(Vec2::new(normal.x, normal.y))
}

/// Get the normal of the facet at a position on the faceted ball. Positions off the ball use the rim.
pub fn faceted_normal(pos: Vec2) -> Vec3 {
    let r = pos.magnitude().min(1.0);
    let ring = FACET_BREAKPOINTS.iter().filter(|&&breakpoint| r >= breakpoint).count();
    if ring == 0 { return Vec3::FORWARD; }

    let phi = ring as f32 * FACET_ANGLE_STEP;
    let theta = (pos.y.atan2(pos.x) / FACET_ANGLE_STEP).round() * FACET_ANGLE_STEP;
    Vec3::new(phi.sin() * theta.cos(), phi.sin() * theta.sin(), phi.cos().max(0.0))
}

/// Get the position of the center of the facet with a normal on the faceted ball, or None if no facet has that normal.
pub fn faceted_position(normal: Vec3) -> Option<Vec2> {
    let normal = normal.normalized();

    // Find the ring from the angle away from straight out
    let ring = snap_to_facet(normal.z.clamp(-1.0, 1.0).acos())?;
    if ring == 0 { return Some(Vec2::ZERO); }
    if ring > FACET_BREAKPOINTS.len() as i32 { return None; }

    // Find the facet around the ring
    let slice = snap_to_facet(normal.y.atan2(normal.x))?;
    let theta = slice as f32 * FACET_ANGLE_STEP;

    // The facet's center is halfway through its ring
    let ring = ring as usize;
    let inner = FACET_BREAKPOINTS[ring - 1];
    let outer = FACET_BREAKPOINTS.get(ring).copied().unwrap_or(1.0);
    let r = (inner + outer) / 2.0;
    Some(Vec2::new(r * theta.cos(), r * theta.sin()))
}

/// Get how many facet steps an angle is, or None if it isn't at the center of a facet.
fn snap_to_facet(angle: f32) -> Option<i32> {
    let steps = (angle / FACET_ANGLE_STEP).round();
    ((angle - steps * FACET_ANGLE_STEP).abs() <= FACET_ANGLE_TOLERANCE).then_some(steps as i32)
}


#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(a: Vec3, b: Vec3) {
        assert!(a.distance(b) < 1e-4, "{a:?} != {b:?}");
    }

    fn assert_near_2d(a: Vec2, b: Vec2) {
        assert!(a.distance(b) < 1e-4, "{a:?} != {b:?}");
    }

    /// The smooth ball faces straight out at the center, and up at the top
    #[test]
    fn smooth_normal_on_ball() {
        assert_near(smooth_normal(Vec2::ZERO), Vec3::FORWARD);
        assert_near(smooth_normal(Vec2::new(0.0, 1.0)), Vec3::UP);
        assert_near(smooth_normal(Vec2::new(0.6, 0.0)), Vec3::new(0.6, 0.0, 0.8));
    }

    /// Positions off the smooth ball use the closest point on its edge
    #[test]
    fn smooth_normal_off_ball() {
        assert_near(smooth_normal(Vec2::new(-3.0, 0.0)), Vec3::LEFT);
        assert_near(smooth_normal(Vec2::new(2.0, 2.0)), Vec3::new(0.5f32.sqrt(), 0.5f32.sqrt(), 0.0));
    }

    /// Smooth positions and normals round trip, and normals facing away have no position
    #[test]
    fn smooth_round_trip() {
        let pos = Vec2::new(0.3, -0.4);
        assert_near_2d(smooth_position(smooth_normal(pos)).unwrap(), pos);
        assert!(smooth_position(Vec3::BACK).is_none());
    }

    /// Each ring of the faceted ball starts at its breakpoint
    #[test]
    fn faceted_rings() {
        let phi_at = |r: f32| faceted_normal(Vec2::new(r, 0.0)).z.acos();
        assert_eq!(faceted_normal(Vec2::new(0.19, 0.05)), Vec3::FORWARD);
        assert!((phi_at(0.2) - FACET_ANGLE_STEP).abs() < 1e-5);
        assert!((phi_at(0.449) - FACET_ANGLE_STEP).abs() < 1e-5);
        assert!((phi_at(0.45) - 2.0 * FACET_ANGLE_STEP).abs() < 1e-5);
        assert!((phi_at(0.7) - 3.0 * FACET_ANGLE_STEP).abs() < 1e-5);
        assert_near(faceted_normal(Vec2::new(0.9, 0.0)), Vec3::RIGHT);
        assert_near(faceted_normal(Vec2::new(5.0, 0.0)), Vec3::RIGHT);
    }

    /// Facets around the ball are centered on multiples of the facet angle, starting at 0
    #[test]
    fn faceted_slices() {
        let small = FACET_ANGLE_STEP * 0.4;
        let at = |theta: f32| faceted_normal(Vec2::new(0.95 * theta.cos(), 0.95 * theta.sin()));
        assert_near(at(small), Vec3::RIGHT);
        assert_near(at(-small), Vec3::RIGHT);
        assert_near(at(FACET_ANGLE_STEP * 3.6), Vec3::new((FACET_ANGLE_STEP * 4.0).cos(), (FACET_ANGLE_STEP * 4.0).sin(), 0.0));
        assert_near(at(PI / 2.0), Vec3::UP);
    }

    /// Facet normals are drawn at the middle of their facet, and other normals aren't drawn
    #[test]
    fn faceted_positions() {
        assert_eq!(faceted_position(Vec3::FORWARD), Some(Vec2::ZERO));
        assert_near_2d(faceted_position(faceted_normal(Vec2::new(0.3, 0.0))).unwrap(), Vec2::new(0.325, 0.0));
        assert_near_2d(faceted_position(Vec3::UP).unwrap(), Vec2::new(0.0, 0.9375));
        assert!(faceted_position(smooth_normal(Vec2::new(0.3, 0.1))).is_none());
        assert!(faceted_position(Vec3::BACK).is_none());
    }

    /// Every facet's normal maps back to a position inside that facet
    #[test]
    fn faceted_round_trip() {
        for ring in 1..=FACET_BREAKPOINTS.len() {
            for slice in 0..16 {
                let theta = slice as f32 * FACET_ANGLE_STEP;
                let r = FACET_BREAKPOINTS[ring - 1] + 0.01;
                let normal = faceted_normal(Vec2::new(r * theta.cos(), r * theta.sin()));
                let pos = faceted_position(normal).unwrap();
                assert_near(faceted_normal(pos), normal);
            }
        }
    }
}
