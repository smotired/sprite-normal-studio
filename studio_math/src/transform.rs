use crate::Vec2;

/// A 2D affine transformation: a linear map followed by a translation.
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct Affine2 {
    /// Where the linear part sends the x axis
    pub x_axis: Vec2,

    /// Where the linear part sends the y axis
    pub y_axis: Vec2,

    /// Added after the linear part
    pub translation: Vec2,
}

impl Affine2 {
    pub const IDENTITY: Self = Self { x_axis: Vec2::RIGHT, y_axis: Vec2::UP, translation: Vec2::ZERO };

    /// Scale each axis by its own factor, keeping an anchor in place
    pub fn scale_about(factors: Vec2, anchor: Vec2) -> Self {
        Self::about(Vec2::hz(factors.x), Vec2::vt(factors.y), anchor)
    }

    /// Rotate by an angle in radians, keeping a center in place
    pub fn rotate_about(angle: f32, center: Vec2) -> Self {
        let (sin, cos) = angle.sin_cos();
        Self::about(Vec2::new(cos, sin), Vec2::new(-sin, cos), center)
    }

    /// Move everything by an offset
    pub fn translate(offset: Vec2) -> Self { Self { translation: offset, ..Self::IDENTITY } }

    /// How much the transform scales areas by. Negative if it mirrors.
    pub fn determinant(&self) -> f32 { self.x_axis.cross(self.y_axis) }

    /// A linear map that keeps a fixed point in place
    fn about(x_axis: Vec2, y_axis: Vec2, fixed: Vec2) -> Self {
        let linear = Self { x_axis, y_axis, translation: Vec2::ZERO };
        Self { translation: fixed - linear.vector(fixed), ..linear }
    }

    /// Transform a position
    pub fn point(&self, point: Vec2) -> Vec2 { self.vector(point) + self.translation }

    /// Transform a relative vector, which ignores the translation
    pub fn vector(&self, vector: Vec2) -> Vec2 { self.x_axis * vector.x + self.y_axis * vector.y }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Scaling keeps the anchor in place and scales everything else away from it
    #[test]
    fn scale_about() {
        let transform = Affine2::scale_about(Vec2::new(2.0, -1.0), Vec2::new(10.0, 10.0));
        assert_eq!(transform.point(Vec2::new(10.0, 10.0)), Vec2::new(10.0, 10.0));
        assert_eq!(transform.point(Vec2::new(12.0, 13.0)), Vec2::new(14.0, 7.0));
        assert_eq!(transform.vector(Vec2::new(1.0, 1.0)), Vec2::new(2.0, -1.0));
    }

    /// Rotating keeps the center in place and turns from +x towards +y
    #[test]
    fn rotate_about() {
        let transform = Affine2::rotate_about(std::f32::consts::FRAC_PI_2, Vec2::new(5.0, 5.0));
        assert!(transform.point(Vec2::new(5.0, 5.0)).distance(Vec2::new(5.0, 5.0)) < 1e-5);
        assert!(transform.point(Vec2::new(10.0, 5.0)).distance(Vec2::new(5.0, 10.0)) < 1e-5);
        assert!(transform.vector(Vec2::new(0.0, 2.0)).distance(Vec2::new(-2.0, 0.0)) < 1e-5);
    }

    /// Translating moves points but not vectors
    #[test]
    fn translate() {
        let transform = Affine2::translate(Vec2::new(2.0, -1.0));
        assert_eq!(transform.point(Vec2::new(1.0, 1.0)), Vec2::new(3.0, 0.0));
        assert_eq!(transform.vector(Vec2::new(1.0, 1.0)), Vec2::new(1.0, 1.0));
    }

    /// The determinant scales areas, and is negative for mirroring
    #[test]
    fn determinant() {
        assert_eq!(Affine2::scale_about(Vec2::new(2.0, 3.0), Vec2::ONE).determinant(), 6.0);
        assert_eq!(Affine2::scale_about(Vec2::new(-1.0, 1.0), Vec2::ONE).determinant(), -1.0);
        assert!((Affine2::rotate_about(1.0, Vec2::ONE).determinant() - 1.0).abs() < 1e-6);
    }

    /// The identity changes nothing
    #[test]
    fn identity() {
        assert_eq!(Affine2::IDENTITY.point(Vec2::new(3.0, -4.0)), Vec2::new(3.0, -4.0));
    }
}
