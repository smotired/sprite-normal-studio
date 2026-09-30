use vector::Vec3;

/// A Zone is made up of a list of control points and a list of shapes.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Zone {
    /// The base surface normal vector of this zone, which is applied over all shapes.
    normal: Vec3,

    // Start of the control points in this path in the control points list
    point_start: u16,

    // Amount of control points in this list
    point_count: u16,
}

impl Zone {
    pub fn new(point_start: u16, normal: Vec3) -> Self {
        Self {
            point_start,
            point_count: 0,
            normal,
        }
    }

    pub fn add_point(&mut self) { self.point_count += 1; }

    /// Adds an offset to this zone's start position assuming we have enough room for all points
    pub fn add_offset(&mut self, offset: i32) {
        self.point_start = (self.point_start as i32 + offset)
            .clamp(0, 65536 - self.point_count as i32) as u16
    }
}