use studio_math::Vec2;
use studio_math::bounds::{bounds_merge, bounds_of};

use super::ObjectBuffers;

/// A bounding box, as its top left and bottom right corners
pub type Bounds = (Vec2, Vec2);

/// Merge two optional bounding boxes, keeping whichever exists if only one does
pub fn merge_bounds(a: Option<Bounds>, b: Option<Bounds>) -> Option<Bounds> {
    match (a, b) {
        (Some((a0, a1)), Some((b0, b1))) => Some(bounds_merge(a0, a1, b0, b1)),
        (a, b) => a.or(b),
    }
}

impl ObjectBuffers {
    /// Get the bounding box of a zone, which is the bounding box of the control points of all its path segments.
    /// Returns None if the zone has no points.
    pub fn zone_bounds(&self, zone_id: u16) -> anyhow::Result<Option<Bounds>> {
        let (start, count) = self.get_zone_info(zone_id)?.range();
        if count == 0 { return Ok(None); }

        // Each segment runs from the previous point to this one, with the last point wrapping around to the first
        let mut bounds = None;
        let mut previous = self.get_point_info(start + count - 1)?;
        for point_id in start..start + count {
            let point = self.get_point_info(point_id)?;
            let segment = bounds_of([previous.position(), previous.right_handle(), point.left_handle(), point.position()]);
            bounds = merge_bounds(bounds, segment);
            previous = point;
        }
        Ok(bounds)
    }

    /// Get the bounding box around several zones. Returns None if none of them have points.
    pub fn zones_bounds(&self, zone_ids: &[u16]) -> anyhow::Result<Option<Bounds>> {
        let mut bounds = None;
        for &zone_id in zone_ids {
            bounds = merge_bounds(bounds, self.zone_bounds(zone_id)?);
        }
        Ok(bounds)
    }

    /// Get the bounding box around every zone that has one of the points or one of their siblings.
    /// Siblings move together, so changing a point can change all of their zones.
    pub fn bounds_around_points(&self, point_ids: &[u16]) -> anyhow::Result<Option<Bounds>> {
        let mut zone_ids = vec![];
        for &point_id in point_ids {
            zone_ids.push(self.get_point_info(point_id)?.zone_id());
            for sibling_id in self.get_siblings(point_id) {
                zone_ids.push(self.get_point_info(sibling_id)?.zone_id());
            }
        }
        zone_ids.sort();
        zone_ids.dedup();
        self.zones_bounds(&zone_ids)
    }

    /// Get the bounding box around every zone from this one to the last.
    /// Deleting a zone changes the IDs of every zone after it, so all of them need to be reassigned.
    pub fn bounds_from_zone(&self, zone_id: u16) -> anyhow::Result<Option<Bounds>> {
        let zone_ids: Vec<u16> = (zone_id..self.zone_count()).collect();
        self.zones_bounds(&zone_ids)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::{add_square, add_zone};

    /// Merging optional boxes keeps whichever exists
    #[test]
    fn merge_optional() {
        let a = (Vec2::ZERO, Vec2::ONE);
        let b = (Vec2::new(2.0, 2.0), Vec2::new(3.0, 3.0));
        assert_eq!(merge_bounds(None, None), None);
        assert_eq!(merge_bounds(Some(a), None), Some(a));
        assert_eq!(merge_bounds(None, Some(b)), Some(b));
        assert_eq!(merge_bounds(Some(a), Some(b)), Some((Vec2::ZERO, Vec2::new(3.0, 3.0))));
    }

    /// A zone's bounds cover all its points
    #[test]
    fn zone() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, -5.0), (30.0, 0.0), (25.0, 8.0)]);
        assert_eq!(objects.zone_bounds(0).unwrap(), Some((Vec2::ZERO, Vec2::new(10.0, 10.0))));
        assert_eq!(objects.zone_bounds(1).unwrap(), Some((Vec2::new(20.0, -5.0), Vec2::new(30.0, 8.0))));
        assert_eq!(objects.zones_bounds(&[0, 1]).unwrap(), Some((Vec2::new(0.0, -5.0), Vec2::new(30.0, 10.0))));
        assert!(objects.zone_bounds(2).is_err());
    }

    /// Handles that stick out of the points are included
    #[test]
    fn zone_with_handles() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        objects.update_point(1, None, Some(crate::ControlPointMode::Broken), None, Some(Vec2::new(5.0, -7.0))).unwrap();
        assert_eq!(objects.zone_bounds(0).unwrap(), Some((Vec2::new(0.0, -7.0), Vec2::new(15.0, 10.0))));
    }

    /// An empty zone has no bounds
    #[test]
    fn empty_zone() {
        let mut objects = ObjectBuffers::headless();
        let zone_id = objects.create_zone().unwrap();
        assert_eq!(objects.zone_bounds(zone_id).unwrap(), None);
    }

    /// Bounds around points include the zones of their siblings
    #[test]
    fn around_points_includes_siblings() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (zone_id, point_id) = objects.create_branching_zone(1).unwrap();
        objects.create_point(zone_id, Vec2::new(30.0, -10.0)).unwrap();
        assert_eq!(point_id, 4);

        let bounds = objects.bounds_around_points(&[1]).unwrap();
        assert_eq!(bounds, Some((Vec2::new(0.0, -10.0), Vec2::new(30.0, 10.0))));
    }

    /// Bounds from a zone cover it and every zone after it
    #[test]
    fn from_zone() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);
        add_zone(&mut objects, &[(40.0, 0.0), (50.0, 0.0), (50.0, 10.0)]);
        assert_eq!(objects.bounds_from_zone(1).unwrap(), Some((Vec2::new(20.0, 0.0), Vec2::new(50.0, 10.0))));
        assert_eq!(objects.bounds_from_zone(3).unwrap(), None);
    }
}
