use studio_math::Vec2;

/// Most vertices a shape can have. One makes a point, two a line, and three a triangle.
pub const MAX_SHAPE_VERTICES: usize = 3;

/// Smallest outer radius a shape can have, so its falloff always has some width
pub const MIN_OUTER_RADIUS: f32 = 0.5;

/// Smallest steepness a shape can have, so it always rises above the ground
pub const MIN_STEEPNESS: f32 = 0.01;

/// Smallest blend width a shape can have, so the weights in the union never divide by zero
pub const MIN_BLEND: f32 = 0.1;

/// Largest slope of a dome, which would otherwise be vertical at its edge. About 83 degrees.
/// Must match MAX_DOME_SLOPE in generator.wgsl and helpers.wgsl.
pub const MAX_DOME_SLOPE: f32 = 8.0;

/// Flag for shapes in a path that is still being drawn. They are drawn in the overlay but not used for normals, and not saved.
pub const SHAPE_PENDING: u8 = 1 << 0;

/// How the surface of a shape falls from its core to its outer radius
#[repr(u8)]
#[derive(Copy, Clone, PartialEq, Eq, Debug, Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ShapeProfile {
    /// Falls in a straight line, like the side of a cone
    #[default]
    Cone = 0,

    /// Falls in a quarter circle, like the side of a dome
    Dome = 1,
}

impl From<u8> for ShapeProfile {
    fn from(value: u8) -> Self {
        match value {
            1 => Self::Dome,
            _ => Self::Cone, // fallback for any unexpected value
        }
    }
}

impl From<ShapeProfile> for u8 {
    fn from(value: ShapeProfile) -> Self { value as u8 }
}

/// Everything about a shape other than its zone and vertices
#[derive(Copy, Clone, PartialEq, Debug)]
pub struct ShapeParams {
    pub profile: ShapeProfile,

    /// How far the flat core extends out from the hull of the vertices
    pub inner_radius: f32,

    /// How far the surface falls from the edge of the core to the ground
    pub outer_radius: f32,

    /// Height of the shape as a ratio of its outer radius, so 1 makes 45 degree cones
    pub steepness: f32,

    /// Width in pixels of the soft edge where the shape meets the ground and other shapes
    pub blend: f32,
}

impl Default for ShapeParams {
    fn default() -> Self {
        Self { profile: ShapeProfile::Cone, inner_radius: 0.0, outer_radius: 10.0, steepness: 1.0, blend: 2.0 }
    }
}

impl ShapeParams {
    /// Check that every value is finite and within its range
    pub fn validate(&self) -> anyhow::Result<()> {
        let values = [self.inner_radius, self.outer_radius, self.steepness, self.blend];
        if values.iter().any(|value| !value.is_finite()) { anyhow::bail!("Shape values must be finite numbers!"); }
        if self.inner_radius < 0.0 { anyhow::bail!("Inner radius can't be negative!"); }
        if self.outer_radius < MIN_OUTER_RADIUS { anyhow::bail!("Outer radius must be at least {MIN_OUTER_RADIUS}!"); }
        if self.steepness < MIN_STEEPNESS { anyhow::bail!("Steepness must be at least {MIN_STEEPNESS}!"); }
        if self.blend < MIN_BLEND { anyhow::bail!("Blend must be at least {MIN_BLEND}!"); }
        Ok(())
    }

    /// Get these params with one value changed, clamped into its range
    pub fn with(mut self, change: ShapeParamChange) -> Self {
        match change {
            ShapeParamChange::Profile(profile) => self.profile = profile,
            ShapeParamChange::InnerRadius(radius) => self.inner_radius = radius.max(0.0),
            ShapeParamChange::OuterRadius(radius) => self.outer_radius = radius.max(MIN_OUTER_RADIUS),
            ShapeParamChange::Steepness(steepness) => self.steepness = steepness.max(MIN_STEEPNESS),
            ShapeParamChange::Blend(blend) => self.blend = blend.max(MIN_BLEND),
        }
        self
    }

