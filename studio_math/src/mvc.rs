//! Mean value coordinates, which describe a position as a weighted average of the corners of a polygon (the cage).
//! Moving the corners and taking the same weighted average moves the position along with the cage, smoothly,
//! and exactly as an affine transform if the whole cage is transformed that way.
//! From Hormann and Floater, "Mean value coordinates for arbitrary planar polygons" (2006), which works for concave cages and positions outside them.

use crate::Vec2;

/// Get the mean value coordinates of a position in a cage: one weight for each corner, which sum to 1.
/// Positions on a corner or edge get weights for just that corner or edge.
/// Returns None if the cage has fewer than 3 corners, or the position can't be described by it.
pub fn mean_value_weights(cage: &[Vec2], pos: Vec2) -> Option<Vec<f32>> {
    let count = cage.len();
    if count < 3 { return None; }

    let offsets: Vec<Vec2> = cage.iter().map(|&corner| corner - pos).collect();
    let distances: Vec<f32> = offsets.iter().map(|offset| offset.magnitude()).collect();
    let mut weights = vec![0.0; count];

    // On a corner or an edge, interpolate along it
    for i in 0..count {
        let next = (i + 1) % count;
        if distances[i] <= 1e-6 {
            weights[i] = 1.0;
            return Some(weights);
        }
        let area = offsets[i].cross(offsets[next]);
        if area.abs() <= 1e-6 * distances[i] * distances[next] && offsets[i].dot(offsets[next]) < 0.0 {
            let total = distances[i] + distances[next];
            weights[i] = distances[next] / total;
            weights[next] = distances[i] / total;
            return Some(weights);
        }
    }

    // Otherwise each corner is weighted by the tangents of the half angles on either side of it, over its distance.
    // Edges in a line with the position contribute nothing.
    let term = |i: usize| {
        let next = (i + 1) % count;
        let area = offsets[i].cross(offsets[next]);
        if area == 0.0 { return 0.0; }
        (distances[i] * distances[next] - offsets[i].dot(offsets[next])) / area
    };
    for (i, weight) in weights.iter_mut().enumerate() {
        let prev = (i + count - 1) % count;
        *weight = (term(prev) + term(i)) / distances[i];
    }

    let total: f32 = weights.iter().sum();
    if !total.is_finite() || total.abs() <= 1e-12 { return None; }
    for weight in &mut weights { *weight /= total; }
    Some(weights)
}

/// Get the weighted average of the corners of a cage, using weights from mean_value_weights
pub fn interpolate(cage: &[Vec2], weights: &[f32]) -> Vec2 {
    cage.iter().zip(weights).fold(Vec2::ZERO, |sum, (&corner, &weight)| sum + corner * weight)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::Affine2;

    /// A concave cage shaped like an L
    fn l_cage() -> Vec<Vec2> {
        [(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (10.0, 10.0), (10.0, 20.0), (0.0, 20.0)].into_iter().map(Vec2::from).collect()
    }

    fn assert_near(actual: Vec2, expected: Vec2) {
        assert!(actual.distance(expected) < 1e-3, "{actual} != {expected}");
    }

    /// Weights sum to 1 and average back to the position, inside, outside, and in the concave part
    #[test]
    fn reproduces_positions() {
        let cage = l_cage();
        for pos in [(5.0, 5.0), (15.0, 5.0), (5.0, 15.0), (15.0, 15.0), (-5.0, 8.0), (30.0, 30.0)] {
            let pos = Vec2::from(pos);
            let weights = mean_value_weights(&cage, pos).unwrap();
            assert!((weights.iter().sum::<f32>() - 1.0).abs() < 1e-4);
            assert_near(interpolate(&cage, &weights), pos);
        }
    }

    /// Positions on corners and edges only depend on them
    #[test]
    fn on_corners_and_edges() {
        let cage = l_cage();
        assert_eq!(mean_value_weights(&cage, Vec2::new(20.0, 10.0)).unwrap(), vec![0.0, 0.0, 1.0, 0.0, 0.0, 0.0]);
        let weights = mean_value_weights(&cage, Vec2::new(5.0, 0.0)).unwrap();
        assert_eq!(weights, vec![0.75, 0.25, 0.0, 0.0, 0.0, 0.0]);
    }

    /// Transforming the cage transforms positions exactly the same way
    #[test]
    fn linear_precision() {
        let cage = l_cage();
        let transform = Affine2::rotate_about(0.7, Vec2::new(3.0, 4.0));
        let transform = Affine2 { x_axis: transform.x_axis * 2.0, ..transform };
        let moved: Vec<Vec2> = cage.iter().map(|&corner| transform.point(corner)).collect();
        for pos in [(5.0, 5.0), (15.0, 15.0), (-5.0, 8.0)] {
            let pos = Vec2::from(pos);
            let weights = mean_value_weights(&cage, pos).unwrap();
            assert_near(interpolate(&moved, &weights), transform.point(pos));
        }
    }

    /// Moving one corner moves nearby positions more than far ones
    #[test]
    fn local_influence() {
        let cage = l_cage();
        let mut moved = cage.clone();
        moved[1] += Vec2::new(10.0, 0.0);
        let near = Vec2::new(18.0, 2.0);
        let far = Vec2::new(2.0, 18.0);
        let near_shift = interpolate(&moved, &mean_value_weights(&cage, near).unwrap()) - near;
        let far_shift = interpolate(&moved, &mean_value_weights(&cage, far).unwrap()) - far;
        assert!(near_shift.x > far_shift.x.abs() * 3.0);
    }

    /// Cages need at least 3 corners
    #[test]
    fn too_small() {
        assert!(mean_value_weights(&[Vec2::ZERO, Vec2::ONE], Vec2::new(0.5, 0.0)).is_none());
    }
}
