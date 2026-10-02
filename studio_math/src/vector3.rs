use std::fmt::Display;
use std::ops;

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Vec3 { pub x: f32, pub y: f32, pub z: f32 }

/// Basic vector2 struct
impl Vec3 {
    /// Constants
    pub const ZERO:    Self = Self { x:  0.0, y:  0.0, z:  0.0 };
    pub const ONE:     Self = Self { x:  1.0, y:  1.0, z:  1.0 };
    pub const UP:      Self = Self { x:  0.0, y:  1.0, z:  0.0 };
    pub const DOWN:    Self = Self { x:  0.0, y: -1.0, z:  0.0 };
    pub const RIGHT:   Self = Self { x:  1.0, y:  0.0, z:  0.0 };
    pub const LEFT:    Self = Self { x: -1.0, y:  0.0, z:  0.0 };
    pub const FORWARD: Self = Self { x:  0.0, y:  0.0, z:  1.0 };
    pub const BACK:    Self = Self { x:  0.0, y:  0.0, z: -1.0 };

    /// Create a new Vec3 from x, y, z coords
    pub fn new(x: f32, y: f32, z: f32) -> Self { Self { x, y, z } }

    /// Create a new Vec3 from x and y coords
    pub fn plane(x: f32, y: f32) -> Self { Self { x, y, z: 0.0 } }

    /// Create a Vec3 on the x axis
    pub fn x(x: f32) -> Self { Self { x, y: 0.0, z: 0.0 } }

    /// Create a Vec3 on the y axis
    pub fn y(y: f32) -> Self { Self { x: 0.0, y, z: 0.0 } }

    /// Create a Vec3 on the z axis
    pub fn z(z: f32) -> Self { Self { x: 0.0, y: 0.0, z } }

    /// Create a vector with identical x, y, and z
    pub fn cube(v: f32) -> Self { Self { x: v, y: v, z: v } }

    /// Create a random normalized vector (uniformly distributed on unit sphere)
    pub fn random_on_sphere() -> Self {
        let cos_theta = rand::random_range(-1.0 .. 1.0);
        let phi   = rand::random_range(0.0 .. crate::M_PI);
        Self {
            x: cos_theta * phi.sin(),
            y: cos_theta.acos().sin() * phi.sin(),
            z: phi.cos(),
        }
    }

    /// Create a random normalized vector (uniformly distributed on unit hemisphere with non-negative Z)
    pub fn random_on_hemisphere() -> Self {
        let mut result = Self::random_on_sphere();
        result.z = result.z.abs();
        result
    }

    /// Find the dot product with another Vec3
    pub fn dot(self, rhs: Self) -> f32 { self.x * rhs.x + self.y * rhs.y + self.z * rhs . z }

    /// Find the cross product with another Vec3
    pub fn cross(self, rhs: Self) -> Self { 
        Self {
            x: self.y * rhs.z - rhs.y * self.z,
            y: rhs.x * self.z - self.x * rhs.z,
            z: self.x * rhs.y - rhs.x * self.y,
        }
    }

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

    /// Find any vector orthagonal to this vector. If self is 0, this also returns 0.
    pub fn any_orthag(self) -> Self { Self { x: self.y + self.z, y: self.z - self.x, z: -self.x - self.y } }

    /// Square distance from the point represented by this vector to the point represented by another vector
    pub fn sq_distance(self, rhs: Self) -> f32 { (rhs - self).sq_magnitude() }

    /// Distance from the point represented by this vector to the point represented by another vector
    pub fn distance(self, rhs: Self) -> f32 { (rhs - self).magnitude() }

    /// Linear interpolation between two vectors according to a value between 0 and 1
    pub fn lerp(lhs: Self, rhs: Self, t: f32) -> Vec3 { lhs + t.clamp(0.0, 1.0) * (rhs - lhs) }

    /// Convert to bytes for writing into a buffer
    pub fn bytes(&self) -> &[u8] { bytemuck::bytes_of(self) }
}

impl ops::Add<Self> for Vec3 {
    type Output = Vec3;

    fn add(self, rhs: Self) -> Self::Output {
        Self::Output { x: self.x + rhs.x, y: self.y + rhs.y, z: self.z + rhs.z }
    }
}

impl ops::AddAssign for Vec3 {
    fn add_assign(&mut self, rhs: Self) {
        self.x += rhs.x;
        self.y += rhs.y;
        self.z += rhs.z;
    }
}

impl ops::Sub<Self> for Vec3 {
    type Output = Self;

    fn sub(self, rhs: Self) -> Self::Output {
        Self::Output { x: self.x - rhs.x, y: self.y - rhs.y, z: self.z - rhs.z }
    }
}

impl ops::SubAssign for Vec3 {
    fn sub_assign(&mut self, rhs: Self) {
        self.x -= rhs.x;
        self.y -= rhs.y;
        self.z -= rhs.z;
    }
}

impl ops::Mul<f32> for Vec3 {
    type Output = Self;

    fn mul(self, rhs: f32) -> Self::Output {
        Self::Output { x: self.x * rhs, y: self.y * rhs, z: self.z * rhs }
    }
}

impl ops::MulAssign<f32> for Vec3 {
    fn mul_assign(&mut self, rhs: f32) {
        self.x *= rhs;
        self.y *= rhs;
        self.z *= rhs;
    }
}

impl ops::Mul<Vec3> for f32 {
    type Output = Vec3;

    fn mul(self, rhs: Vec3) -> Self::Output {
        Self::Output { x: self * rhs.x, y: self * rhs.y, z: self * rhs.z }
    }
}

impl ops::Neg for Vec3 {
    type Output = Self;

    fn neg(self) -> Self::Output { self * -1.0 }
}

impl From<(f32, f32, f32)> for Vec3 {
    fn from(value: (f32, f32, f32)) -> Self {
        Self { x: value.0, y: value.1, z: value.2 }
    }
}

impl From<(usize, usize, usize)> for Vec3 {
    fn from(value: (usize, usize, usize)) -> Self {
        Self { x: value.0 as f32, y: value.1 as f32, z: value.2 as f32 }
    }
}

impl From<[f32; 2]> for Vec3 {
    fn from(value: [f32; 2]) -> Self {
        Self { x: value[0], y: value[1], z: 0.0 }
    }
}

impl From<[f32; 3]> for Vec3 {
    fn from(value: [f32; 3]) -> Self {
        Self { x: value[0], y: value[1], z: value[2] }
    }
}

impl From<egui::Pos2> for Vec3 {
    fn from(value: egui::Pos2) -> Self {
        Self { x: value.x, y: value.y, z: 0.0 }
    }
}

impl From<egui::Vec2> for Vec3 {
    fn from(value: egui::Vec2) -> Self {
        Self { x: value.x, y: value.y, z: 0.0 }
    }
}

impl From<wgpu::Extent3d> for Vec3 {
    fn from(value: wgpu::Extent3d) -> Self {
        Self { x: value.width as f32, y: value.height as f32, z: value.depth_or_array_layers as f32 }
    }
}

impl Display for Vec3 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.z)
    }
}