    /// How far from the hull the shape affects normals, past its outer radius by however far its soft edge reaches.
    /// Must match shape_reach in generator.wgsl and helpers.wgsl.
    pub fn reach(&self) -> f32 {
        let end_slope = match self.profile {
            ShapeProfile::Cone => 1.0,
            ShapeProfile::Dome => MAX_DOME_SLOPE,
        };
        self.inner_radius + self.outer_radius + self.blend / (self.steepness * end_slope)
    }
}

/// One change to the params of shapes
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum ShapeParamChange {
    Profile(ShapeProfile),
    InnerRadius(f32),
    OuterRadius(f32),
    Steepness(f32),
    Blend(f32),
}

/// A primitive inside a zone that loosely defines its surface. Made from 1 to 3 of the zone's shape vertices.
/// Must match ShapePacked in generator.wgsl and bindings.wgsl.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub struct Shape {
    /// The zone this shape is a part of
    zone_id: u16,

    /// The profile of the shape. Use ShapeProfile::from and u8::from.
    profile: u8,

    /// Extra flags for this shape, like SHAPE_PENDING
    flags: u8,

    /// The vertices of this shape, as their index in the zone's vertex range plus one. 0 means there is no vertex.
    /// The first slot is always used, and used slots always come before unused ones.
    vertices: [u16; MAX_SHAPE_VERTICES],

    _padding: u16,

    inner_radius: f32,
    outer_radius: f32,
    steepness: f32,
    blend: f32,
}

impl Shape {
    /// Create a shape from the local indices of its vertices in its zone, which must be 1 to 3 different vertices.
    pub fn new(zone_id: u16, local_vertices: &[u16], params: ShapeParams, flags: u8) -> anyhow::Result<Self> {
        params.validate()?;
        let mut shape = Self {
            zone_id,
            profile: 0,
            flags,
            vertices: [0; MAX_SHAPE_VERTICES],
            _padding: 0,
            inner_radius: 0.0,
            outer_radius: 0.0,
            steepness: 0.0,
            blend: 0.0,
        };
        shape.set_params(params);
        shape.set_local_vertices(local_vertices)?;
        Ok(shape)
    }

    /// Get the zone ID of this shape.
    pub fn zone_id(&self) -> u16 { self.zone_id }

    /// Set the zone ID. Should be called when zone ordering changes.
    pub fn set_zone_id(&mut self, zone_id: u16) { self.zone_id = zone_id; }

    /// Get the indices of this shape's vertices in its zone's vertex range
    pub fn local_vertices(&self) -> Vec<u16> {
        self.vertices.iter().take_while(|&&slot| slot != 0).map(|&slot| slot - 1).collect()
    }

    /// Set the indices of this shape's vertices in its zone's vertex range. There must be 1 to 3 of them, all different.
    pub fn set_local_vertices(&mut self, local_vertices: &[u16]) -> anyhow::Result<()> {
        if local_vertices.is_empty() || local_vertices.len() > MAX_SHAPE_VERTICES {
            anyhow::bail!("A shape must have 1 to {MAX_SHAPE_VERTICES} vertices, not {}!", local_vertices.len());
        }
        for (i, vertex) in local_vertices.iter().enumerate() {
            if local_vertices[..i].contains(vertex) { anyhow::bail!("A shape can't use the same vertex twice!"); }
            if *vertex == u16::MAX { anyhow::bail!("Vertex index {vertex} is too large!"); }
        }

        self.vertices = [0; MAX_SHAPE_VERTICES];
        for (slot, &vertex) in self.vertices.iter_mut().zip(local_vertices) {
            *slot = vertex + 1;
        }
        Ok(())
    }

    /// Returns true if this shape uses a vertex, by its index in the zone's vertex range
    pub fn uses_local(&self, local_vertex: u16) -> bool { self.local_vertices().contains(&local_vertex) }

    /// Get everything about this shape other than its zone and vertices
    pub fn params(&self) -> ShapeParams {
        ShapeParams {
            profile: ShapeProfile::from(self.profile),
            inner_radius: self.inner_radius,
            outer_radius: self.outer_radius,
            steepness: self.steepness,
            blend: self.blend,
        }
    }

    /// Set everything about this shape other than its zone and vertices
    pub fn set_params(&mut self, params: ShapeParams) {
        self.profile = u8::from(params.profile);
        self.inner_radius = params.inner_radius;
        self.outer_radius = params.outer_radius;
        self.steepness = params.steepness;
        self.blend = params.blend;
    }

