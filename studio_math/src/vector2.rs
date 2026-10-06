use std::fmt::Display;
use std::ops;

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vec2 { pub x: f32, pub y: f32, }

/// Basic vector2 struct
impl Vec2 {
    /// Constants
    pub const ZERO:  Self = Self { x:  0.0, y:  0.0 };
    pub const ONE:   Self = Self { x:  1.0, y:  1.0 };
    pub const UP:    Self = Self { x:  0.0, y:  1.0 };
    pub const DOWN:  Self = Self { x:  0.0, y: -1.0 };
    pub const RIGHT: Self = Self { x:  1.0, y:  0.0 };
    pub const LEFT:  Self = Self { x: -1.0, y:  0.0 };

    /// Create a new Vec2 from x and y coords
    pub fn new(x: f32, y: f32) -> Self { Self { x, y } }

    /// Create a Vec2 on the x axis
    pub fn hz(x: f32) -> Self { Self { x, y: 0.0 } }

    /// Create a Vec2 on the y axis
    pub fn vt(y: f32) -> Self { Self { x: 0.0, y } }

    /// Create a vector with identical x and y
    pub fn square(v: f32) -> Self { Self { x: v, y: v } }

    /// Create a random normalized vector (uniformly distributed on unit circle)
    pub fn random_on_circle() -> Self {
        let angle = rand::random_range(0.0 .. core::f32::consts::TAU);
        Self {
            x: angle.cos(),
            y: angle.sin(),
        }
    }

    /// Find the dot product with another Vec2
    pub fn dot(self, rhs: Self) -> f32 { self.x * rhs.x + self.y * rhs.y }

    /// Find the cross product with another Vec2 (determinant of 2D matrix)
    pub fn cross(self, rhs: Self) -> f32 { self.x * rhs.y - self.y * rhs.x }

    /// Square magnitude of this vector
    pub fn sq_magnitude(self) -> f32 { self.dot(self) }

    /// Magnitude of this vector
    pub fn magnitude(self) -> f32 { self.sq_magnitude().sqrt() }

    /// Normalized form of this vector, or 0 if it's the zero vector
    pub fn normalized(self) -> Self { 
        let magnitude = self.magnitude();
        if magnitude == 0.0 {
            Self::ZERO
        } else {
            self * (1.0 / magnitude)
        }
    }

    /// Result of rotating this vector 90 degrees counter-clockwise
    pub fn left(self) -> Self { Self { x: -self.y, y: self.x } }

    /// Result of rotating this vector 90 degrees clockwise
    pub fn right(self) -> Self { Self { x: self.y, y: -self.x } }

    /// Square distance from the point represented by this vector to the point represented by another vector
    pub fn sq_distance(self, rhs: Self) -> f32 { (rhs - self).sq_magnitude() }

    /// Distance from the point represented by this vector to the point represented by another vector
    pub fn distance(self, rhs: Self) -> f32 { (rhs - self).magnitude() }

    /// Linear interpolation between two vectors according to a value between 0 and 1
    pub fn lerp(lhs: Self, rhs: Self, t: f32) -> Vec2 { lhs + t.clamp(0.0, 1.0) * (rhs - lhs) }

    /// Convert to bytes for writing into a buffer
    pub fn bytes(&self) -> &[u8] { bytemuck::bytes_of(self) }
}

impl ops::Add<Self> for Vec2 {
    type Output = Self;

    fn add(self, rhs: Self) -> Self::Output {
        Self::Output { x: self.x + rhs.x, y: self.y + rhs.y }
    }
}

impl ops::AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl ops::Sub<Self> for Vec2 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Output { x: self.x - rhs.x, y: self.y - rhs.y }
    }
}

impl ops::SubAssign for Vec2 {
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
    }
}

impl ops::Mul<f32> for Vec2 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::Output { x: self.x * rhs, y: self.y * rhs }
    }
}

impl ops::MulAssign<f32> for Vec2 {
    fn mul_assign(&mut self, rhs: f32) {
        self.x *= rhs;
        self.y *= rhs;
    }
}

impl ops::Mul<Vec2> for f32 {
    type Output = Vec2;

    fn mul(self, rhs: Vec2) -> Self::Output {
        Self::Output { x: self * rhs.x, y: self * rhs.y }
    }
}

impl ops::Neg for Vec2 {
    type Output = Self;

    fn neg(self) -> Self::Output { self * -1.0 }
}

impl From<(f32, f32)> for Vec2 {
    fn from(value: (f32, f32)) -> Self {
        Self { x: value.0, y: value.1 }
    }
}

impl From<(usize, usize)> for Vec2 {
    fn from(value: (usize, usize)) -> Self {
        Self { x: value.0 as f32, y: value.1 as f32 }
    }
}

impl From<[f32; 2]> for Vec2 {
    fn from(value: [f32; 2]) -> Self {
        Self { x: value[0], y: value[1] }
    }
}

impl From<egui::Pos2> for Vec2 {
    fn from(value: egui::Pos2) -> Self {
        Self { x: value.x, y: value.y }
    }
}

impl From<egui::Vec2> for Vec2 {
    fn from(value: egui::Vec2) -> Self {
        Self { x: value.x, y: value.y }
    }
}

impl From<wgpu::Extent3d> for Vec2 {
    fn from(value: wgpu::Extent3d) -> Self {
        Self { x: value.width as f32, y: value.height as f32 }
    }
}

