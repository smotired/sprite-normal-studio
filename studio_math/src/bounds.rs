use crate::Vec2;

/// Correct two opposite corners of a bounding box so they are the top left and bottom right corners.
pub fn bounds_correct(corner0: Vec2, corner1: Vec2) -> (Vec2, Vec2) {
    (corner0.min(corner1), corner0.max(corner1))
}

/// Find the bounding box around two bounding boxes. Returns the top left and bottom right corners.
/// Works even if the corners haven't been corrected.
pub fn bounds_merge(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2) -> (Vec2, Vec2) {
    (a0.min(a1).min(b0.min(b1)), a0.max(a1).max(b0.max(b1)))
}

/// Find the bounding box around a set of points, or None if there are no points.
/// Returns the top left and bottom right corners.
pub fn bounds_of(points: impl IntoIterator<Item = Vec2>) -> Option<(Vec2, Vec2)> {
    let mut points = points.into_iter();
    let first = points.next()?;
    Some(points.fold((first, first), |(min, max), point| (min.min(point), max.max(point))))
}

/// Clamp both corners of a bounding box into another bounding box.
/// If the boxes don't overlap, the result has no width or height.
pub fn bounds_clamp(corner0: Vec2, corner1: Vec2, outer0: Vec2, outer1: Vec2) -> (Vec2, Vec2) {
    let (outer_min, outer_max) = bounds_correct(outer0, outer1);
    let clamp = |corner: Vec2| corner.max(outer_min).min(outer_max);
    (clamp(corner0), clamp(corner1))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flipped corners become the top left and bottom right
    #[test]
    fn correct() {
        let expected = (Vec2::new(1.0, 2.0), Vec2::new(5.0, 6.0));
        assert_eq!(bounds_correct(Vec2::new(5.0, 2.0), Vec2::new(1.0, 6.0)), expected);
        assert_eq!(bounds_correct(Vec2::new(5.0, 6.0), Vec2::new(1.0, 2.0)), expected);
        assert_eq!(bounds_correct(expected.0, expected.1), expected);
    }

    /// Merging covers both boxes
    #[test]
    fn merge() {
        let overlapping = bounds_merge(Vec2::new(0.0, 0.0), Vec2::new(4.0, 4.0), Vec2::new(2.0, -1.0), Vec2::new(3.0, 3.0));
        assert_eq!(overlapping, (Vec2::new(0.0, -1.0), Vec2::new(4.0, 4.0)));

        let disjoint = bounds_merge(Vec2::new(0.0, 0.0), Vec2::new(1.0, 1.0), Vec2::new(10.0, 10.0), Vec2::new(8.0, 12.0));
        assert_eq!(disjoint, (Vec2::new(0.0, 0.0), Vec2::new(10.0, 12.0)));
    }

    /// The box around points covers every point
    #[test]
    fn of_points() {
        assert_eq!(bounds_of([]), None);
        assert_eq!(bounds_of([Vec2::new(3.0, 4.0)]), Some((Vec2::new(3.0, 4.0), Vec2::new(3.0, 4.0))));
        let points = [Vec2::new(0.0, 5.0), Vec2::new(-2.0, 1.0), Vec2::new(4.0, 3.0), Vec2::new(1.0, -1.0)];
        assert_eq!(bounds_of(points), Some((Vec2::new(-2.0, -1.0), Vec2::new(4.0, 5.0))));
    }

    /// Clamping keeps boxes inside, trims boxes partly outside, and collapses boxes fully outside
    #[test]
    fn clamp() {
        let (outer0, outer1) = (Vec2::ZERO, Vec2::new(10.0, 10.0));
        let inside = (Vec2::new(2.0, 3.0), Vec2::new(4.0, 5.0));
        assert_eq!(bounds_clamp(inside.0, inside.1, outer0, outer1), inside);
        assert_eq!(bounds_clamp(Vec2::new(-5.0, 2.0), Vec2::new(4.0, 20.0), outer0, outer1), (Vec2::new(0.0, 2.0), Vec2::new(4.0, 10.0)));

        let (min, max) = bounds_clamp(Vec2::new(20.0, 2.0), Vec2::new(30.0, 4.0), outer0, outer1);
        assert_eq!(max.x - min.x, 0.0);
    }
}
