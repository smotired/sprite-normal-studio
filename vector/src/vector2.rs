use std::fmt::Display;
use std::ops;

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vec2 { pub x: f32, pub y: f32, }

/// Basic vector2 struct
impl Vec2 {
    /// Constants
    pub const ZERO: Self = Self { x: 0.0, y: 0.0 };
    pub const ONE: Self  = Self { x: 1.0, y: 1.0 };

    /// Create a new Vec2 from x and y coords
    pub fn new(x: f32, y: f32) -> Self { Self { x, y } }

    /// Create a Vec2 on the x axis
    pub fn hz(x: f32) -> Self { Self { x, y: 0.0 } }

    /// Create a Vec2 on the y axis
    pub fn vt(y: f32) -> Self { Self { x: 0.0, y } }

    /// Create a vector with identical x and y
    pub fn square(v: f32) -> Self { Self { x: v, y: v } }

    /// Find the dot product with another Vec2
    pub fn dot(self, rhs: Self) -> f32 { self.x * rhs.x + self.y * rhs.y }

    /// Find the cross product with another Vec2 (determinant of 2D matrix)
    pub fn cross(self, rhs: Self) -> f32 { self.x * rhs.y - self.y * rhs.x }

    /// Square magnitude of this vector
    pub fn sq_magnitude(self) -> f32 { self.dot(self) }

    /// Magnitude of this vector
    pub fn magnitude(self) -> f32 { self.sq_magnitude().sqrt() }

    /// Normalized form of this vector, or 0 if it's the zero vector
    pub fn normalized(self) -> Vec2 { 
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

impl ops::Add<Vec2> for Vec2 {
    type Output = Vec2;

    fn add(self, rhs: Vec2) -> Self::Output {
        Self::Output { x: self.x + rhs.x, y: self.y + rhs.y }
    }
}

impl ops::AddAssign for Vec2 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
    }
}

impl ops::Sub<Vec2> for Vec2 {
    type Output = Vec2;

    fn sub(self, rhs: Vec2) -> Self::Output {
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
    type Output = Vec2;

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
    type Output = Vec2;

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