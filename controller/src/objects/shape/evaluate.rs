//! CPU reference for how the generator turns a zone's shapes into a normal.
//! The GPU has no tests, so this is what the blending is tested against.
//! Must match core_distance, surface_height, Union, shape_gradient, rotate_from_up and evaluate_zone in generator.wgsl.

use studio_math::{Vec2, Vec3};

use super::{ShapeParams, ShapeProfile, MAX_DOME_SLOPE, dome_cap_t, hull_distance};
use crate::objects::ObjectsHandle;

/// Get the distance from a position to the edge of a shape's flat core, and the direction away from the core.
/// Both are zero inside the core.
pub fn core_distance(params: &ShapeParams, vertices: &[Vec2], pos: Vec2) -> (f32, Vec2) {
    let (hull, direction) = hull_distance(vertices, pos);
    let distance = (hull - params.inner_radius).max(0.0);
    (distance, if distance > 0.0 { direction } else { Vec2::ZERO })
}

/// Get the height of a surface a distance away from its core, and its gradient given the direction away from the core,
/// both in world pixels with +y down. The height is 0 past the surface's reach, where it meets the ground.
pub fn surface_height(params: &ShapeParams, distance: f32, direction: Vec2) -> (f32, Vec2) {
    // Height and slope of the profile, which continue below the ground past the outer radius
    let (radius, steepness, blend) = (params.outer_radius, params.steepness, params.blend);
    let t = distance / radius;
    let cap_t = dome_cap_t();
    let (height, slope) = match params.profile {
        // Soft shoulder: within a blend width of the core the cone rounds over, so its slope starts at 0 at the core's edge.
        // Past that it's the straight cone, so its foot and reach don't change.
        ShapeProfile::Cone => {
            let (ramp, ramp_slope) = if distance < blend { (0.5 * blend + distance * distance / (2.0 * blend), distance / blend) } else { (distance, 1.0) };
            (steepness * (radius - ramp), -steepness * ramp_slope)
        },
        // A quarter circle until its slope reaches the cap, then a straight line at the capped slope
        ShapeProfile::Dome if t < cap_t => {
            let root = (1.0 - t * t).sqrt();
            (steepness * radius * root, -steepness * t / root)
        },
        ShapeProfile::Dome => {
            let cap_height = steepness * radius * (1.0 - cap_t * cap_t).sqrt();
            (cap_height - steepness * MAX_DOME_SLOPE * (distance - cap_t * radius), -steepness * MAX_DOME_SLOPE)
        },
    };

    // Soft foot: a smooth maximum with the ground, which is exactly the ground once the height is a blend width below it
    let mix = (0.5 + 0.5 * height / blend).clamp(0.0, 1.0);
    (mix * height + blend * mix * (1.0 - mix), direction * (mix * slope))
}

/// The union of several surfaces, with each surface's own blend width.
/// Higher surfaces outweigh lower ones, surfaces fade in from nothing at their edge, and the order doesn't matter.
#[derive(Default)]
struct Union {
    /// Each weight is exp(height / blend) - 1, scaled down by exp(max_exponent) so it can't overflow
    max_exponent: f32,
    weight_sum: f32,
    gradient_sum: Vec2,
}

impl Union {
    /// Add a surface, from its params and the distance and direction away from its core
    fn add(&mut self, params: &ShapeParams, distance: f32, direction: Vec2) {
        let (height, gradient) = surface_height(params, distance, direction);
        if height <= 0.0 { return; }

        let exponent = height / params.blend;
        if exponent > self.max_exponent {
            let scale = (self.max_exponent - exponent).exp();
            self.weight_sum *= scale;
            self.gradient_sum *= scale;
            self.max_exponent = exponent;
        }
        let weight = (exponent - self.max_exponent).exp() - (-self.max_exponent).exp();
        self.weight_sum += weight;
        self.gradient_sum += gradient * weight;
    }