    /// Returns true if this shape is part of a path that is still being drawn
    pub fn is_pending(&self) -> bool { self.flags & SHAPE_PENDING != 0 }

    /// Set if this shape is part of a path that is still being drawn
    pub fn set_pending(&mut self, pending: bool) {
        if pending { self.flags |= SHAPE_PENDING; } else { self.flags &= !SHAPE_PENDING; }
    }

    /// Shift the zone this shape refers to, for when it is added after other zones
    pub(crate) fn offset_zone(&mut self, zone_offset: u16) { self.zone_id += zone_offset; }
}

/// A position a shape can be made from. Shapes in the same zone can share them.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Debug, Default, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ShapeVertex {
    position: Vec2,
}

impl ShapeVertex {
    pub fn new(position: Vec2) -> Self { Self { position } }

    pub fn position(&self) -> Vec2 { self.position }

    pub fn set_position(&mut self, position: Vec2) { self.position = position; }
}

/// Get the signed distance from a position to the hull of 1 to 3 vertices (a point, a line, or a triangle),
/// and the direction from the closest point on the hull to the position.
/// Inside a triangle the distance to its edges is negative, and the direction is zero. The direction is also zero exactly on the hull.
/// Must match hull_distance in generator.wgsl and helpers.wgsl.
pub fn hull_distance(vertices: &[Vec2], pos: Vec2) -> (f32, Vec2) {
    match *vertices {
        [a] => away_from(a, pos),
        [a, b] => away_from(closest_on_segment(a, b, pos), pos),
        [a, b, c] => {
            // Distance to the closest edge
            let (distance, direction) = [(a, b), (b, c), (c, a)].into_iter()
                .map(|(start, end)| away_from(closest_on_segment(start, end, pos), pos))
                .min_by(|x, y| x.0.total_cmp(&y.0))
                .unwrap();

            // Inside if the position is on the same side of every edge, whichever way the triangle winds
            let sides = [(b - a).cross(pos - a), (c - b).cross(pos - b), (a - c).cross(pos - c)];
            let inside = sides.iter().all(|&side| side >= 0.0) || sides.iter().all(|&side| side <= 0.0);
            if inside { (-distance, Vec2::ZERO) } else { (distance, direction) }
        },
        _ => (f32::INFINITY, Vec2::ZERO),
    }
}

/// Get the closest point to a position on a line segment
fn closest_on_segment(start: Vec2, end: Vec2, pos: Vec2) -> Vec2 {
    let segment = end - start;
    let length_squared = segment.sq_magnitude();
    if length_squared <= 0.0 { return start; }
    let t = ((pos - start).dot(segment) / length_squared).clamp(0.0, 1.0);
    start + segment * t
}

