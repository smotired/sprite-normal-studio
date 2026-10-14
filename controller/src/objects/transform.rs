use studio_math::transform::Affine2;

use super::{ObjectBuffers, ControlPoint};

/// Copies of the points in some zones and all their siblings, from before they were transformed.
/// Transforms are always applied to these originals, so they don't build up error over a drag and can be undone exactly.
#[derive(Clone, Default)]
pub struct PointsSnapshot(Vec<(u16, ControlPoint)>);

impl ObjectBuffers {
    /// Take a snapshot of every point in some zones, and every sibling of those points.
    /// Synced handles only follow siblings, so transforming whole sibling rings keeps them in sync.
    pub fn snapshot_zone_points(&self, zone_ids: &[u16]) -> anyhow::Result<PointsSnapshot> {
        let mut point_ids = vec![];
        for &zone_id in zone_ids {
            let (start, count) = self.get_zone_info(zone_id)?.range();
            for point_id in start..start + count {
                point_ids.push(point_id);
                point_ids.extend(self.get_siblings(point_id));
            }
        }
        point_ids.sort();
        point_ids.dedup();

        let points = point_ids.into_iter()
            .map(|point_id| Ok((point_id, self.get_point_info(point_id)?)))
            .collect::<anyhow::Result<_>>()?;
        Ok(PointsSnapshot(points))
    }

    /// Set every point in a snapshot to its original position and handles, transformed.
    pub fn apply_transform(&mut self, snapshot: &PointsSnapshot, transform: &Affine2) -> anyhow::Result<()> {
        let points = &mut self.points.borrow_mut().items;
        for (point_id, original) in &snapshot.0 {
            let Some(point) = points.get_mut(*point_id as usize) else { anyhow::bail!("Point {} does not exist!", point_id); };
            point.set_transformed(original, transform);
        }
        Ok(())
    }

    /// Put every point in a snapshot back where it was
    pub fn restore_snapshot(&mut self, snapshot: &PointsSnapshot) -> anyhow::Result<()> {
        self.apply_transform(snapshot, &Affine2::IDENTITY)
    }
}

#[cfg(test)]
mod tests {
    use studio_math::Vec2;

    use crate::ControlPointMode;
    use super::*;
    use super::super::test_utils::{add_square, add_zone};

    /// Transforming moves points and turns their handles, and restoring puts them back
    #[test]
    fn transform_and_restore() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);
        objects.update_point(1, None, Some(ControlPointMode::Broken), Some(Vec2::new(-2.0, 0.0)), Some(Vec2::new(0.0, 3.0))).unwrap();

        let snapshot = objects.snapshot_zone_points(&[0]).unwrap();
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
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (_, branch_id) = objects.create_branching_zone(1).unwrap();

        let snapshot = objects.snapshot_zone_points(&[0]).unwrap();
        objects.apply_transform(&snapshot, &Affine2::scale_about(Vec2::new(2.0, 2.0), Vec2::ZERO)).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(20.0, 0.0));
        assert_eq!(objects.get_point_info(branch_id).unwrap().position(), Vec2::new(20.0, 0.0));
    }

    /// Snapshots of zones that don't exist are errors
    #[test]
    fn snapshot_missing_zone() {
        let objects = ObjectBuffers::headless();
        assert!(objects.snapshot_zone_points(&[0]).is_err());
    }
}
