use studio_math::Vec2;

use super::{ObjectBuffers, ControlPoint, ControlPointMode};

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

    pub fn update_zone_position(&mut self, zone_id: u16, first_point_position: Vec2) -> anyhow::Result<()> {
        let (start, count) = self.get_zone_info(zone_id).unwrap().range();
        let delta = first_point_position - self.get_point_info(start).unwrap().position();
        for i in 0..count {
            ControlPoint::add_position_delta(start + i, delta, &mut self.points.borrow_mut().items, &self.point_siblings.borrow().items)?;
        }
        Ok(())
    }

    /// Remove a control point from the points list.
    /// Returns true if the path was deleted.
    fn delete_point_with_threshold(&mut self, point_id: u16, threshold: u16) -> anyhow::Result<bool> {
        let point = self.get_point_info(point_id).unwrap();
        let zone_id = point.zone_id();
        let (_, count) = self.get_zone_info(zone_id).unwrap().range();

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
        let (start, count) = self.get_zone_info(zone_id).unwrap().range();
        
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

        objects.update_zone_position(0, Vec2::new(5.0, 5.0)).unwrap();
        assert_eq!(objects.get_point_info(0).unwrap().position(), Vec2::new(5.0, 5.0));
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 15.0));
        assert_eq!(objects.get_point_info(4).unwrap().position(), Vec2::new(20.0, 0.0));
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
}
