use super::{ObjectBuffers, ControlPoint, MAX_OBJECT_ID};

impl ObjectBuffers {
    /// Add a broken control point to some other path, to create a new zone branching from this path.
    /// Returns the ID of the the created zone and point.
    pub fn create_branching_zone(&mut self, sibling_id: u16) -> anyhow::Result<(u16, u16)> {
        // Create the zone
        let zone_count = self.zone_count();
        let point_count = self.point_count();
        if zone_count == MAX_OBJECT_ID { anyhow::bail!("No room to create another zone!"); }
        if point_count == MAX_OBJECT_ID { anyhow::bail!("No room to create another control point!"); }
        if sibling_id >= point_count { anyhow::bail!("Sibling point {} does not exist!", sibling_id); }
        let zone_id = self.create_zone()?;

        // Create a point branching off the sibling point toward the right
        let point_id = point_count;
        let points = &mut self.points.borrow_mut().items;
        let point = ControlPoint::new_sibling_branch_start(point_id, zone_id, sibling_id, points, &mut self.point_siblings.borrow_mut().items);
        points.push(point);
        self.zones.borrow_mut().items[zone_id as usize].add_point();

        Ok((zone_id, point_id))
    }

    /// Add a broken control point to some other path, to create a new zone branching from this path.
    /// Returns the IDs of all points created to complete the zone.
    /// Takes in the point we first branched off of (not the sibling we created), the ID of the zone we are creating, and the ID of the point we are merging back into.
    pub fn complete_branching_zone(&mut self, source_point_id: u16, creating_zone_id: u16, sibling_id: u16) -> anyhow::Result<Vec<u16>> {
        // Validate
        let zone_count = self.zone_count();
        let point_count = self.point_count();
        if point_count == MAX_OBJECT_ID { anyhow::bail!("No room to create another control point!"); }
        if sibling_id >= point_count { anyhow::bail!("Sibling point {} does not exist!", sibling_id); }
        if source_point_id >= point_count { anyhow::bail!("Source point {} does not exist!", source_point_id); }
        if creating_zone_id >= zone_count { anyhow::bail!("Current zone {} does not exist!", creating_zone_id); }
        let source_zone_id = self.get_point_info(source_point_id).unwrap().zone_id();

        // When start = end, convert first point to a free node with a broken handle instead and don't add any other points.
        // This also means we don't have to flip
        let same_zone_id = ControlPoint::find_in_zone(sibling_id, source_zone_id, &self.points.borrow().items, &self.point_siblings.borrow().items)?;
        if let Some(sibling_id) = same_zone_id
            && source_point_id == sibling_id {
                let (first_id, _) = self.get_zone_info(creating_zone_id).unwrap().range();
                self.points.borrow_mut().items[first_id as usize].force_free(first_id);
                return Ok(vec![]);
            }

        // Ensure the points are connected
        if !self.check_points_connected(source_point_id, sibling_id)? {
            anyhow::bail!("Cannot join control points in disconnected zones! Make the paths close together and use the merge tool.");
        }

        // Find the exterior path to take and bail if there isn't room for all the points
        let (exterior_path, requires_flip) = self.find_exterior_path(source_point_id, sibling_id, creating_zone_id)?;
        if point_count >= MAX_OBJECT_ID - exterior_path.len().max(1) as u16 { anyhow::bail!("No room to create interior control points!"); }
        // max bc we need 65535 free to flip
        // and not -1 from path length because we do still need to create the join point's sibling

        // Create a point branching off the sibling point toward the left
        let mut created_point_id = {
            let created_point_id = self.zones.borrow_mut().items[creating_zone_id as usize].add_point();
            let points = &mut self.points.borrow_mut().items;

            let point = ControlPoint::new_sibling_branch_end(created_point_id, creating_zone_id, sibling_id, points, &mut self.point_siblings.borrow_mut().items);
            points.push(point); // goes on the end of the list because we are creating a zone
            created_point_id
        };

        // Flip the path we're creating if necessary
        if requires_flip {
            (created_point_id, _) = self.flip_zone(creating_zone_id)?;
        }

        // Ensure the start and end of the path point to the right places
        {
            let (start, count) = self.get_zone_info(creating_zone_id).unwrap().range();
            let points = &mut self.points.borrow_mut().items;

            let first_seg = exterior_path.first().unwrap();
            let last_seg = exterior_path.last().unwrap();
            let (out_id, out_right) = ControlPoint::get_sync_id(first_seg.start_id, first_seg.start_handle, points)?;
            let (in_id, in_right) = ControlPoint::get_sync_id(last_seg.end_id, last_seg.end_handle, points)?;

            let last_point = start + count - 1;
            ControlPoint::retarget(last_point, true, out_id, out_right, points)?;
            ControlPoint::retarget(start, false, in_id, in_right, points)?;
            ControlPoint::resync_handles(last_point, points)?;
            ControlPoint::resync_handles(start, points)?;
        }

        // Create interior points for everything along the chosen path
        let mut created = vec![created_point_id];
        let creating_zone = &mut self.zones.borrow_mut().items[creating_zone_id as usize];
        let points = &mut self.points.borrow_mut().items;

        // Add an interior control point at the start of each segment
        let mut last_segment = &exterior_path[0]; // exists if start != end which we checked earlier
        for segment in &exterior_path[1..] {

            // Add a point for the start of the segment
            let interior_id = creating_zone.add_point();
            let point = ControlPoint::new_sibling_branch_interior(
                interior_id,
                creating_zone_id,
                segment.start_id,
                last_segment.end_id,
                last_segment.end_handle,
                segment.start_id,
                segment.start_handle,
                points,
                &mut self.point_siblings.borrow_mut().items,
            );

            points.push(point);
            created.push(interior_id);

            last_segment = segment;
        }

        Ok(created)
    }
}
