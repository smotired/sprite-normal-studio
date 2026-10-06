use studio_math::bezier::bezier_point_at;
use studio_math::{Vec2, Vec3};

use super::{ObjectBuffers, ControlPoint, ControlPointMode, Zone, MAX_OBJECT_ID};

impl ObjectBuffers {
    /// Add a zone to the zones buffer with a random (for now) offset, and return its ID.
    pub fn create_zone(&mut self) -> anyhow::Result<u16> {
        let zone_id = self.zone_count();
        if zone_id == MAX_OBJECT_ID {
            anyhow::bail!("No room to create another zone!");
        }

        self.zones.borrow_mut().items.push(Zone::new(self.point_count(), Vec3::random_on_hemisphere()));
        Ok(zone_id)
    }

    /// Add a linear control point to a zone, to have the selected ID. Does not add any siblings.
    fn insert_point_helper(&mut self, zone_id: u16, point_id: u16, position: Vec2) -> anyhow::Result<u16> {
        // Make sure it would fit in this zone
        let point_count = self.point_count();
        let zone_count = self.zone_count();
        if point_count == MAX_OBJECT_ID { anyhow::bail!("No room to create another control point!"); }
        if zone_id >= zone_count { anyhow::bail!("Zone {} does not exist!", zone_id); }

        let (start, count) = self.get_zone_info(zone_id).unwrap().range();
        if point_id < start || point_id > start + count {
            anyhow::bail!("Invalid point id {}: must be within range {}..={}", point_id, start, start + count);
        }
        
        // Create the point
        let point = ControlPoint::new_solo(point_id, zone_id, position);
        let points = &mut self.points.borrow_mut().items;
        
        // Push point IDs ahead
        let points_after = point_count - point_id;
        for i in 0..points_after {
            let index = point_count - i;
            ControlPoint::update_id(index - 1, index, points, &mut self.point_siblings.borrow_mut().items);
        }

        // Push siblings ahead
        let siblings = &mut self.point_siblings.borrow_mut().items;
        siblings.copy_within(point_id as usize..point_count as usize, point_id as usize + 1);
        siblings[point_id as usize] = point_id;

        // Push zone ranges ahead
        let zones = &mut self.zones.borrow_mut().items;
        zones[zone_id as usize].add_point();
        for zone in &mut zones[(zone_id as usize + 1)..(zone_count as usize)] {
            zone.add_offset(1);
        }

        // Add the point
        points.insert(point_id as usize, point);

        Ok(point_id)
    }

