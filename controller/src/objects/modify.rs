use studio_math::{Vec2, Vec3};

use super::{ObjectBuffers, ControlPoint, ControlPointMode, one_per_sibling_ring};

impl ObjectBuffers {
    pub fn update_point(&mut self, point_id: u16, position: Option<Vec2>, mode: Option<ControlPointMode>, left_handle: Option<Vec2>, right_handle: Option<Vec2>) -> anyhow::Result<()> {
        let points = &mut self.points.borrow_mut().items;
        let siblings = &self.point_siblings.borrow().items;
        if let Some(position) = position {
            ControlPoint::set_position(point_id, position, points, siblings)?;
        }
        if let Some(mode) = mode {
            ControlPoint::set_handle_mode(point_id, mode, points, siblings)?;
        }
        
        let mode = mode.unwrap_or(points[point_id as usize].mode());

        if let Some(left_handle) = left_handle {
            ControlPoint::set_left_handle(point_id, left_handle, points, siblings)?;
            if right_handle.is_none() && mode == ControlPointMode::Continuous {
                let current_right_handle = points[point_id as usize].right_handle() - points[point_id as usize].position();
                ControlPoint::set_right_handle(point_id, -left_handle.normalized() * current_right_handle.magnitude(), points, siblings)?;
            }
        }
        if let Some(right_handle) = right_handle {
            ControlPoint::set_right_handle(point_id, right_handle, points, siblings)?;
            if left_handle.is_none() && mode == ControlPointMode::Continuous {
                let current_left_handle = points[point_id as usize].left_handle() - points[point_id as usize].position();
                ControlPoint::set_left_handle(point_id, -right_handle.normalized() * current_left_handle.magnitude(), points, siblings)?;
            }
        }

        Ok(())
    }

    /// Updates the position of a list of zones, based on a new position for the very first point in that range.
    pub fn move_zones(&mut self, zone_ids: &[u16], first_point_position: Vec2) -> anyhow::Result<()> {
        if zone_ids.is_empty() { return Ok(()); }

        // Get the ID of the very first point
        let first_id = {
            let zone_id = *zone_ids.iter().min().unwrap();
            self.get_zone_info(zone_id)?.range().0
        };

        // Determine how much to move it
        let delta = first_point_position - self.get_point_info(first_id)?.position();

        // Get all point IDs in all zones
        let mut point_ids = vec![];
        for &zone_id in zone_ids {
            let (start, count) = self.get_zone_info(zone_id)?.range();
            point_ids.extend(start..start+count);
        }
        self.move_points(&point_ids[..], delta)
    }

    /// Move a list of points by a delta. Siblings move together, so each set of siblings is only moved once
    /// no matter how many of them are in the list.
    pub fn move_points(&mut self, point_ids: &[u16], delta: Vec2) -> anyhow::Result<()> {
        let siblings = &self.point_siblings.borrow().items;

        // Get one copy of each set of siblings
        let targets = one_per_sibling_ring(point_ids, siblings);

        // Update all the points, which also updates their siblings
        for point_id in targets {
            ControlPoint::add_position_delta(point_id, delta, &mut self.points.borrow_mut().items, siblings)?;
        }
        Ok(())
    }

    /// Remove a control point from the points list.
    /// Returns true if the path was deleted.
    fn delete_point_with_threshold(&mut self, point_id: u16, threshold: u16) -> anyhow::Result<bool> {
        let point = self.get_point_info(point_id)?;
        let zone_id = point.zone_id();
        let (_, count) = self.get_zone_info(zone_id)?.range();

        // If there are two or fewer points, delete the whole zone instead, as that's the minimum requirement for a path.
        if count.max(1) <= threshold { self.delete_zone(zone_id)?; return Ok(true); }
        let zone_id = zone_id as usize;

        let zone_count = self.zone_count() as usize;
        let zones = &mut self.zones.borrow_mut().items;
        
        ControlPoint::remove_point(point_id, &mut self.points.borrow_mut().items, &mut self.point_siblings.borrow_mut().items)?; // updates the list
        zones[zone_id].dec_points()?;
        
        // Pull the rest of the points and zones backwards
        for zone in &mut zones[(zone_id + 1)..zone_count] {
            zone.add_offset(-1);
        }

        Ok(false)
    }

    pub fn delete_point(&mut self, point_id: u16) -> anyhow::Result<()> {
        self.delete_point_with_threshold(point_id, 2)?;
        Ok(())
    }

