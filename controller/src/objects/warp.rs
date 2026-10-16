use studio_math::Vec2;
use studio_math::bezier::bezier_point_at;
use studio_math::mvc::{interpolate, mean_value_weights};

use super::{ObjectBuffers, ShapeParams};

/// How many points each curve of a zone's path is sampled at to make its cage.
/// Fixed, so cages from before and after an edit line up corner for corner as long as the zone has the same points.
const CAGE_SAMPLES: usize = 8;

/// Limits on how much warping can scale a shape's radii, so a squashed zone can't make shapes vanish or explode
const MIN_RADIUS_SCALE: f32 = 0.1;
const MAX_RADIUS_SCALE: f32 = 10.0;

/// How shapes in some zones follow their zones' paths when the paths are edited, from mean value coordinates in each zone's cage.
/// Warping always starts from the shapes as they were when this was made, so it doesn't build up error and can be undone exactly.
#[derive(Clone, Default)]
pub struct ShapeWarp(Vec<ZoneWarp>);

/// How the shapes in one zone follow its path
#[derive(Clone)]
struct ZoneWarp {
    zone_id: u16,

    /// How many points the zone had. If that changes, its cage no longer lines up, so its shapes are left alone.
    point_count: u16,

    /// Each shape vertex, its original position, and its weights in the cage
    vertices: Vec<(u16, Vec2, Vec<f32>)>,

    /// Each group of shapes, which share their params
    groups: Vec<GroupWarp>,
}

/// How the radii of a group of shapes follow its zone's path
#[derive(Clone)]
struct GroupWarp {
    shape_ids: Vec<u16>,

    /// The params the group's shapes shared when the warp was made
    original: ShapeParams,

    /// Weights in the cage for the center of the group's vertices, and for one pixel right and one pixel down from it.
    /// How far apart those end up gives how much the cage scales areas there, which scales the radii.
    anchor: [Vec<f32>; 3],
}

impl ObjectBuffers {
    /// Get a zone's cage: points along its path, CAGE_SAMPLES for each curve
    fn zone_cage(&self, zone_id: u16) -> anyhow::Result<Vec<Vec2>> {
        let (start, count) = self.get_zone_info(zone_id)?.range();
        let mut cage = Vec::with_capacity(count as usize * CAGE_SAMPLES);
        if count == 0 { return Ok(cage); }

        let mut previous = self.get_point_info(start + count - 1)?;
        for point_id in start..start + count {
            let point = self.get_point_info(point_id)?;
            let curve = [previous.position(), previous.right_handle(), point.left_handle(), point.position()];
            for i in 0..CAGE_SAMPLES {
                cage.push(bezier_point_at(curve[0], curve[1], curve[2], curve[3], i as f32 / CAGE_SAMPLES as f32));
            }
            previous = point;
        }
        Ok(cage)
    }

    /// Get the zones of some points and all of their siblings, which are the zones whose paths change when the points do, sorted
    pub fn zones_around_points(&self, point_ids: &[u16]) -> anyhow::Result<Vec<u16>> {
        let mut zone_ids = vec![];
        for &point_id in point_ids {
            zone_ids.push(self.get_point_info(point_id)?.zone_id());
            for sibling_id in self.get_siblings(point_id) {
                zone_ids.push(self.get_point_info(sibling_id)?.zone_id());
            }
        }
        zone_ids.sort();
        zone_ids.dedup();
        Ok(zone_ids)
    }

    /// Get ready to warp the shapes in some zones to follow their paths when they are edited.
    /// Zones without shapes, or whose paths are too small to make a cage, are left out.
    pub fn begin_shape_warp(&self, zone_ids: &[u16]) -> anyhow::Result<ShapeWarp> {
        let mut zones = vec![];
        for &zone_id in zone_ids {
            let zone = self.get_zone_info(zone_id)?;
            if zone.shapes().1 == 0 { continue; }
            let cage = self.zone_cage(zone_id)?;

            // Vertices, which are skipped if the cage can't describe them, so they stay where they are
            let mut vertices = vec![];
            for vertex_id in self.zone_vertex_ids(zone_id)? {
                let position = self.get_vertex_info(vertex_id)?.position();
                if let Some(weights) = mean_value_weights(&cage, position) {
                    vertices.push((vertex_id, position, weights));
                }
            }

            // Groups, anchored at the center of their vertices
            let mut groups: Vec<GroupWarp> = vec![];
            let mut seen = vec![];
            for shape_id in self.zone_shape_ids(zone_id)? {
                let group = self.get_shape_info(shape_id)?.group();
                if seen.contains(&group) { continue; }
                seen.push(group);

                let shape_ids = self.shape_groups(&[shape_id])?;
                let mut positions = vec![];
                for &member in &shape_ids {
                    positions.extend(self.shape_positions(member)?);
                }
                let center = positions.iter().fold(Vec2::ZERO, |sum, &pos| sum + pos) * (1.0 / positions.len() as f32);
                let anchor = [center, center + Vec2::RIGHT, center + Vec2::UP].map(|pos| mean_value_weights(&cage, pos));
                let [Some(center), Some(right), Some(down)] = anchor else { continue; };
                groups.push(GroupWarp { shape_ids, original: self.get_shape_info(shape_id)?.params(), anchor: [center, right, down] });
            }

            zones.push(ZoneWarp { zone_id, point_count: zone.range().1, vertices, groups });
        }
        Ok(ShapeWarp(zones))
    }

