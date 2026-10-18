use studio_math::Vec3;

use super::Bounds;

/// A Zone is made up of a list of control points, a list of shapes, and the vertices those shapes are made from.
/// Each list is a contiguous range, and the ranges of every zone are in zone order.
/// Must match ZonePacked in assignment.wgsl, generator.wgsl and bindings.wgsl.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Zone {
    /// The base surface normal vector of this zone, which is applied over all shapes.
    normal: Vec3,

    // Start of the control points in this path in the control points list
    point_start: u16,

    // Amount of control points in this list
    point_count: u16,

    // Start of this zone's shapes in the shapes list
    shape_start: u16,

    // Amount of shapes in this zone
    shape_count: u16,

    // Start of this zone's shape vertices in the shape vertices list
    vertex_start: u16,

    // Amount of shape vertices in this zone
    vertex_count: u16,

    // WGSL rounds the struct up to the 16 byte alignment of the vec3
    _padding: [u32; 2],
}

/// A zone as it is sent to the GPU, with bounding boxes the shaders use to skip zones far from a pixel.
/// The bounds are worked out from the object lists whenever they change, so they aren't stored in the zones or the history.
/// Must match ZonePacked in assignment.wgsl, generator.wgsl and bindings.wgsl.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ZoneGpu {
    zone: Zone,

    /// Top left and bottom right corners around the zone's path, including its handles, which the curves never leave
    pub(super) path_bounds: [f32; 4],

    /// Top left and bottom right corners around everything the zone's shapes reach, or an empty box if it has none
    pub(super) shapes_bounds: [f32; 4],
}

/// Corners of a box nothing is inside, for zones with no path or no shapes
pub(super) const EMPTY_BOUNDS: [f32; 4] = [f32::MAX, f32::MAX, f32::MIN, f32::MIN];

impl ZoneGpu {
    pub fn new(zone: Zone, path: Option<Bounds>, shapes: Option<Bounds>) -> Self {
        let corners = |bounds: Option<Bounds>| bounds.map_or(EMPTY_BOUNDS, |(min, max)| [min.x, min.y, max.x, max.y]);
        Self { zone, path_bounds: corners(path), shapes_bounds: corners(shapes) }
    }
}

/// Add an offset to the start of a range, assuming there is room for the whole range in a list of up to 65536 items
fn offset_start(start: u16, count: u16, offset: i32) -> u16 {
    (start as i32 + offset).clamp(0, 65536 - count as i32) as u16
}

impl Zone {
    pub fn new(point_start: u16, shape_start: u16, vertex_start: u16, normal: Vec3) -> Self {
        Self {
            normal,
            point_start,
            point_count: 0,
            shape_start,
            shape_count: 0,
            vertex_start,
            vertex_count: 0,
            _padding: [0; 2],
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
        if self.point_count == 0 { anyhow::bail!("Can't remove points from an empty zone!"); }
        self.point_count -= 1;
        Ok(())
    }

    /// Adds an offset to the start of this zone's points, assuming we have enough room for all of them
    pub fn add_point_offset(&mut self, offset: i32) {
        self.point_start = offset_start(self.point_start, self.point_count, offset);
    }

    /// Adds an offset to the start of this zone's shapes, assuming we have enough room for all of them
    pub fn add_shape_offset(&mut self, offset: i32) {
        self.shape_start = offset_start(self.shape_start, self.shape_count, offset);
    }

    /// Adds an offset to the start of this zone's shape vertices, assuming we have enough room for all of them
    pub fn add_vertex_offset(&mut self, offset: i32) {
        self.vertex_start = offset_start(self.vertex_start, self.vertex_count, offset);
    }

    /// Move every range of this zone along, for when it is added after other objects
    pub fn offset_ranges(&mut self, points: u16, shapes: u16, vertices: u16) {
        self.add_point_offset(points as i32);
        self.add_shape_offset(shapes as i32);
        self.add_vertex_offset(vertices as i32);
    }

    /// A copy of this zone with its ranges starting somewhere else, keeping their counts
    pub fn with_starts(&self, point_start: u16, shape_start: u16, vertex_start: u16) -> Self {
        Self { point_start, shape_start, vertex_start, ..*self }
    }

    /// Get the start and count of this zone's control points
    pub fn range(&self) -> (u16, u16) { (self.point_start, self.point_count) }

    /// Get the start and count of this zone's shapes
    pub fn shapes(&self) -> (u16, u16) { (self.shape_start, self.shape_count) }

    /// Get the start and count of this zone's shape vertices
    pub fn vertices(&self) -> (u16, u16) { (self.vertex_start, self.vertex_count) }

    /// Add a shape to the end of this zone's shapes and return its new ID
    pub fn add_shape(&mut self) -> u16 {
        let shape_id = self.shape_start + self.shape_count;
        self.shape_count += 1;
        shape_id
    }

    /// Decrease the shape count
    pub fn dec_shapes(&mut self) -> anyhow::Result<()> {
        if self.shape_count == 0 { anyhow::bail!("Can't remove shapes from a zone with no shapes!"); }
        self.shape_count -= 1;
        Ok(())
    }

    /// Add a shape vertex to the end of this zone's vertices and return its new ID
    pub fn add_vertex(&mut self) -> u16 {
        let vertex_id = self.vertex_start + self.vertex_count;
        self.vertex_count += 1;
        vertex_id
    }

    /// Decrease the shape vertex count
    pub fn dec_vertices(&mut self) -> anyhow::Result<()> {
        if self.vertex_count == 0 { anyhow::bail!("Can't remove shape vertices from a zone with no shape vertices!"); }
        self.vertex_count -= 1;
        Ok(())
    }

    /// Remove every shape and shape vertex from this zone, keeping where their ranges start
    pub fn clear_shapes(&mut self) {
        self.shape_count = 0;
        self.vertex_count = 0;
    }

    /// Get the base surface normal vector of this zone.
    pub fn normal(&self) -> Vec3 { self.normal }

    /// Set the base surface normal vector of this zone.
    pub fn set_normal(&mut self, normal: Vec3) { self.normal = normal; }
}


#[cfg(test)]
mod tests {
    use super::*;