    /// The gradient of the union
    fn gradient(&self) -> Vec2 {
        if self.weight_sum <= 0.0 { Vec2::ZERO } else { self.gradient_sum * (1.0 / self.weight_sum) }
    }
}

/// Combine shapes into the gradient of the surface they make, given each shape's group, params and vertices in order.
/// A run of shapes with the same group is one surface: its cores join exactly, and it uses the first shape's params.
/// The surfaces then blend into their union.
pub fn shape_gradient<'a>(shapes: impl IntoIterator<Item = (u16, &'a ShapeParams, &'a [Vec2])>, pos: Vec2) -> Vec2 {
    let mut union = Union::default();

    // The group being joined, its params, and the closest of its cores so far
    let mut current: Option<(u16, &ShapeParams, f32, Vec2)> = None;
    for (group, params, vertices) in shapes {
        let (distance, direction) = core_distance(params, vertices, pos);
        match &mut current {
            Some((current_group, _, closest, closest_direction)) if *current_group == group => {
                if distance < *closest { (*closest, *closest_direction) = (distance, direction); }
            },
            _ => {
                if let Some((_, params, closest, direction)) = current { union.add(params, closest, direction); }
                current = Some((group, params, distance, direction));
            },
        }
    }
    if let Some((_, params, closest, direction)) = current { union.add(params, closest, direction); }

    union.gradient()
}

/// Get the normal of a surface with a gradient, in the normal map's space where +y is up
pub fn tangent_normal(gradient: Vec2) -> Vec3 {
    Vec3::new(-gradient.x, gradient.y, 1.0).normalized()
}

/// Rotate a normal by the shortest rotation that takes +z to a base normal
pub fn rotate_from_up(normal: Vec3, base: Vec3) -> Vec3 {
    let base = base.normalized();
    if base == Vec3::ZERO { return normal; }

    // The rotation is undefined for a base pointing straight back, which normal maps can't have anyway
    let (x, y, z) = (base.x, base.y, base.z.max(-0.999));
    let a = 1.0 / (1.0 + z);
    Vec3::new(
        normal.x * (1.0 - x * x * a) - normal.y * x * y * a + normal.z * x,
        -normal.x * x * y * a + normal.y * (1.0 - y * y * a) + normal.z * y,
        -normal.x * x - normal.y * y + normal.z * z,
    ).normalized()
}