    pub fn delete_point_in_wip_path(&mut self, point_id: u16) -> anyhow::Result<bool> {
        self.delete_point_with_threshold(point_id, 1)
    }

    /// Remove a whole zone and all its points.
    pub fn delete_zone(&mut self, zone_id: u16) -> anyhow::Result<()> {
        let (start, count) = self.get_zone_info(zone_id)?.range();
        
        // Remove points backwards to preserve indices
        for i in 0..count {
            let point_id = start + count - 1 - i;
            self.delete_point_with_threshold(point_id, 0)?; // threshold 0 means it won't just call this method again
        }

        // Remove the zone from the list, and pull the rest of them backward
        let new_zone_count = self.zone_count() as usize - 1;
        let zones = &mut self.zones.borrow_mut().items;
        let points = &mut self.points.borrow_mut().items;
        zones.remove(zone_id as usize);
        for (i, zone) in zones.iter().enumerate().take(new_zone_count).skip(zone_id as usize) {
            // The zone should already have had its range updated
            let (start, count) = zone.range();
            for j in 0..count {
                points[(start + j) as usize].set_zone_id(i as u16);
            }
        }

        Ok(())
    }

    /// Get the IDs of the points before and after a point in its zone.
    pub fn get_neighbors(&self, point_id: u16) -> anyhow::Result<(u16, u16)> {
        let zone_id = self.get_point_info(point_id)?.zone_id();
        let (start, count) = self.get_zone_info(zone_id)?.range();
        let index = point_id - start;
        Ok((start + (index + count - 1) % count, start + (index + 1) % count))
    }

    /// Get the mode of the point that a point's handles sync to, or None if its two handles sync to different points.
    pub fn get_point_mode(&self, point_id: u16) -> anyhow::Result<Option<ControlPointMode>> {
        let points = &self.points.borrow().items;
        let (left_root, _) = ControlPoint::get_sync_id(point_id, false, points)?;
        let (right_root, _) = ControlPoint::get_sync_id(point_id, true, points)?;
        Ok((left_root == right_root).then(|| points[left_root as usize].mode()))
    }

    /// Change the handle mode of a point, which changes the point its handles sync to.
    /// Converting to continuous lines up the handles, and gives the point new handles if it's missing one.
    pub fn set_point_mode(&mut self, point_id: u16, mode: ControlPointMode) -> anyhow::Result<()> {
        let root_id = {
            let points = &self.points.borrow().items;
            let (left_root, _) = ControlPoint::get_sync_id(point_id, false, points)?;
            let (right_root, _) = ControlPoint::get_sync_id(point_id, true, points)?;
            if left_root != right_root { anyhow::bail!("Can't change the handle mode of a point whose handles sync to different points!"); }
            left_root
        };

        let (prev_id, next_id) = self.get_neighbors(root_id)?;
        let points = &mut self.points.borrow_mut().items;
        let siblings = &self.point_siblings.borrow().items;
        ControlPoint::convert_mode(root_id, mode, prev_id, next_id, points, siblings)
    }

    /// Set the base surface normal vector of every listed zone.
    pub fn set_zone_normals(&mut self, zone_ids: &[u16], normal: Vec3) -> anyhow::Result<()> {
        let zone_count = self.zone_count();
        let zones = &mut self.zones.borrow_mut().items;
        for &zone_id in zone_ids {
            if zone_id >= zone_count { anyhow::bail!("Zone {} does not exist!", zone_id); }
            zones[zone_id as usize].set_normal(normal);
        }
        Ok(())
    }

    /// Separate the listed points from their unlisted siblings, so they can be moved independently.
    /// Listed siblings stay siblings with each other, and unlisted siblings stay siblings with each other.
    pub fn separate_points(&mut self, point_ids: &[u16]) -> anyhow::Result<()> {
        let points = &mut self.points.borrow_mut().items;
        let siblings = &mut self.point_siblings.borrow_mut().items;
        ControlPoint::separate_siblings(point_ids, points, siblings)
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::{add_zone, add_square};

    /// Position changes should apply to the point
    #[test]
    fn update_point_position() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);