    /// The zone must match ZonePacked in the shaders, which WGSL rounds up to 32 bytes
    #[test]
    fn layout() {
        assert_eq!(std::mem::size_of::<Zone>(), 32);
    }

    /// The GPU's zone adds two vec4s of bounds after the zone, which is already a multiple of their 16 byte alignment
    #[test]
    fn gpu_layout() {
        assert_eq!(std::mem::size_of::<ZoneGpu>(), 64);
        let empty = ZoneGpu::new(Zone::new(0, 0, 0, Vec3::FORWARD), None, None);
        assert_eq!(empty.path_bounds, EMPTY_BOUNDS);
    }

    /// A new zone has no points, shapes or vertices, starting at the given indices
    #[test]
    fn new() {
        let zone = Zone::new(5, 6, 7, Vec3::FORWARD);
        assert_eq!(zone.range(), (5, 0));
        assert_eq!(zone.shapes(), (6, 0));
        assert_eq!(zone.vertices(), (7, 0));
        assert_eq!(zone.normal, Vec3::FORWARD);
    }

    /// Adding shapes and vertices returns consecutive IDs after their starts, and removing them stops at empty
    #[test]
    fn add_shapes_and_vertices() {
        let mut zone = Zone::new(0, 3, 8, Vec3::FORWARD);
        assert_eq!(zone.add_shape(), 3);
        assert_eq!(zone.add_shape(), 4);
        assert_eq!(zone.add_vertex(), 8);
        assert_eq!((zone.shapes(), zone.vertices()), ((3, 2), (8, 1)));

        assert!(zone.dec_vertices().is_ok());
        assert!(zone.dec_vertices().is_err());
        zone.clear_shapes();
        assert_eq!((zone.shapes(), zone.vertices()), ((3, 0), (8, 0)));
        assert!(zone.dec_shapes().is_err());
    }

    /// Each range is offset on its own
    #[test]
    fn offset_ranges() {
        let mut zone = Zone::new(1, 2, 3, Vec3::FORWARD);
        zone.add_shape();
        zone.offset_ranges(10, 20, 30);
        assert_eq!((zone.range(), zone.shapes(), zone.vertices()), ((11, 0), (22, 1), (33, 0)));
        zone.add_shape_offset(-100);
        zone.add_vertex_offset(-1);
        assert_eq!((zone.shapes(), zone.vertices()), ((0, 1), (32, 0)));
    }

    /// Adding points returns consecutive IDs after the start
    #[test]
    fn add_point() {
        let mut zone = Zone::new(5, 0, 0, Vec3::FORWARD);
        assert_eq!(zone.add_point(), 5);
        assert_eq!(zone.add_point(), 6);
        assert_eq!(zone.range(), (5, 2));
    }

    /// Decreasing points works until the zone is empty, then it is an error
    #[test]
    fn dec_points() {
        let mut zone = Zone::new(0, 0, 0, Vec3::FORWARD);
        zone.add_point();
        assert!(zone.dec_points().is_ok());
        assert_eq!(zone.range(), (0, 0));
        assert!(zone.dec_points().is_err());
        assert_eq!(zone.range(), (0, 0));
    }

    /// Offsets can move the zone either way, but can't push it out of the points list
    #[test]
    fn add_point_offset() {
        let mut zone = Zone::new(10, 0, 0, Vec3::FORWARD);
        for _ in 0..4 { zone.add_point(); }

        zone.add_point_offset(5);
        assert_eq!(zone.range(), (15, 4));
        zone.add_point_offset(-3);
        assert_eq!(zone.range(), (12, 4));
        zone.add_point_offset(-100);
        assert_eq!(zone.range(), (0, 4));
        zone.add_point_offset(100000);
        assert_eq!(zone.range(), (65532, 4));
    }
}