    /// Add a linear control point to a zone at `t` along the path that starts with start_id
    /// Returns a list of created IDs, where the target point will always be the first one.
    pub fn insert_point(&mut self, zone_id: u16, mut start_id: u16, t: f32) -> anyhow::Result<Vec<u16>> {
        // Make sure it would fit in this zone
        let point_count = self.point_count();
        let zone_count = self.zone_count();
        if point_count == MAX_OBJECT_ID {
            anyhow::bail!("No room to create another control point!");
        }
        if zone_id >= zone_count {
            anyhow::bail!("Zone {} does not exist!", zone_id);
        }
        let (start, count) = self.get_zone_info(zone_id).unwrap().range();
        if start_id < start || start_id >= start + count {
            anyhow::bail!("Invalid start id {}: must be within range {}..{}", start_id, start, start + count);
        }


        // Get left sync target for the path end node.
        let start_info = self.get_point_info(start_id).unwrap();
        let (start_sync_id, _) = ControlPoint::get_sync_id(start_id, true, &self.points.borrow().items)?; // TODO: what to do with flips
        let mut end_id = start + (start_id - start + 1) % count;
        let end_info = self.get_point_info(end_id).unwrap();
        let (end_sync_id, _) = ControlPoint::get_sync_id(end_id, false, &self.points.borrow().items)?; // TODO: what to do with flips
        
        // First determine how many siblings need to be created, and ensure we have room.
        let mut synced_start_ids = {
            let immut_points = &self.points.borrow().items;
            let immut_siblings = &self.point_siblings.borrow().items;
            let mut ids: Vec<u16> = ControlPoint::get_siblings(start_id, immut_siblings).into_iter()
                // Only include siblings where the endpoints sync to the same values
                .filter(|sibling_start_id| {
                    let sibling_start_id = *sibling_start_id;
                    let sibling_start_info = self.get_point_info(sibling_start_id).unwrap();

                    // If there isn't even an end sibling in this zone, exclude
                    let sibling_end_id = ControlPoint::find_in_zone(end_id, sibling_start_info.zone_id(), immut_points, immut_siblings);
                    if sibling_end_id.is_err() || sibling_end_id.as_ref().unwrap().is_none() { return false; }
                    let sibling_end_id = sibling_end_id.unwrap().unwrap();

                    // Make sure sibling end is right after sibling start
                    let (start, count) = self.get_zone_info(sibling_start_info.zone_id()).unwrap().range();
                    let projected_sibling_end_id = start + (sibling_start_id - start + 1) % count;
                    if sibling_end_id != projected_sibling_end_id { return false; } // there is something else between

                    // Determine what the start and end sync to
                    let ss_right = ControlPoint::get_sync_id(sibling_start_id, true, immut_points); // TODO: what to do with flips
                    let se_left  = ControlPoint::get_sync_id(sibling_end_id, false, immut_points); // TODO: what to do with flips
                    if ss_right.is_err() || se_left.is_err() { return false; }

                    // We must add a synced pair here IF:
                    // The start's right syncs to the same eventual thing as the real start's right
                    // AND
                    // The end's left syncs to the same eventual thing as the real end's left
                    ss_right.unwrap().0 == start_sync_id && se_left.unwrap().0 == end_sync_id
                }).collect();
            ids.sort();
            ids
        };
        let sibling_count = synced_start_ids.len();

        // Create the point
        let position = bezier_point_at(start_info.position(), start_info.right_handle(), end_info.left_handle(), end_info.position(), t);
        let mut point_id = self.insert_point_helper(zone_id, start_id + 1, position)?;

        // Increase end_id and everything in synced_start_ids if they're after the point
        if end_id >= point_id { end_id += 1; } // only doesn't hit if start_id is the last point
        for synced_start_id in &mut synced_start_ids {
            if *synced_start_id >= point_id {
                *synced_start_id += 1;
            }
        }

        // we will add the siblings backwards because it's easier. doesn't really matter.
        let mut last_sibling_id = point_id;

        // Create the synced siblings and sync them as Both
        for i in 0..sibling_count {
            let sibling_start_id = synced_start_ids[i];
            let sibling_zone_id = self.get_point_info(sibling_start_id).unwrap().zone_id();

            // Create the synced point
            let sibling_id = self.insert_point_helper(sibling_zone_id, sibling_start_id + 1, position)?;

            // Increment all tracked IDs if they're after this point
            if point_id >= sibling_id { point_id += 1; }
            if start_id >= sibling_id { start_id += 1; }
            if end_id >= sibling_id { end_id += 1; }
            for synced_start_id in &mut synced_start_ids[(i + 1)..sibling_count] {
                *synced_start_id += 1;
            }

            // Update the synced point
            self.points.borrow_mut().items[sibling_id as usize].force_synced(point_id);// we will update handles later so we don't need to do anything after this.
            self.point_siblings.borrow_mut().items[sibling_id as usize] = last_sibling_id;
            last_sibling_id = sibling_id;
        }
        self.point_siblings.borrow_mut().items[point_id as usize] = last_sibling_id;

        // Update the point to split the curve correctly (updating the siblings as well)
        // Determine mode and handles for the path to not change
        let (mode, left_handle, right_handle, start_rh, end_lh) = {
            let start_sync_point_mode = self.get_point_info(start_sync_id).unwrap().mode();
            let end_sync_point_mode = self.get_point_info(end_sync_id).unwrap().mode();

            // If both are linear, this should be linear.
            if start_sync_point_mode == ControlPointMode::Linear && end_sync_point_mode == ControlPointMode::Linear {
                (ControlPointMode::Linear, None, None, None, None)
            } 
            
            // Otherwise it should be continuous.
            else {
                let (start_r, new_l, _, new_r, end_l) = studio_math::bezier::bezier_split_at(
                    start_info.position(),
                    start_info.right_handle(),
                    end_info.left_handle(),
                    end_info.position(),
                    t
                );

                (
                    ControlPointMode::Continuous,
                    Some(new_l - position),
                    Some(new_r - position),
                    Some(start_r - start_info.position()),
                    Some(end_l - end_info.position()),
                )
            }
        };
        self.update_point(point_id, None, Some(mode), left_handle, right_handle)?; // cascades to the new watching points

        // Also update the right and left handles of previous and next points, respectively.
        // Theoretically they should be on the same lines, so even if continuous they shouldn't affect other parts of the curve.
        // Should also cascade to watching points
        if let Some(handle) = start_rh {
            self.update_point(start_id, None, None, None, Some(handle))?;
        }
        if let Some(handle) = end_lh {
            self.update_point(end_id, None, None, Some(handle), None)?;
        }

        // Return the list of created points, which is the point ID and its siblings.
        let mut created = vec![point_id];
        for sibling_id in ControlPoint::get_siblings(point_id, &self.point_siblings.borrow().items) {
            created.push(sibling_id);
        }
        Ok(created)
    }