    /// Move the shapes in a warp to follow their zones' paths as they are now, from where they were when the warp was made.
    /// Their radii scale by how much the path scales areas around them. Zones whose point count changed are left alone.
    pub fn apply_shape_warp(&mut self, warp: &ShapeWarp) -> anyhow::Result<()> {
        for zone in &warp.0 {
            if self.get_zone_info(zone.zone_id)?.range().1 != zone.point_count { continue; }
            let cage = self.zone_cage(zone.zone_id)?;

            for (vertex_id, _, weights) in &zone.vertices {
                self.set_vertex_position(*vertex_id, interpolate(&cage, weights))?;
            }

            for group in &zone.groups {
                let [center, right, down] = group.anchor.each_ref().map(|weights| interpolate(&cage, weights));
                let scale = (right - center).cross(down - center).abs().sqrt().clamp(MIN_RADIUS_SCALE, MAX_RADIUS_SCALE);
                self.set_group_radii(group, scale);
            }
        }
        Ok(())
    }

    /// Put every shape in a warp back exactly how it was when the warp was made
    pub fn restore_shape_warp(&mut self, warp: &ShapeWarp) -> anyhow::Result<()> {
        for zone in &warp.0 {
            for (vertex_id, original, _) in &zone.vertices {
                self.set_vertex_position(*vertex_id, *original)?;
            }
            for group in &zone.groups {
                self.set_group_radii(group, 1.0);
            }
        }
        Ok(())
    }

    /// Set the radii of every shape in a group to its original radii, scaled
    fn set_group_radii(&mut self, group: &GroupWarp, scale: f32) {
        let params = scale_radii(group.original, scale);
        let shapes = &mut self.shapes.borrow_mut().items;
        for &shape_id in &group.shape_ids {
            if let Some(shape) = shapes.get_mut(shape_id as usize) { shape.set_params(params); }
        }
    }
}

/// Scale a shape's inner and outer radius, keeping the outer radius in range
pub(super) fn scale_radii(params: ShapeParams, scale: f32) -> ShapeParams {
    ShapeParams {
        inner_radius: params.inner_radius * scale,
        outer_radius: (params.outer_radius * scale).max(super::shape::MIN_OUTER_RADIUS),
        ..params
    }
}

#[cfg(test)]
mod tests {
    use studio_math::transform::Affine2;

    use super::*;
    use crate::ControlPointMode;
    use super::super::test_utils::{add_square, add_zone};

    fn assert_near(actual: Vec2, expected: Vec2) {
        assert!(actual.distance(expected) < 1e-3, "{actual} != {expected}");
    }

