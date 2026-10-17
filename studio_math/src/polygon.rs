use crate::Vec2;

/// Get the signed area of a polygon. Positive if it winds from +x towards +y.
pub fn signed_area(polygon: &[Vec2]) -> f32 {
    let mut area = 0.0;
    for (i, &point) in polygon.iter().enumerate() {
        area += point.cross(polygon[(i + 1) % polygon.len()]);
    }
    area * 0.5
}

/// Returns true if any two edges of a closed polygon cross or touch, other than neighboring edges meeting at their shared corner
pub fn self_intersects(polygon: &[Vec2]) -> bool {
    let count = polygon.len();
    for i in 0..count {
        for j in i + 1..count {
            // Neighboring edges share a corner, which doesn't count
            if j == i + 1 || (i == 0 && j == count - 1) { continue; }
            let (a0, a1) = (polygon[i], polygon[(i + 1) % count]);
            let (b0, b1) = (polygon[j], polygon[(j + 1) % count]);
            if segments_touch(a0, a1, b0, b1) { return true; }
        }
    }
    false
}

/// Split a simple polygon into triangles by ear clipping. Returns triples of indices into the polygon, wound the same way as the polygon.
/// Corners in a straight line with their neighbors are dropped rather than making a triangle with no area.
/// Returns None if the polygon has fewer than 3 corners, crosses itself, or has no area.
pub fn triangulate(polygon: &[Vec2]) -> Option<Vec<[usize; 3]>> {
    if polygon.len() < 3 || self_intersects(polygon) { return None; }
    let winding = signed_area(polygon).signum();
    if winding == 0.0 { return None; }

    let mut remaining: Vec<usize> = (0..polygon.len()).collect();
    let mut triangles = vec![];
    while remaining.len() > 3 {
        let count = remaining.len();
        let corner_at = |i: usize| (remaining[(i + count - 1) % count], remaining[i], remaining[(i + 1) % count]);
        let turn_at = |i: usize| {
            let (prev, corner, next) = corner_at(i);
            let (a, b, c) = (polygon[prev], polygon[corner], polygon[next]);
            ((b - a).cross(c - b) * winding, 1e-6 * (b - a).magnitude() * (c - b).magnitude())
        };

        // Corners in a straight line are dropped first, so they don't make extra thin triangles
        if let Some(i) = (0..count).find(|&i| { let (turn, tolerance) = turn_at(i); turn.abs() <= tolerance }) {
            remaining.remove(i);
            continue;
        }

        // Otherwise clip an ear: a corner turning the same way as the polygon, with no other corner inside it
        let ear = (0..count).find(|&i| {
            if turn_at(i).0 < 0.0 { return false; }
            let (prev, corner, next) = corner_at(i);
            !remaining.iter()
                .filter(|&&other| other != prev && other != corner && other != next)
                .any(|&other| in_triangle(polygon[other], polygon[prev], polygon[corner], polygon[next]))
        });

        // A simple polygon always has an ear, so this only happens through rounding
        let i = ear?;
        let (prev, corner, next) = corner_at(i);
        triangles.push([prev, corner, next]);
        remaining.remove(i);
    }

    let (a, b, c) = (polygon[remaining[0]], polygon[remaining[1]], polygon[remaining[2]]);
    if (b - a).cross(c - a).abs() > 1e-6 { triangles.push([remaining[0], remaining[1], remaining[2]]); }
    (!triangles.is_empty()).then_some(triangles)
}

/// Returns true if a point is inside a triangle or on its edges, whichever way it winds
fn in_triangle(point: Vec2, a: Vec2, b: Vec2, c: Vec2) -> bool {
    let sides = [(b - a).cross(point - a), (c - b).cross(point - b), (a - c).cross(point - c)];
    sides.iter().all(|&side| side >= 0.0) || sides.iter().all(|&side| side <= 0.0)
}

/// Returns true if two line segments cross or touch
fn segments_touch(a0: Vec2, a1: Vec2, b0: Vec2, b1: Vec2) -> bool {
    let side = |start: Vec2, end: Vec2, point: Vec2| (end - start).cross(point - start);
    let (d0, d1) = (side(b0, b1, a0), side(b0, b1, a1));
    let (d2, d3) = (side(a0, a1, b0), side(a0, a1, b1));
    if ((d0 > 0.0 && d1 < 0.0) || (d0 < 0.0 && d1 > 0.0)) && ((d2 > 0.0 && d3 < 0.0) || (d2 < 0.0 && d3 > 0.0)) { return true; }

    // Touching: an end lies on the other segment
    let on_segment = |start: Vec2, end: Vec2, point: Vec2, side: f32| {
        side == 0.0 && point.x >= start.x.min(end.x) && point.x <= start.x.max(end.x) && point.y >= start.y.min(end.y) && point.y <= start.y.max(end.y)
    };
    on_segment(b0, b1, a0, d0) || on_segment(b0, b1, a1, d1) || on_segment(a0, a1, b0, d2) || on_segment(a0, a1, b1, d3)
}

