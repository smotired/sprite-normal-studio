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
    return bezier_de_casteljau(pos0, pos1, pos2, pos3, t).5;
}

/// Split a bezier curve at a given t value and return control points for the two new curves.
/// Returns the points in the order along the curve.
pub fn bezier_split_at(pos0: Vec2, pos1: Vec2, pos2: Vec2, pos3: Vec2, t: f32) -> (Vec2, Vec2, Vec2, Vec2, Vec2) {
    let (a1, _, b2, a2, b1, mid) = bezier_de_casteljau(pos0, pos1, pos2, pos3, t);
    return (a1, a2, mid, b1, b2);
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