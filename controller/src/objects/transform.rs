use studio_math::Vec2;
use studio_math::transform::Affine2;

use super::{ObjectsHandle, ControlPoint, ShapeParams};
use super::warp::{ShapeWarp, scale_radii};

/// Copies of the points in some zones and all their siblings, and of the zones' shapes, from before they were transformed.
/// Transforms are always applied to these originals, so they don't build up error over a drag and can be undone exactly.
#[derive(Clone, Default)]
pub struct ZonesSnapshot {
    points: Vec<(u16, ControlPoint)>,

    /// Each shape vertex in the zones and its original position
    vertices: Vec<(u16, Vec2)>,

    /// Each shape in the zones and its original params, whose radii scale with the zones
    shapes: Vec<(u16, ShapeParams)>,

    /// Zones that aren't in the snapshot, but share points with it, change shape when it is transformed, so their shapes warp to follow
    warp: ShapeWarp,
}

impl ObjectsHandle {
    /// Take a snapshot of every point and shape in some zones, and every sibling of those points.
    /// Synced handles only follow siblings, so transforming whole sibling rings keeps them in sync.
    pub fn snapshot_zones(&self, zone_ids: &[u16]) -> anyhow::Result<ZonesSnapshot> {
        let mut point_ids = vec![];
        let mut vertex_ids = vec![];
        let mut shape_ids = vec![];
        for &zone_id in zone_ids {
            let (start, count) = self.get_zone_info(zone_id)?.range();
            for point_id in start..start + count {
                point_ids.push(point_id);
                point_ids.extend(self.get_siblings(point_id));
            }
            vertex_ids.extend(self.zone_vertex_ids(zone_id)?);
            shape_ids.extend(self.zone_shape_ids(zone_id)?);
        }
        point_ids.sort();
        point_ids.dedup();

        // The siblings' zones that aren't being transformed
        let mut others = self.zones_around_points(&point_ids)?;
        others.retain(|zone_id| !zone_ids.contains(zone_id));

        Ok(ZonesSnapshot {
            points: point_ids.into_iter().map(|point_id| Ok((point_id, self.get_point_info(point_id)?))).collect::<anyhow::Result<_>>()?,
            vertices: vertex_ids.into_iter().map(|vertex_id| Ok((vertex_id, self.get_vertex_info(vertex_id)?.position()))).collect::<anyhow::Result<_>>()?,
            shapes: shape_ids.into_iter().map(|shape_id| Ok((shape_id, self.get_shape_info(shape_id)?.params()))).collect::<anyhow::Result<_>>()?,
            warp: self.begin_shape_warp(&others)?,
        })
    }

    /// Set every point and shape vertex in a snapshot to its original position, transformed, and turn its handles.
    /// Shape radii scale by the square root of how much the transform scales areas, so shapes keep their size relative to their zones.
    /// Shapes in other zones that share points with the snapshot warp to follow their zones.
    pub fn apply_transform(&mut self, snapshot: &ZonesSnapshot, transform: &Affine2) -> anyhow::Result<()> {
        {
            let points = &mut self.points.borrow_mut().items;
            for (point_id, original) in &snapshot.points {
                let Some(point) = points.get_mut(*point_id as usize) else { anyhow::bail!("Point {} does not exist!", point_id); };
                point.set_transformed(original, transform);
            }
        }
        for &(vertex_id, original) in &snapshot.vertices {
            self.set_vertex_position(vertex_id, transform.point(original))?;
        }
        {
            let scale = transform.determinant().abs().sqrt();
            let shapes = &mut self.shapes.borrow_mut().items;
            for &(shape_id, original) in &snapshot.shapes {
                let Some(shape) = shapes.get_mut(shape_id as usize) else { anyhow::bail!("Shape {} does not exist!", shape_id); };
                shape.set_params(scale_radii(original, scale));
            }
        }
        self.apply_shape_warp(&snapshot.warp)
    }

    /// Put everything in a snapshot back how it was
    pub fn restore_snapshot(&mut self, snapshot: &ZonesSnapshot) -> anyhow::Result<()> {
        self.apply_transform(snapshot, &Affine2::IDENTITY)?;
        self.restore_shape_warp(&snapshot.warp)
    }
}

#[cfg(test)]
mod tests {
    use crate::ControlPointMode;
    use super::*;
    use super::super::test_utils::{add_square, add_zone};