/// Get the distance from a point to a position, and the direction from the point to the position, or zero if they are the same
fn away_from(point: Vec2, pos: Vec2) -> (f32, Vec2) {
    let offset = pos - point;
    let distance = offset.magnitude();
    (distance, if distance > 1e-6 { offset * (1.0 / distance) } else { Vec2::ZERO })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn assert_near(actual: (f32, Vec2), expected: (f32, Vec2)) {
        assert!((actual.0 - expected.0).abs() < 1e-5 && actual.1.distance(expected.1) < 1e-5, "{actual:?} != {expected:?}");
    }

    /// Shapes and vertices must match ShapePacked and the vertices array in the shaders
    #[test]
    fn layout() {
        assert_eq!(std::mem::size_of::<Shape>(), 28);
        assert_eq!(std::mem::size_of::<ShapeVertex>(), 8);
    }

    /// Vertices are stored one higher than their index, so 0 means no vertex
    #[test]
    fn local_vertices() {
        let shape = Shape::new(2, &[0, 4], ShapeParams::default(), 0).unwrap();
        assert_eq!(shape.vertices, [1, 5, 0]);
        assert_eq!(shape.local_vertices(), vec![0, 4]);
        assert!(shape.uses_local(4) && !shape.uses_local(1));
        assert_eq!(shape.zone_id(), 2);
    }

    /// Shapes need 1 to 3 different vertices
    #[test]
    fn invalid_vertices() {
        let params = ShapeParams::default();
        assert!(Shape::new(0, &[], params, 0).is_err());
        assert!(Shape::new(0, &[0, 1, 2, 3], params, 0).is_err());
        assert!(Shape::new(0, &[1, 1], params, 0).is_err());
        assert!(Shape::new(0, &[0, 1, 2], params, 0).is_ok());
    }

    /// Params read back what was set, and must be in range
    #[test]
    fn params() {
        let params = ShapeParams { profile: ShapeProfile::Dome, inner_radius: 3.0, outer_radius: 7.0, steepness: 0.5, blend: 1.0 };
        let shape = Shape::new(0, &[0], params, 0).unwrap();
        assert_eq!(shape.params(), params);

        assert!(Shape::new(0, &[0], ShapeParams { outer_radius: 0.0, ..params }, 0).is_err());
        assert!(Shape::new(0, &[0], ShapeParams { inner_radius: -1.0, ..params }, 0).is_err());
        assert!(Shape::new(0, &[0], ShapeParams { steepness: f32::NAN, ..params }, 0).is_err());
        assert!(Shape::new(0, &[0], ShapeParams { blend: 0.0, ..params }, 0).is_err());
    }

    /// Changing a param clamps it into its range
    #[test]
    fn param_changes() {
        let params = ShapeParams::default();
        assert_eq!(params.with(ShapeParamChange::OuterRadius(-5.0)).outer_radius, MIN_OUTER_RADIUS);
        assert_eq!(params.with(ShapeParamChange::InnerRadius(-5.0)).inner_radius, 0.0);
        assert_eq!(params.with(ShapeParamChange::Profile(ShapeProfile::Dome)).profile, ShapeProfile::Dome);
        assert_eq!(params.with(ShapeParamChange::Steepness(2.0)).steepness, 2.0);
    }

    /// Reach covers the core, the falloff, and the soft edge, which is shorter for steeper ends
    #[test]
    fn reach() {
        let cone = ShapeParams { inner_radius: 2.0, outer_radius: 10.0, steepness: 0.5, blend: 2.0, profile: ShapeProfile::Cone };
        assert_eq!(cone.reach(), 16.0);
        let dome = ShapeParams { profile: ShapeProfile::Dome, ..cone };
        assert_eq!(dome.reach(), 12.5);
    }

    /// Pending flag can be set and cleared
    #[test]
    fn pending() {
        let mut shape = Shape::new(0, &[0], ShapeParams::default(), SHAPE_PENDING).unwrap();
        assert!(shape.is_pending());
        shape.set_pending(false);
        assert!(!shape.is_pending());
    }

    /// Distance to a point or line points away from it, and is zero on it
    #[test]
    fn hull_distance_point_and_line() {
        let a = Vec2::new(0.0, 0.0);
        let b = Vec2::new(10.0, 0.0);
        assert_near(hull_distance(&[a], Vec2::new(3.0, 4.0)), (5.0, Vec2::new(0.6, 0.8)));
        assert_near(hull_distance(&[a], a), (0.0, Vec2::ZERO));
        assert_near(hull_distance(&[a, b], Vec2::new(5.0, -2.0)), (2.0, Vec2::new(0.0, -1.0)));
        assert_near(hull_distance(&[a, b], Vec2::new(13.0, 4.0)), (5.0, Vec2::new(0.6, 0.8)));
        assert_near(hull_distance(&[a, b], Vec2::new(5.0, 0.0)), (0.0, Vec2::ZERO));
    }

    /// Distance inside a triangle is negative whichever way it winds, and outside points away from the closest edge
    #[test]
    fn hull_distance_triangle() {
        let clockwise = [Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0), Vec2::new(0.0, 10.0)];
        let counter = [clockwise[0], clockwise[2], clockwise[1]];
        for triangle in [clockwise, counter] {
            assert_near(hull_distance(&triangle, Vec2::new(1.0, 2.0)), (-1.0, Vec2::ZERO));
            assert_near(hull_distance(&triangle, Vec2::new(5.0, -3.0)), (3.0, Vec2::new(0.0, -1.0)));
            assert_near(hull_distance(&triangle, Vec2::new(-4.0, -3.0)), (5.0, Vec2::new(-0.8, -0.6)));
        }
    }
}
