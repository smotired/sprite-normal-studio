use studio_math::Vec3;

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

    // Add a point and return the new ID (need an Insert method later)
    pub fn add_point(&mut self) -> u16 {
        let point_id = self.point_start + self.point_count;
        self.point_count += 1;
        point_id
    }

    // Decrease the point count
    pub fn dec_points(&mut self) -> anyhow::Result<()> {
        if self.point_count == 0 { anyhow::bail!("Can't remove ponits from an empty zone!"); }
        self.point_count -= 1;
        Ok(())
    }

    /// Adds an offset to this zone's start position assuming we have enough room for all points
    pub fn add_offset(&mut self, offset: i32) {
        self.point_start = (self.point_start as i32 + offset)
            .clamp(0, 65536 - self.point_count as i32) as u16
    }

    pub fn range(&self) -> (u16, u16) { (self.point_start, self.point_count) }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// A new zone has no points, starting at the given index
    #[test]
    fn new() {
        let zone = Zone::new(5, Vec3::FORWARD);
        assert_eq!(zone.range(), (5, 0));
        assert_eq!(zone.normal, Vec3::FORWARD);
    }

    /// Adding points returns consecutive IDs after the start
    #[test]
    fn add_point() {
        let mut zone = Zone::new(5, Vec3::FORWARD);
        assert_eq!(zone.add_point(), 5);
        assert_eq!(zone.add_point(), 6);
        assert_eq!(zone.range(), (5, 2));
    }

    /// Decreasing points works until the zone is empty, then it is an error
    #[test]
    fn dec_points() {
        let mut zone = Zone::new(0, Vec3::FORWARD);
        zone.add_point();
        assert!(zone.dec_points().is_ok());
        assert_eq!(zone.range(), (0, 0));
        assert!(zone.dec_points().is_err());
        assert_eq!(zone.range(), (0, 0));
    }

    /// Offsets can move the zone either way, but can't push it out of the points list
    #[test]
    fn add_offset() {
        let mut zone = Zone::new(10, Vec3::FORWARD);
        for _ in 0..4 { zone.add_point(); }

        zone.add_offset(5);
        assert_eq!(zone.range(), (15, 4));
        zone.add_offset(-3);
        assert_eq!(zone.range(), (12, 4));
        zone.add_offset(-100);
        assert_eq!(zone.range(), (0, 4));
        zone.add_offset(100000);
        assert_eq!(zone.range(), (65532, 4));
    }
}