impl Display for Vec2 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {})", self.x, self.y)
    }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// Constructors should place values on the right axes
    #[test]
    fn constructors() {
        assert_eq!(Vec2::new(1.0, 2.0), Vec2 { x: 1.0, y: 2.0 });
        assert_eq!(Vec2::hz(3.0), Vec2::new(3.0, 0.0));
        assert_eq!(Vec2::vt(3.0), Vec2::new(0.0, 3.0));
        assert_eq!(Vec2::square(4.0), Vec2::new(4.0, 4.0));
    }

    /// Random vectors should always be unit length
    #[test]
    fn random_on_circle_is_normalized() {
        for _ in 0..100 {
            assert!((Vec2::random_on_circle().magnitude() - 1.0).abs() < 1e-5);
        }
    }

    /// Dot product of perpendicular vectors is 0
    #[test]
    fn dot() {
        assert_eq!(Vec2::new(1.0, 2.0).dot(Vec2::new(3.0, 4.0)), 11.0);
        assert_eq!(Vec2::RIGHT.dot(Vec2::UP), 0.0);
    }

    /// Cross product is the determinant, so it's positive going counter-clockwise
    #[test]
    fn cross() {
        assert_eq!(Vec2::RIGHT.cross(Vec2::UP), 1.0);
        assert_eq!(Vec2::UP.cross(Vec2::RIGHT), -1.0);
        assert_eq!(Vec2::ONE.cross(Vec2::ONE), 0.0);
    }

    /// 3-4-5 triangle
    #[test]
    fn magnitude() {
        assert_eq!(Vec2::new(3.0, 4.0).sq_magnitude(), 25.0);
        assert_eq!(Vec2::new(3.0, 4.0).magnitude(), 5.0);
    }

    /// Normalizing gives unit length, and the zero vector stays zero
    #[test]
    fn normalized() {
        assert_eq!(Vec2::new(0.0, 5.0).normalized(), Vec2::UP);
        assert!((Vec2::new(3.0, 4.0).normalized().magnitude() - 1.0).abs() < 1e-6);
        assert_eq!(Vec2::ZERO.normalized(), Vec2::ZERO);
    }

    /// Left is counter-clockwise, right is clockwise
    #[test]
    fn left_and_right() {
        assert_eq!(Vec2::RIGHT.left(), Vec2::UP);
        assert_eq!(Vec2::UP.left(), Vec2::LEFT);
        assert_eq!(Vec2::RIGHT.right(), Vec2::DOWN);
        assert_eq!(Vec2::new(2.0, 3.0).left().right(), Vec2::new(2.0, 3.0));
    }

    /// Distance should be symmetric
    #[test]
    fn distance() {
        let a = Vec2::new(1.0, 1.0);
        let b = Vec2::new(4.0, 5.0);
        assert_eq!(a.sq_distance(b), 25.0);
        assert_eq!(a.distance(b), 5.0);
        assert_eq!(b.distance(a), 5.0);
    }

    /// Lerp hits both endpoints and the middle
    #[test]
    fn lerp() {
        let a = Vec2::new(0.0, 10.0);
        let b = Vec2::new(10.0, 20.0);
        assert_eq!(Vec2::lerp(a, b, 0.0), a);
        assert_eq!(Vec2::lerp(a, b, 1.0), b);
        assert_eq!(Vec2::lerp(a, b, 0.5), Vec2::new(5.0, 15.0));
    }

    /// Lerp clamps t to between 0 and 1
    #[test]
    fn lerp_clamps() {
        assert_eq!(Vec2::lerp(Vec2::ZERO, Vec2::ONE, -1.0), Vec2::ZERO);
        assert_eq!(Vec2::lerp(Vec2::ZERO, Vec2::ONE, 2.0), Vec2::ONE);
    }

    /// Bytes should be x then y as native f32s
    #[test]
    fn bytes() {
        let vector = Vec2::new(1.0, 2.0);
        let bytes = vector.bytes();
        assert_eq!(bytes.len(), 8);
        assert_eq!(&bytes[0..4], &1.0f32.to_ne_bytes());
        assert_eq!(&bytes[4..8], &2.0f32.to_ne_bytes());
    }

    /// Arithmetic operators, including assigning versions
    #[test]
    fn operators() {
        let a = Vec2::new(1.0, 2.0);
        let b = Vec2::new(3.0, 5.0);
        assert_eq!(a + b, Vec2::new(4.0, 7.0));
        assert_eq!(b - a, Vec2::new(2.0, 3.0));
        assert_eq!(a * 2.0, Vec2::new(2.0, 4.0));
        assert_eq!(2.0 * a, Vec2::new(2.0, 4.0));
        assert_eq!(-a, Vec2::new(-1.0, -2.0));

        let mut c = a;
        c += b;
        assert_eq!(c, Vec2::new(4.0, 7.0));
        c -= b;
        assert_eq!(c, a);
        c *= 3.0;
        assert_eq!(c, Vec2::new(3.0, 6.0));
    }

    /// Conversions from tuples, arrays, and library types
    #[test]
    fn conversions() {
        assert_eq!(Vec2::from((1.0, 2.0)), Vec2::new(1.0, 2.0));
        assert_eq!(Vec2::from((1usize, 2usize)), Vec2::new(1.0, 2.0));
        assert_eq!(Vec2::from([1.0, 2.0]), Vec2::new(1.0, 2.0));
        assert_eq!(Vec2::from(egui::Pos2::new(1.0, 2.0)), Vec2::new(1.0, 2.0));
        assert_eq!(Vec2::from(egui::Vec2::new(1.0, 2.0)), Vec2::new(1.0, 2.0));

        let extent = wgpu::Extent3d { width: 3, height: 4, depth_or_array_layers: 5 };
        assert_eq!(Vec2::from(extent), Vec2::new(3.0, 4.0));
    }

    /// Display formats as a coordinate pair
    #[test]
    fn display() {
        assert_eq!(Vec2::new(1.0, 2.5).to_string(), "(1, 2.5)");
    }
}