    /// Add a linear control point to a zone and return its ID.
    /// Assumes this is a free point in the final zone. If you need to insert a point into
    /// a different zone, use insert_point.
    pub fn create_point(&mut self, zone_id: u16, position: Vec2) -> anyhow::Result<u16> {
        // Ensure we can create a point in this zone
        if zone_id != self.zone_count() - 1 { anyhow::bail!("Can only use create_point in the final zone, not {}!", zone_id); }
        if self.point_count() == MAX_OBJECT_ID { anyhow::bail!("No room to create another control point!"); }

        // Create the point
        let point_id = self.zones.borrow_mut().items[zone_id as usize].add_point();
        let point = ControlPoint::new_solo(point_id, zone_id, position);
        self.points.borrow_mut().items.push(point);
        self.point_siblings.borrow_mut().items[point_id as usize] = point_id;

        Ok(point_id)
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::{add_zone, add_square};

    /// Zones are numbered in order, and start after the existing points
    #[test]
    fn create_zone() {
        let mut objects = ObjectBuffers::headless();
        assert_eq!(objects.create_zone().unwrap(), 0);
        objects.create_point(0, Vec2::ZERO).unwrap();
        objects.create_point(0, Vec2::ONE).unwrap();

        assert_eq!(objects.create_zone().unwrap(), 1);
        assert_eq!(objects.get_zone_info(0).unwrap().range(), (0, 2));
        assert_eq!(objects.get_zone_info(1).unwrap().range(), (2, 0));
    }

    /// Created points are linear, solo, and added to the end of the zone
    #[test]
    fn create_point() {
        let mut objects = ObjectBuffers::headless();
        let zone_id = objects.create_zone().unwrap();
        assert_eq!(objects.create_point(zone_id, Vec2::new(1.0, 2.0)).unwrap(), 0);
        assert_eq!(objects.create_point(zone_id, Vec2::new(3.0, 4.0)).unwrap(), 1);

        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.position(), Vec2::new(3.0, 4.0));
        assert_eq!(point.zone_id(), zone_id);
        assert_eq!(point.mode(), ControlPointMode::Linear);
        assert_eq!(objects.get_sibling(1).unwrap(), 1);
        assert_eq!(objects.get_zone_info(zone_id).unwrap().range(), (0, 2));
    }