    /// A 100 pixel square with a point shape and a line shape in it
    fn sample() -> ObjectBuffers {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 100.0);
        let point = objects.add_shape_vertex(0, Vec2::new(25.0, 25.0)).unwrap();
        objects.create_shape(0, &[point], ShapeParams::default(), 0).unwrap();
        let line: Vec<u16> = [(50.0, 75.0), (90.0, 75.0)].into_iter().map(|pos| objects.add_shape_vertex(0, Vec2::from(pos)).unwrap()).collect();
        objects.create_shape(0, &line, ShapeParams { inner_radius: 2.0, ..ShapeParams::default() }, 0).unwrap();
        objects
    }

    /// Cages have the same number of corners for each curve, starting at each point
    #[test]
    fn cage() {
        let objects = sample();
        let cage = objects.zone_cage(0).unwrap();
        assert_eq!(cage.len(), 4 * CAGE_SAMPLES);
        assert_eq!(cage[0], Vec2::new(0.0, 100.0)); // the curve into the first point starts at the last point
        assert_eq!(cage[CAGE_SAMPLES], Vec2::new(0.0, 0.0));
        assert_near(cage[CAGE_SAMPLES + CAGE_SAMPLES / 2], Vec2::new(50.0, 0.0));
    }

    /// Transforming the whole path transforms the shapes the same way, scaling radii by the square root of how much areas scale
    #[test]
    fn affine_path_change() {
        let mut objects = sample();
        let warp = objects.begin_shape_warp(&[0]).unwrap();
        let transform = Affine2::scale_about(Vec2::new(2.0, 0.5), Vec2::new(10.0, 10.0));
        for point_id in 0..4 {
            let position = objects.get_point_info(point_id).unwrap().position();
            objects.update_point(point_id, Some(transform.point(position)), None, None, None).unwrap();
        }
        objects.apply_shape_warp(&warp).unwrap();

        assert_near(objects.shape_positions(0).unwrap()[0], transform.point(Vec2::new(25.0, 25.0)));
        assert_near(objects.shape_positions(1).unwrap()[1], transform.point(Vec2::new(90.0, 75.0)));
        assert!((objects.get_shape_info(0).unwrap().params().outer_radius - 10.0).abs() < 1e-3); // 2 * 0.5 scales areas by 1
        assert!((objects.get_shape_info(1).unwrap().params().inner_radius - 2.0).abs() < 1e-3);
    }

    /// Moving one corner moves shapes near it more than shapes far from it, and restoring puts everything back exactly
    #[test]
    fn local_change_and_restore() {
        let mut objects = sample();
        let before = objects.save();
        let warp = objects.begin_shape_warp(&[0]).unwrap();
        objects.update_point(2, Some(Vec2::new(130.0, 130.0)), None, None, None).unwrap();
        objects.apply_shape_warp(&warp).unwrap();

        let near = objects.shape_positions(1).unwrap()[1] - Vec2::new(90.0, 75.0);
        let far = objects.shape_positions(0).unwrap()[0] - Vec2::new(25.0, 25.0);
        assert!(near.magnitude() > far.magnitude() * 2.0);
        assert!(objects.get_shape_info(1).unwrap().params().outer_radius > 10.0);

        objects.restore_shape_warp(&warp).unwrap();
        objects.update_point(2, Some(Vec2::new(100.0, 100.0)), None, None, None).unwrap();
        assert_eq!(objects.save(), before);
    }

    /// Curving an edge pulls the shapes near it along with it
    #[test]
    fn curved_edge() {
        let mut objects = sample();
        let warp = objects.begin_shape_warp(&[0]).unwrap();
        objects.update_point(2, None, Some(ControlPointMode::Broken), None, Some(Vec2::new(-30.0, 40.0))).unwrap();
        objects.update_point(3, None, Some(ControlPointMode::Broken), Some(Vec2::new(30.0, 40.0)), None).unwrap();
        objects.apply_shape_warp(&warp).unwrap();
        assert!(objects.shape_positions(1).unwrap()[0].y > 80.0);
        assert!(objects.shape_positions(0).unwrap()[0].y < 30.0);
    }

    /// Zones whose point count changed, or that have no shapes, are left alone
    #[test]
    fn skipped_zones() {
        let mut objects = sample();
        add_zone(&mut objects, &[(200.0, 0.0), (300.0, 0.0), (300.0, 100.0)]);
        let warp = objects.begin_shape_warp(&[0, 1]).unwrap();
        assert_eq!(warp.0.len(), 1);

        objects.insert_point(0, 0, 0.5).unwrap();
        objects.update_point(3, Some(Vec2::new(150.0, 150.0)), None, None, None).unwrap();
        objects.apply_shape_warp(&warp).unwrap();
        assert_eq!(objects.shape_positions(0).unwrap(), vec![Vec2::new(25.0, 25.0)]);
    }

    /// Zones around points include the zones of their siblings
    #[test]
    fn zones_around() {
        let mut objects = sample();
        let (zone_id, _) = objects.create_branching_zone(1).unwrap();
        objects.create_point(zone_id, Vec2::new(200.0, 0.0)).unwrap();
        assert_eq!(objects.zones_around_points(&[1]).unwrap(), vec![0, 1]);
        assert_eq!(objects.zones_around_points(&[0]).unwrap(), vec![0]);
    }
}