/// Returns true if a point is inside a closed polygon, by counting how many edges a ray straight down from it crosses
pub fn point_in_polygon(point: Vec2, polygon: &[Vec2]) -> bool {
    let mut inside = false;
    for i in 0..polygon.len() {
        let (start, end) = (polygon[i], polygon[(i + 1) % polygon.len()]);
        if (start.x <= point.x) != (end.x <= point.x) && start.y + (point.x - start.x) * (end.y - start.y) / (end.x - start.x) >= point.y {
            inside = !inside;
        }
    }
    inside
}

#[cfg(test)]
mod tests {
    use super::*;

    fn polygon(points: &[(f32, f32)]) -> Vec<Vec2> { points.iter().map(|&point| Vec2::from(point)).collect() }

    /// Total area of the triangles, which should cover the polygon exactly
    fn area_of(polygon: &[Vec2], triangles: &[[usize; 3]]) -> f32 {
        triangles.iter().map(|&[a, b, c]| signed_area(&[polygon[a], polygon[b], polygon[c]]).abs()).sum()
    }

    /// Area is positive winding from +x towards +y
    #[test]
    fn area() {
        let square = polygon(&[(0.0, 0.0), (2.0, 0.0), (2.0, 2.0), (0.0, 2.0)]);
        assert_eq!(signed_area(&square), 4.0);
        let reversed: Vec<Vec2> = square.iter().rev().copied().collect();
        assert_eq!(signed_area(&reversed), -4.0);
    }

    /// Squares split into two triangles, whichever way they wind
    #[test]
    fn square() {
        let square = polygon(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        let reversed: Vec<Vec2> = square.iter().rev().copied().collect();
        for shape in [square, reversed] {
            let triangles = triangulate(&shape).unwrap();
            assert_eq!(triangles.len(), 2);
            assert!((area_of(&shape, &triangles) - 100.0).abs() < 1e-3);
        }
    }

    /// Concave polygons are covered exactly, without triangles outside them
    #[test]
    fn concave() {
        let arrow = polygon(&[(0.0, 0.0), (10.0, 5.0), (0.0, 10.0), (3.0, 5.0)]);
        let triangles = triangulate(&arrow).unwrap();
        assert_eq!(triangles.len(), 2);
        assert!((area_of(&arrow, &triangles) - signed_area(&arrow).abs()).abs() < 1e-3);

        let hexagon_l = polygon(&[(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (10.0, 10.0), (10.0, 20.0), (0.0, 20.0)]);
        let triangles = triangulate(&hexagon_l).unwrap();
        assert_eq!(triangles.len(), 4);
        assert!((area_of(&hexagon_l, &triangles) - 300.0).abs() < 1e-3);
    }

    /// Corners in a straight line are dropped instead of making flat triangles
    #[test]
    fn collinear() {
        let square = polygon(&[(0.0, 0.0), (5.0, 0.0), (10.0, 0.0), (10.0, 10.0), (0.0, 10.0)]);
        let triangles = triangulate(&square).unwrap();
        assert_eq!(triangles.len(), 2);
        assert!((area_of(&square, &triangles) - 100.0).abs() < 1e-3);
    }

    /// Polygons that cross themselves, have no area, or too few corners can't be split
    #[test]
    fn invalid() {
        let bowtie = polygon(&[(0.0, 0.0), (10.0, 10.0), (10.0, 0.0), (0.0, 10.0)]);
        assert!(self_intersects(&bowtie));
        assert!(triangulate(&bowtie).is_none());
        assert!(triangulate(&polygon(&[(0.0, 0.0), (5.0, 0.0), (10.0, 0.0)])).is_none());
        assert!(triangulate(&polygon(&[(0.0, 0.0), (5.0, 0.0)])).is_none());
        assert!(!self_intersects(&polygon(&[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)])));
    }

    /// Points inside are found by crossings, and points outside aren't
    #[test]
    fn points_in_polygon() {
        let triangle = polygon(&[(0.0, 0.0), (10.0, 0.0), (0.0, 10.0)]);
        assert!(point_in_polygon(Vec2::new(2.0, 2.0), &triangle));
        assert!(!point_in_polygon(Vec2::new(8.0, 8.0), &triangle));
    }
}