    /// Points can only be created in the last zone
    #[test]
    fn create_point_in_earlier_zone() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        objects.create_zone().unwrap();
        assert!(objects.create_point(0, Vec2::ZERO).is_err());
        assert_eq!(objects.point_count(), 4);
    }

    /// Inserting at the halfway point of a straight edge adds a linear point in the middle
    #[test]
    fn insert_point() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);

        let created = objects.insert_point(0, 0, 0.5).unwrap();
        assert_eq!(created, vec![1]);
        assert_eq!(objects.point_count(), 5);
        assert_eq!(objects.get_zone_info(0).unwrap().range(), (0, 5));
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(5.0, 0.0));
        assert_eq!(objects.get_point_info(1).unwrap().mode(), ControlPointMode::Linear);
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(10.0, 0.0));
        assert_eq!(objects.get_sibling(1).unwrap(), 1);
    }

    /// Inserting on the closing edge of a zone adds the point on the end
    #[test]
    fn insert_point_on_closing_edge() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);

        let created = objects.insert_point(0, 3, 0.5).unwrap();
        assert_eq!(created, vec![4]);
        assert_eq!(objects.get_point_info(4).unwrap().position(), Vec2::new(0.0, 5.0));
    }

    /// Zones after the insertion should move along and keep pointing at their own points
    #[test]
    fn insert_point_shifts_later_zones() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);

        objects.insert_point(0, 0, 0.5).unwrap();
        assert_eq!(objects.get_zone_info(1).unwrap().range(), (5, 3));
        assert_eq!(objects.get_point_info(5).unwrap().position(), Vec2::new(20.0, 0.0));
        assert_eq!(objects.get_point_info(5).unwrap().zone_id(), 1);
        assert_eq!(objects.get_sibling(7).unwrap(), 7);
    }

    /// Inserting into a curved edge splits it into two continuous halves
    #[test]
    fn insert_point_on_curve() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        objects.update_point(0, None, Some(ControlPointMode::Broken), None, Some(Vec2::new(0.0, -5.0))).unwrap();
        objects.update_point(1, None, Some(ControlPointMode::Broken), Some(Vec2::new(0.0, -5.0)), None).unwrap();

        objects.insert_point(0, 0, 0.5).unwrap();
        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.mode(), ControlPointMode::Continuous);

        // The point is on the original curve, with its handles on a line along the tangent
        assert!((point.position() - Vec2::new(5.0, -3.75)).magnitude() < 1e-4);
        let left = point.left_handle() - point.position();
        let right = point.right_handle() - point.position();
        assert!(left.cross(right).abs() < 1e-4);
        assert!(left.dot(right) < 0.0);
    }

    /// Insertion needs an existing zone and a start point inside of it
    #[test]
    fn insert_point_errors() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0)]);

        assert!(objects.insert_point(5, 0, 0.5).is_err());
        assert!(objects.insert_point(0, 4, 0.5).is_err());
        assert!(objects.insert_point(1, 0, 0.5).is_err());
        assert_eq!(objects.point_count(), 6);
    }

    /// Inserting into a shared edge adds a synced point to every zone that shares it
    #[test]
    fn insert_point_into_shared_edge() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (zone_id, _) = objects.create_branching_zone(0).unwrap();
        objects.create_point(zone_id, Vec2::new(-10.0, 5.0)).unwrap();
        objects.complete_branching_zone(0, zone_id, 3).unwrap();

        // The left edge of the square (3 -> 0) is shared by both zones
        let created = objects.insert_point(0, 3, 0.5).unwrap();
        assert_eq!(created.len(), 2);
        let zones: Vec<u16> = created.iter().map(|&id| objects.get_point_info(id).unwrap().zone_id()).collect();
        assert!(zones.contains(&0) && zones.contains(&zone_id));
        for &id in &created {
            assert_eq!(objects.get_point_info(id).unwrap().position(), Vec2::new(0.0, 5.0));
        }
    }
}