    /// Transforming moves points and turns their handles, and restoring puts them back
    #[test]
    fn transform_and_restore() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);
        objects.update_point(1, None, Some(ControlPointMode::Broken), Some(Vec2::new(-2.0, 0.0)), Some(Vec2::new(0.0, 3.0))).unwrap();

        let snapshot = objects.snapshot_zones(&[0]).unwrap();
        objects.apply_transform(&snapshot, &Affine2::scale_about(Vec2::new(2.0, 3.0), Vec2::ZERO)).unwrap();
        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.position(), Vec2::new(20.0, 0.0));
        assert_eq!(point.left_handle(), Vec2::new(16.0, 0.0));
        assert_eq!(point.right_handle(), Vec2::new(20.0, 9.0));
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(20.0, 30.0));
        assert_eq!(objects.get_point_info(4).unwrap().position(), Vec2::new(20.0, 0.0)); // other zone untouched

        objects.restore_snapshot(&snapshot).unwrap();
        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.position(), Vec2::new(10.0, 0.0));
        assert_eq!(point.right_handle(), Vec2::new(10.0, 3.0));
    }

    /// Siblings in other zones are included in the snapshot, so they move along with their zones
    #[test]
    fn snapshot_includes_siblings() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let (_, branch_id) = objects.create_branching_zone(1).unwrap();

        let snapshot = objects.snapshot_zones(&[0]).unwrap();
        objects.apply_transform(&snapshot, &Affine2::scale_about(Vec2::new(2.0, 2.0), Vec2::ZERO)).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(20.0, 0.0));
        assert_eq!(objects.get_point_info(branch_id).unwrap().position(), Vec2::new(20.0, 0.0));
    }

    /// Shapes in transformed zones move with them, and their radii scale with the square root of how much areas scale
    #[test]
    fn transform_shapes() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);
        let vertex = objects.add_shape_vertex(0, Vec2::new(5.0, 5.0)).unwrap();
        let params = ShapeParams { inner_radius: 1.0, ..ShapeParams::default() };
        objects.create_shape(0, &[vertex], params, 0).unwrap();
        let other = objects.add_shape_vertex(1, Vec2::new(28.0, 2.0)).unwrap();
        objects.create_shape(1, &[other], params, 0).unwrap();

        // Mirrored and scaled by 4 and 1, which scales areas by 4
        let snapshot = objects.snapshot_zones(&[0]).unwrap();
        objects.apply_transform(&snapshot, &Affine2::scale_about(Vec2::new(-4.0, 1.0), Vec2::ZERO)).unwrap();
        assert_eq!(objects.shape_positions(0).unwrap(), vec![Vec2::new(-20.0, 5.0)]);
        assert_eq!(objects.get_shape_info(0).unwrap().params(), ShapeParams { inner_radius: 2.0, outer_radius: 20.0, ..params });
        assert_eq!(objects.shape_positions(1).unwrap(), vec![Vec2::new(28.0, 2.0)]);
        assert_eq!(objects.get_shape_info(1).unwrap().params(), params);

        objects.restore_snapshot(&snapshot).unwrap();
        assert_eq!(objects.shape_positions(0).unwrap(), vec![Vec2::new(5.0, 5.0)]);
        assert_eq!(objects.get_shape_info(0).unwrap().params(), params);
    }

    /// Shapes in zones that share points with the transformed zones warp to follow them, and restoring puts them back
    #[test]
    fn warps_connected_zones() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        let (zone_id, _) = objects.create_branching_zone(1).unwrap();
        objects.create_point(zone_id, Vec2::new(20.0, 5.0)).unwrap();
        objects.complete_branching_zone(1, zone_id, 2).unwrap();
        let vertex = objects.add_shape_vertex(zone_id, Vec2::new(12.0, 5.0)).unwrap();
        objects.create_shape(zone_id, &[vertex], ShapeParams::default(), 0).unwrap();
        let before = objects.save();

        // Moving the square right squashes the triangle, pushing its shape right
        let snapshot = objects.snapshot_zones(&[0]).unwrap();
        objects.apply_transform(&snapshot, &Affine2::translate(Vec2::new(5.0, 0.0))).unwrap();
        assert!(objects.shape_positions(0).unwrap()[0].x > 14.0);

        objects.restore_snapshot(&snapshot).unwrap();
        assert_eq!(objects.save(), before);
    }

    /// Snapshots of zones that don't exist are errors
    #[test]
    fn snapshot_missing_zone() {
        let objects = ObjectsHandle::headless();
        assert!(objects.snapshot_zones(&[0]).is_err());
    }
}