impl ObjectsHandle {
    /// Get the normal the generator makes for a zone at a world space position, from its shapes and base normal.
    /// Ignores whether the position is in the zone, and how pixels on the edge are shared with other zones.
    pub fn zone_normal_at(&self, zone_id: u16, pos: Vec2) -> anyhow::Result<Vec3> {
        let mut shapes = vec![];
        for shape_id in self.zone_shape_ids(zone_id)? {
            let shape = self.get_shape_info(shape_id)?;
            if shape.is_pending() { continue; }
            shapes.push((shape.group(), shape.params(), self.shape_positions(shape_id)?));
        }

        let gradient = shape_gradient(shapes.iter().map(|(group, params, vertices)| (*group, params, &vertices[..])), pos);
        Ok(rotate_from_up(tangent_normal(gradient), self.get_zone_info(zone_id)?.normal()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::objects::test_utils::add_square;
    use super::super::SHAPE_PENDING;

    const CONE: ShapeParams = ShapeParams { profile: ShapeProfile::Cone, inner_radius: 0.0, outer_radius: 20.0, steepness: 1.0, blend: 2.0 };
    const DOME: ShapeParams = ShapeParams { profile: ShapeProfile::Dome, ..CONE };

    /// A big square zone to put shapes in
    fn zone() -> ObjectsHandle {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 200.0);
        objects
    }

    /// Add a shape to zone 0 with a new vertex at each position
    fn add(objects: &mut ObjectsHandle, positions: &[(f32, f32)], params: ShapeParams) -> u16 {
        let vertices: Vec<u16> = positions.iter().map(|&pos| objects.add_shape_vertex(0, Vec2::from(pos)).unwrap()).collect();
        objects.create_shape(0, &vertices, params, 0).unwrap()
    }

    fn normal(objects: &ObjectsHandle, x: f32, y: f32) -> Vec3 {
        objects.zone_normal_at(0, Vec2::new(x, y)).unwrap()
    }

    fn assert_near(actual: Vec3, expected: Vec3) {
        assert!(actual.distance(expected) < 1e-4, "{actual:?} != {expected:?}");
    }

    /// A zone with no shapes gets its base normal, as before shapes existed
    #[test]
    fn no_shapes() {
        let mut objects = zone();
        assert_near(normal(&objects, 50.0, 50.0), Vec3::FORWARD);
        let tilted = Vec3::new(0.6, 0.0, 0.8);
        objects.set_zone_normals(&[0], tilted).unwrap();
        assert_near(normal(&objects, 50.0, 50.0), tilted);
    }

    /// Past a shape's reach, the zone is flat
    #[test]
    fn outside_reach() {
        let mut objects = zone();
        add(&mut objects, &[(100.0, 100.0)], CONE);
        assert_near(normal(&objects, 100.0 + CONE.reach() + 0.01, 100.0), Vec3::FORWARD);
        assert_near(normal(&objects, 10.0, 10.0), Vec3::FORWARD);
    }

    /// A cone tilts away from its apex at 45 degrees, with +y flipped so faces below the apex point down in the normal map
    #[test]
    fn cone_falloff() {
        let mut objects = zone();
        add(&mut objects, &[(100.0, 100.0)], CONE);
        let tilt = std::f32::consts::FRAC_1_SQRT_2;
        assert_near(normal(&objects, 110.0, 100.0), Vec3::new(tilt, 0.0, tilt));
        assert_near(normal(&objects, 90.0, 100.0), Vec3::new(-tilt, 0.0, tilt));
        assert_near(normal(&objects, 100.0, 110.0), Vec3::new(0.0, -tilt, tilt));
        assert_near(normal(&objects, 100.0, 90.0), Vec3::new(0.0, tilt, tilt));

        // Exactly on the apex there is no direction to tilt
        assert_near(normal(&objects, 100.0, 100.0), Vec3::FORWARD);
    }

    /// Steepness is the slope of a cone
    #[test]
    fn steepness() {
        let mut objects = zone();
        add(&mut objects, &[(100.0, 100.0)], ShapeParams { steepness: 0.5, ..CONE });
        assert_near(normal(&objects, 110.0, 100.0), Vec3::new(0.5, 0.0, 1.0).normalized());
    }

    /// The core is flat, whether it's a triangle or a circle grown from a point
    #[test]
    fn plateaus() {
        let mut objects = zone();
        add(&mut objects, &[(20.0, 20.0), (80.0, 20.0), (20.0, 80.0)], CONE);
        add(&mut objects, &[(150.0, 150.0)], ShapeParams { inner_radius: 10.0, ..CONE });
        assert_near(normal(&objects, 30.0, 30.0), Vec3::FORWARD);
        assert_near(normal(&objects, 155.0, 148.0), Vec3::FORWARD);

        // Outside the circle's core it falls away from the core's edge
        let tilt = std::f32::consts::FRAC_1_SQRT_2;
        assert_near(normal(&objects, 165.0, 150.0), Vec3::new(tilt, 0.0, tilt));
    }

    /// The foot of a shape eases to flat over its blend width, so there's no crease where it meets the ground
    #[test]
    fn soft_foot() {
        let mut objects = zone();
        add(&mut objects, &[(100.0, 100.0)], CONE);
        let full = normal(&objects, 110.0, 100.0).x;
        let at_edge = normal(&objects, 120.0, 100.0).x;
        assert!(at_edge > 0.0 && at_edge < full);

        // Normals change smoothly through the foot
        let mut last = full;
        for i in 0..=100 {
            let x = normal(&objects, 115.0 + i as f32 * 0.1, 100.0).x;
            assert!(x <= last + 1e-6 && last - x < 0.05, "jump at {i}: {last} -> {x}");
            last = x;
        }
        assert_near(normal(&objects, 122.5, 100.0), Vec3::FORWARD);
    }

    /// A dome is flat on top and steepens towards its edge
    #[test]
    fn dome() {
        let mut objects = zone();
        add(&mut objects, &[(100.0, 100.0)], DOME);
        let near_top = normal(&objects, 102.0, 100.0);
        let middle = normal(&objects, 110.0, 100.0);
        let near_edge = normal(&objects, 118.0, 100.0);
        assert!(near_top.x < middle.x && middle.x < near_edge.x);

        // Halfway out, a hemisphere's normal points straight out from its center
        assert_near(middle, Vec3::new(0.5, 0.0, (0.75f32).sqrt()));
    }

    /// A cone rounds over at the edge of its core, so its slope starts at 0 there and reaches full steepness a blend width out
    #[test]
    fn soft_shoulder() {
        let params = ShapeParams { inner_radius: 10.0, ..CONE };
        let slope = |distance: f32| surface_height(&params, distance, Vec2::new(1.0, 0.0)).1.x;
        assert!(slope(0.0).abs() < 1e-6);
        assert!(slope(0.01).abs() < 0.01);
        assert!((slope(params.blend) + 1.0).abs() < 1e-6);
        assert!((slope(5.0) + 1.0).abs() < 1e-6);

        // The slope changes smoothly through the shoulder, with no step
        let mut last = slope(0.0);
        for i in 1..=100 {
            let next = slope(i as f32 * 0.03);
            assert!(next <= last + 1e-6 && last - next < 0.02, "jump at {i}: {last} -> {next}");
            last = next;
        }

        // The tip of a point cone is rounded too, with no crease across it
        let mut objects = zone();
        add(&mut objects, &[(100.0, 100.0)], CONE);
        assert!(normal(&objects, 100.2, 100.0).x < 0.1);
        assert!(normal(&objects, 99.8, 100.0).x > -0.1);
    }

    /// A dome's slope rises smoothly to its cap and stays there, with no step where its circle becomes a straight rim
    #[test]
    fn dome_rim_capped() {
        let slope = |distance: f32| -surface_height(&DOME, distance, Vec2::new(1.0, 0.0)).1.x / DOME.steepness;
        let cap_distance = dome_cap_t() * DOME.outer_radius;
        assert!((slope(cap_distance - 1e-3) - MAX_DOME_SLOPE).abs() < 0.01);
        assert!((slope(cap_distance + 1e-3) - MAX_DOME_SLOPE).abs() < 1e-4);
        for i in 0..200 {
            assert!(slope(i as f32 * 0.15) <= MAX_DOME_SLOPE + 1e-4);
        }

        // The height doesn't jump there either
        let height = |distance: f32| surface_height(&DOME, distance, Vec2::new(1.0, 0.0)).0;
        assert!((height(cap_distance - 1e-3) - height(cap_distance + 1e-3)).abs() < 0.01);
    }

    /// Where shapes of different heights overlap, the higher one decides the normal
    #[test]
    fn higher_shape_dominates() {
        let mut objects = zone();
        add(&mut objects, &[(100.0, 100.0)], ShapeParams { outer_radius: 50.0, ..CONE });
        add(&mut objects, &[(120.0, 100.0)], ShapeParams { outer_radius: 10.0, ..CONE });

        // At (115, 100) the big cone is 35 high and the small one only 5, so the big cone's slope wins
        let tilt = std::f32::consts::FRAC_1_SQRT_2;
        assert_near(normal(&objects, 115.0, 100.0), Vec3::new(tilt, 0.0, tilt));
    }

    /// Midway between two equal cones is the floor of a valley, and the crease is smooth over the blend width
    #[test]
    fn valley_between_equal_cones() {
        let mut objects = zone();
        add(&mut objects, &[(90.0, 100.0)], CONE);
        add(&mut objects, &[(110.0, 100.0)], CONE);
        assert_near(normal(&objects, 100.0, 100.0), Vec3::FORWARD);

        // Left of the crease faces away from the left cone, right of it faces away from the right cone
        assert!(normal(&objects, 99.0, 100.0).x > 0.1);
        assert!(normal(&objects, 101.0, 100.0).x < -0.1);
        // Away from the crease, the nearer cone takes over
        assert!((normal(&objects, 96.0, 100.0).x - std::f32::consts::FRAC_1_SQRT_2).abs() < 0.02);
    }

    /// A shape with a smaller blend width wins more of the crease it shares with another
    #[test]
    fn crisper_shape_wins_crease() {
        let mut objects = zone();
        add(&mut objects, &[(90.0, 100.0)], ShapeParams { blend: 0.5, ..CONE });
        add(&mut objects, &[(110.0, 100.0)], CONE);
        assert!(normal(&objects, 100.0, 100.0).x > 0.1);
    }

    /// A small shape on a taller plateau is hidden, since the surface is the union of the shapes
    #[test]
    fn hidden_under_plateau() {
        let mut objects = zone();
        add(&mut objects, &[(100.0, 100.0)], ShapeParams { inner_radius: 40.0, outer_radius: 40.0, ..CONE });
        add(&mut objects, &[(100.0, 100.0)], ShapeParams { outer_radius: 5.0, ..CONE });
        assert_near(normal(&objects, 103.0, 100.0), Vec3::FORWARD);
    }

    /// Lines sharing a vertex round off the outside of their corner
    #[test]
    fn shared_vertex_corner() {
        let mut objects = zone();
        let corner = objects.add_shape_vertex(0, Vec2::new(100.0, 100.0)).unwrap();
        let right = objects.add_shape_vertex(0, Vec2::new(150.0, 100.0)).unwrap();
        let down = objects.add_shape_vertex(0, Vec2::new(100.0, 150.0)).unwrap();
        objects.create_shape(0, &[corner, right], CONE, 0).unwrap();
        objects.create_shape(0, &[corner, down], CONE, 0).unwrap();

        // Outside the corner, both lines are closest to the shared vertex, so the surface faces away from it
        let tilt = 0.5;
        assert_near(normal(&objects, 95.0, 95.0), Vec3::new(-tilt, tilt, std::f32::consts::FRAC_1_SQRT_2));
    }

    /// Triangles in one group sharing an edge make one flat plateau, with no seam along the shared edge
    #[test]
    fn triangle_mesh() {
        let mut objects = zone();
        let vertices: Vec<u16> = [(50.0, 50.0), (150.0, 50.0), (150.0, 150.0), (50.0, 150.0)].into_iter()
            .map(|pos| objects.add_shape_vertex(0, Vec2::from(pos)).unwrap())
            .collect();
        let first = objects.create_shape(0, &[vertices[0], vertices[1], vertices[2]], CONE, 0).unwrap();
        let group = objects.get_shape_info(first).unwrap().group();
        objects.create_shape_in_group(0, &[vertices[0], vertices[2], vertices[3]], group, 0).unwrap();
        assert_near(normal(&objects, 100.0, 100.0), Vec3::FORWARD);
        assert_near(normal(&objects, 120.0, 118.0), Vec3::FORWARD);
        assert!(normal(&objects, 160.0, 100.0).x > 0.5);

        // The same triangles as separate shapes leave a soft ridge along the shared edge
        let mut separate = zone();
        add(&mut separate, &[(50.0, 50.0), (150.0, 50.0), (150.0, 150.0)], CONE);
        add(&mut separate, &[(50.0, 50.0), (150.0, 150.0), (50.0, 150.0)], CONE);
        assert!(normal(&separate, 120.0, 118.0).x > 0.1);
    }

    /// Shapes in one group join exactly, so the crease between them is sharp instead of blended
    #[test]
    fn group_crease_is_sharp() {
        let mut objects = zone();
        let first = add(&mut objects, &[(90.0, 100.0)], CONE);
        let vertex = objects.add_shape_vertex(0, Vec2::new(110.0, 100.0)).unwrap();
        objects.create_shape_in_group(0, &[vertex], objects.get_shape_info(first).unwrap().group(), 0).unwrap();

        let tilt = std::f32::consts::FRAC_1_SQRT_2;
        assert_near(normal(&objects, 99.0, 100.0), Vec3::new(tilt, 0.0, tilt));
        assert_near(normal(&objects, 101.0, 100.0), Vec3::new(-tilt, 0.0, tilt));
    }

    /// Shapes in a path still being drawn don't affect the normals
    #[test]
    fn pending_ignored() {
        let mut objects = zone();
        let vertex = objects.add_shape_vertex(0, Vec2::new(100.0, 100.0)).unwrap();
        objects.create_shape(0, &[vertex], CONE, SHAPE_PENDING).unwrap();
        assert_near(normal(&objects, 110.0, 100.0), Vec3::FORWARD);
    }

    /// The order of the shapes doesn't change the result
    #[test]
    fn order_independent() {
        let shapes = [
            (0, CONE, vec![Vec2::new(90.0, 100.0)]),
            (1, ShapeParams { blend: 4.0, ..DOME }, vec![Vec2::new(108.0, 103.0), Vec2::new(115.0, 90.0)]),
            (2, ShapeParams { inner_radius: 3.0, ..CONE }, vec![Vec2::new(100.0, 95.0)]),
        ];
        let pos = Vec2::new(101.0, 99.0);
        let forwards = shape_gradient(shapes.iter().map(|(group, params, vertices)| (*group, params, &vertices[..])), pos);
        let backwards = shape_gradient(shapes.iter().rev().map(|(group, params, vertices)| (*group, params, &vertices[..])), pos);
        assert!(forwards.distance(backwards) < 1e-5);
    }

    /// Very tall shapes don't overflow the weights
    #[test]
    fn tall_shapes() {
        let tall = ShapeParams { outer_radius: 1000.0, blend: 0.1, ..CONE };
        let shapes = [(0, tall, vec![Vec2::new(0.0, 0.0)]), (1, tall, vec![Vec2::new(10.0, 0.0)])];
        let gradient = shape_gradient(shapes.iter().map(|(group, params, vertices)| (*group, params, &vertices[..])), Vec2::new(2.0, 0.0));
        assert!(gradient.x.is_finite() && gradient.y.is_finite());
        assert!(gradient.distance(Vec2::new(-1.0, 0.0)) < 1e-4);
    }

    /// Shapes are turned onto the zone's base normal, so a flat part of the zone faces along the base
    #[test]
    fn rotated_onto_base() {
        let mut objects = zone();
        add(&mut objects, &[(100.0, 100.0)], CONE);
        let base = Vec3::new(0.0, 0.6, 0.8);
        objects.set_zone_normals(&[0], base).unwrap();
        assert_near(normal(&objects, 10.0, 10.0), base);

        // The cone's slopes turn with it, keeping their angle to the base
        let slope = normal(&objects, 110.0, 100.0);
        assert!((slope.dot(base) - std::f32::consts::FRAC_1_SQRT_2).abs() < 1e-4);
        assert!(slope.x > 0.0);
    }

    /// Rotating keeps vectors unit length and turns +z exactly onto the base
    #[test]
    fn rotation() {
        let base = Vec3::new(-0.3, 0.4, 0.5).normalized();
        assert_near(rotate_from_up(Vec3::FORWARD, base), base);
        let turned = rotate_from_up(Vec3::new(1.0, 0.0, 0.0), base);
        assert!((turned.magnitude() - 1.0).abs() < 1e-5 && turned.dot(base).abs() < 1e-5);
        assert_near(rotate_from_up(Vec3::RIGHT, Vec3::FORWARD), Vec3::RIGHT);
    }
}