        objects.update_point(2, Some(Vec2::new(12.0, 13.0)), None, None, None).unwrap();
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(12.0, 13.0));
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(10.0, 0.0));
    }

    /// Mode and handle changes should apply to the point
    #[test]
    fn update_point_mode_and_handles() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);

        objects.update_point(1, None, Some(ControlPointMode::Broken), Some(Vec2::new(-2.0, 0.0)), Some(Vec2::new(0.0, 3.0))).unwrap();
        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.mode(), ControlPointMode::Broken);
        assert_eq!(point.left_handle(), Vec2::new(8.0, 0.0));
        assert_eq!(point.right_handle(), Vec2::new(10.0, 3.0));
    }

    /// Continuous points keep the opposite handle on the line, with its own length
    #[test]
    fn update_point_continuous_mirrors_handle() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        objects.update_point(1, None, Some(ControlPointMode::Continuous), Some(Vec2::new(-2.0, 0.0)), Some(Vec2::new(4.0, 0.0))).unwrap();

        objects.update_point(1, None, None, None, Some(Vec2::new(0.0, 5.0))).unwrap();
        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.right_handle(), Vec2::new(10.0, 5.0));
        assert_eq!(point.left_handle(), Vec2::new(10.0, -2.0));

        objects.update_point(1, None, None, Some(Vec2::new(-3.0, 0.0)), None).unwrap();
        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.left_handle(), Vec2::new(7.0, 0.0));
        assert_eq!(point.right_handle(), Vec2::new(15.0, 0.0));
    }

    /// Moving a zone moves its first point to the target, and everything else along with it
    #[test]
    fn update_zone_position() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0)]);

        objects.move_zones(&[0], Vec2::new(5.0, 5.0)).unwrap();
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(5.0, 5.0));
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 15.0));
        assert_eq!(objects.get_point_info(4).unwrap().position(), Vec2::new(20.0, 0.0));
    }

    /// Moving a connected zone only moves siblings once
    #[test]
    fn update_connected_zone_positions() {
        let mut objects = ObjectBuffers::headless();
        
        add_square(&mut objects, 10.0);
        let (branching_zone, _) = objects.create_branching_zone(2).unwrap();
        objects.create_point(branching_zone, Vec2::new(5.0, 15.0)).unwrap();
        objects.complete_branching_zone(2, branching_zone, 3).unwrap();

        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0)]);

        objects.move_zones(&[0, 1], Vec2::new(5.0, 5.0)).unwrap();
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(5.0, 5.0));
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(15.0, 5.0));
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 15.0));
        assert_eq!(objects.get_point_info(3).unwrap().position(), Vec2::new(5.0, 15.0));
        assert_eq!(objects.get_point_info(6).unwrap().position(), Vec2::new(15.0, 15.0));
        assert_eq!(objects.get_point_info(5).unwrap().position(), Vec2::new(10.0, 20.0));
        assert_eq!(objects.get_point_info(4).unwrap().position(), Vec2::new(5.0, 15.0));
        assert_eq!(objects.get_point_info(7).unwrap().position(), Vec2::new(20.0, 0.0));
    }

    /// Moving points moves each set of siblings exactly once, even if several of them are listed
    #[test]
    fn move_points_with_siblings() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (branching_zone, branch_id) = objects.create_branching_zone(1).unwrap();
        objects.create_point(branching_zone, Vec2::new(20.0, 0.0)).unwrap();

        objects.move_points(&[1, branch_id, 1, 2], Vec2::new(5.0, 2.0)).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(15.0, 2.0));
        assert_eq!(objects.get_point_info(branch_id).unwrap().position(), Vec2::new(15.0, 2.0));
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 12.0));
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::ZERO);
    }

    /// Deleting a point removes it and shifts later points and zones back
    #[test]
    fn delete_point() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0)]);

        objects.delete_point(1).unwrap();
        assert_eq!(objects.object_counts(), (2, 5));
        assert_eq!(objects.get_zone_info(0).unwrap().range(), (0, 3));
        assert_eq!(objects.get_zone_info(1).unwrap().range(), (3, 2));
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(10.0, 10.0));
        assert_eq!(objects.get_point_info(3).unwrap().position(), Vec2::new(20.0, 0.0));
        assert_eq!(objects.get_point_info(3).unwrap().zone_id(), 1);
    }

    /// A zone with only two points is deleted entirely when one of its points is
    #[test]
    fn delete_point_deletes_small_zone() {
        let mut objects = ObjectBuffers::headless();
        add_zone(&mut objects, &[(0.0, 0.0), (1.0, 0.0)]);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);

        objects.delete_point(0).unwrap();
        assert_eq!(objects.object_counts(), (1, 3));
        assert_eq!(objects.get_zone_info(0).unwrap().range(), (0, 3));
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(20.0, 0.0));
        assert_eq!(objects.get_point_info(0).unwrap().zone_id(), 0);
    }

    /// A path still being created is only deleted once its last point is
    #[test]
    fn delete_point_in_wip_path() {
        let mut objects = ObjectBuffers::headless();
        add_zone(&mut objects, &[(0.0, 0.0), (1.0, 0.0)]);

        assert!(!objects.delete_point_in_wip_path(1).unwrap());
        assert_eq!(objects.object_counts(), (1, 1));
        assert!(objects.delete_point_in_wip_path(0).unwrap());
        assert_eq!(objects.object_counts(), (0, 0));
    }

    /// Deleting a zone removes its points, and renumbers the zones after it
    #[test]
    fn delete_zone() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);
        add_zone(&mut objects, &[(40.0, 0.0), (50.0, 0.0), (50.0, 10.0)]);

        objects.delete_zone(0).unwrap();
        assert_eq!(objects.object_counts(), (2, 6));
        assert_eq!(objects.get_zone_info(0).unwrap().range(), (0, 3));
        assert_eq!(objects.get_zone_info(1).unwrap().range(), (3, 3));
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(20.0, 0.0));
        assert_eq!(objects.get_point_info(3).unwrap().position(), Vec2::new(40.0, 0.0));
        assert_eq!(objects.get_point_info(0).unwrap().zone_id(), 0);
        assert_eq!(objects.get_point_info(5).unwrap().zone_id(), 1);
        assert_eq!(objects.get_sibling(4).unwrap(), 4);
    }

    /// Deleting a zone should leave the zones it shared points with intact, but unsynced
    #[test]
    fn delete_zone_with_siblings() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (zone_id, _) = objects.create_branching_zone(1).unwrap();
        objects.create_point(zone_id, Vec2::new(20.0, 0.0)).unwrap();

        objects.delete_zone(zone_id).unwrap();
        assert_eq!(objects.object_counts(), (1, 4));
        for id in 0..4 {
            assert_eq!(objects.get_sibling(id).unwrap(), id);
        }
    }

    /// Zone normals can be set for several zones at once, but not for zones that don't exist
    #[test]
    fn set_zone_normals() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);

        objects.set_zone_normals(&[0, 1], Vec3::RIGHT).unwrap();
        assert_eq!(objects.get_zone_info(0).unwrap().normal(), Vec3::RIGHT);
        assert_eq!(objects.get_zone_info(1).unwrap().normal(), Vec3::RIGHT);
        assert!(objects.set_zone_normals(&[2], Vec3::UP).is_err());
    }

    /// Separated points keep their positions but no longer move with their old siblings
    #[test]
    fn separate_points() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (zone_id, point_id) = objects.create_branching_zone(1).unwrap();
        assert!(objects.can_separate(&[point_id]));

        objects.separate_points(&[point_id]).unwrap();
        assert!(!objects.can_separate(&[point_id]));
        assert!(objects.get_siblings(1).is_empty());
        assert_eq!(objects.sibling_in_zone(1, zone_id), None);
    }

    /// Get a point's handles relative to its position
    fn handles(objects: &ObjectBuffers, point_id: u16) -> (Vec2, Vec2) {
        let point = objects.get_point_info(point_id).unwrap();
        (point.left_handle() - point.position(), point.right_handle() - point.position())
    }

    fn assert_near(a: Vec2, b: Vec2) {
        assert!(a.distance(b) < 1e-4, "{a} != {b}");
    }

    /// Make a square with broken handles on point 1, which is at (10, 0) between (0, 0) and (10, 10)
    fn square_with_handles(left: Vec2, right: Vec2) -> ObjectBuffers {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        objects.update_point(1, None, Some(ControlPointMode::Broken), Some(left), Some(right)).unwrap();
        objects
    }

    /// Converting to linear only changes the mode, so the handles come back when converting away from it
    #[test]
    fn mode_to_linear_keeps_handles() {
        let mut objects = square_with_handles(Vec2::new(-3.0, 0.0), Vec2::new(0.0, 4.0));

        objects.set_point_mode(1, ControlPointMode::Linear).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().mode(), ControlPointMode::Linear);
        assert_eq!(handles(&objects, 1), (Vec2::ZERO, Vec2::ZERO));

        objects.set_point_mode(1, ControlPointMode::Broken).unwrap();
        assert_eq!(handles(&objects, 1), (Vec2::new(-3.0, 0.0), Vec2::new(0.0, 4.0)));
    }

    /// Handles that are already on a line stay as they are when converting to continuous
    #[test]
    fn mode_continuous_collinear_unchanged() {
        let mut objects = square_with_handles(Vec2::new(-3.0, 0.0), Vec2::new(5.0, 0.0));
        objects.set_point_mode(1, ControlPointMode::Continuous).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().mode(), ControlPointMode::Continuous);
        assert_eq!(handles(&objects, 1), (Vec2::new(-3.0, 0.0), Vec2::new(5.0, 0.0)));
    }

    /// Broken handles are lined up keeping their lengths, without either handle flipping around, for corners turning either way
    #[test]
    fn mode_continuous_aligns_broken_handles() {
        let corners = [
            (Vec2::new(-4.0, 0.0), Vec2::new(0.0, 3.0)),
            (Vec2::new(-4.0, 0.0), Vec2::new(0.0, -3.0)),
            (Vec2::new(4.0, 0.0), Vec2::new(0.0, -3.0)),
            (Vec2::new(0.0, 4.0), Vec2::new(2.0, -3.0)),
        ];
        for (left, right) in corners {
            let mut objects = square_with_handles(left, right);
            objects.set_point_mode(1, ControlPointMode::Continuous).unwrap();
            let (new_left, new_right) = handles(&objects, 1);

            assert!((new_left.magnitude() - left.magnitude()).abs() < 1e-4);
            assert!((new_right.magnitude() - right.magnitude()).abs() < 1e-4);
            assert!(new_left.cross(new_right).abs() < 1e-3 && new_left.dot(new_right) < 0.0);
            assert!(new_left.dot(left) > 0.0 && new_right.dot(right) > 0.0, "{left} {right} -> {new_left} {new_right}");
        }
    }

    /// A linear point with no handles gets new ones halfway to its neighbors, then lined up
    #[test]
    fn mode_continuous_from_zero_handles() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);

        objects.set_point_mode(1, ControlPointMode::Continuous).unwrap();
        let direction = Vec2::new(1.0, 1.0).normalized();
        let (left, right) = handles(&objects, 1);
        assert_near(left, -direction * 5.0);
        assert_near(right, direction * 5.0);
    }

    /// Handles pointing the same way are lined up along the direction from the previous point to the next point
    #[test]
    fn mode_continuous_cusp_fallback() {
        let mut objects = square_with_handles(Vec2::new(3.0, 0.0), Vec2::new(5.0, 0.0));
        objects.set_point_mode(1, ControlPointMode::Continuous).unwrap();

        let direction = Vec2::new(1.0, 1.0).normalized();
        let (left, right) = handles(&objects, 1);
        assert_near(left, -direction * 3.0);
        assert_near(right, direction * 5.0);
    }

    /// Make two squares where point 5 is a sibling of point 1, with its left handle synced to point 1's left handle
    fn synced_squares() -> ObjectBuffers {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (10.0, 0.0), (20.0, 10.0)]);
        {
            let siblings = &mut objects.point_siblings.borrow_mut().items;
            (siblings[1], siblings[5]) = (5, 1);
        }
        ControlPoint::retarget(5, false, 1, false, &mut objects.points.borrow_mut().items).unwrap();
        objects
    }

    /// Changing the mode of a point that syncs to another point changes the mode of that point
    #[test]
    fn mode_cascades_to_sync_root() {
        let mut objects = synced_squares();
        ControlPoint::retarget(5, true, 1, true, &mut objects.points.borrow_mut().items).unwrap();
        objects.update_point(1, None, Some(ControlPointMode::Broken), None, None).unwrap();

        objects.set_point_mode(5, ControlPointMode::Linear).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().mode(), ControlPointMode::Linear);
        assert_eq!(objects.get_point_mode(5).unwrap(), Some(ControlPointMode::Linear));
    }

    /// Points whose handles sync to different points have no mode, and it can't be changed
    #[test]
    fn mode_different_roots_errors() {
        let mut objects = synced_squares();
        assert_eq!(objects.get_point_mode(5).unwrap(), None);
        assert!(objects.set_point_mode(5, ControlPointMode::Linear).is_err());
    }
}
