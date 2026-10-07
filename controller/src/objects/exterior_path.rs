use studio_math::{Vec2, bezier::bezier_signed_area};

use super::{ObjectBuffers, ControlPoint};

/// Defines an item in an exterior path that we will need to create an interior node for.
pub struct ExteriorPathSegment {
    /// The ID of the point that starts the curve.
    pub start_id: u16,

    /// The handle we are using to start the curve segment. right = true
    pub start_handle: bool,

    /// The ID of the point that ends the curve.
    pub end_id: u16,

    /// The handle we are using to end the curve segment. right = true
    pub end_handle: bool,
}

impl ExteriorPathSegment {
    pub fn new(start_id: u16, start_handle: bool, end_id: u16, end_handle: bool) -> Self {
        Self {
            start_id,
            start_handle,
            end_id,
            end_handle,
        }
    }

    pub fn flip(&mut self) {
        std::mem::swap(&mut self.start_id, &mut self.end_id);
        std::mem::swap(&mut self.start_handle, &mut self.end_handle);
    }
}

impl ObjectBuffers {
    /// Determine if two points in different zones are connected
    pub fn check_points_connected(&self, point1: u16, point2: u16) -> anyhow::Result<bool> {
        // Don't use get_connected_points, so that we can exit early if we find the one we want.

        // Helper function to get the next point in the same zone in the given direction.
        let next = |i: u16| {
            let zone_id = self.get_point_info(i).unwrap().zone_id();
            let (start, count) = self.get_zone_info(zone_id).unwrap().range();
            start + (i - start + 1) % count
        };

        // Run breadth-first search
        // Could optimize by doing depth first search since everything in a zone shares a range
        let mut visited = [false; 65536];
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(point1);
        while let Some(point_id) = queue.pop_front() {
            // Skip or mark as visited
            if visited[point_id as usize] { continue; }
            visited[point_id as usize] = true;

            // If this is our target point, the points are connected.
            if point_id == point2 { return Ok(true); }

            // Enqueue siblings
            for sibling_id in ControlPoint::get_siblings(point_id, &self.point_siblings.borrow().items) {
                queue.push_back(sibling_id);
            }

            // Enqueue next node
            queue.push_back(next(point_id));
        }

        // If the queue is exhausted the points are not connected
        Ok(false)
    }
    
    // Get the signed area of a zone
    pub fn zone_signed_area(&self, zone_id: u16, closed: bool) -> f32 {
        let (start, count) = self.get_zone_info(zone_id).unwrap().range();
        let first = self.get_point_info(start).unwrap();

        let mut prev = first;
        let mut area = 0.0;
        for i in 1..count {
            let point = self.get_point_info(start + i).unwrap();
            area += bezier_signed_area(prev.position(), prev.right_handle(), point.left_handle(), point.position());
            prev = point;
        }

        if closed {
            area += bezier_signed_area(prev.position(), prev.right_handle(), first.left_handle(), first.position());
        }
        area
    }

    /// Helper method to find an exterior path by traversing in a direction.
    /// Takes in start position, end position, and if we should initially go right form the start position.
    /// Returns a vector of path segments and a total path signed area.
    /// 
    /// Makes the following assumptions:
    /// - Connected zones do not overlap.
    /// - start_id and end_id are not on interior paths
    /// - Interior nodes are correctly only found on interior paths.
    /// - All nodes on interior paths are correctly set up as interior nodes.
    /// - A path does exist (i.e. check_points_connected was run already).
    fn exterior_path_helper(&self, start_id: u16, end_id: u16, start_right: bool) -> anyhow::Result<(Vec<ExteriorPathSegment>, f32)> {
        // Get the handle the start is syncing to, to start by going around in the start direction.
        let points = &self.points.borrow().items;
        let (mut id, mut right) = ControlPoint::get_sync_id(start_id, start_right, points)?;

        // Helper function to get the next point in the same zone in the given direction.
        let next = |i: u16, right: bool| {
            let zone_id = self.get_point_info(i).unwrap().zone_id();
            let (start, count) = self.get_zone_info(zone_id).unwrap().range();
            start + ((i - start + count) as i32 + if right { 1 } else { -1 }) as u16 % count
        };

        // Helper function to get the normalized direction of a point's handle.
        // If the handle's magnitude is 0 (e.g. the point is linear), instead returns the normalized direction to
        // the next point's opposite handle.
        let handle_dir = |i: u16, right: bool| {
            let point = self.get_point_info(i).unwrap();
            let handle = if right { point.right_handle() } else { point.left_handle() };
            let mut dir = (handle - point.position()).normalized();
            if dir.magnitude() == 0.0 {
                let neighbor = self.get_point_info(next(i, right)).unwrap();
                let handle = if right { neighbor.left_handle() } else { neighbor.right_handle() };
                dir = (handle - point.position()).normalized();
            }
            dir
        };

        // True if the edge leaving the point by the handle is shared with any other zone.
        let edge_shared = |point_id: u16, right: bool| -> anyhow::Result<bool> {
            let handle = ControlPoint::get_sync_id(point_id, right, points)?;
            for sibling_id in ControlPoint::get_siblings(point_id, &self.point_siblings.borrow().items) {
                for side in [false, true] {
                    if ControlPoint::get_sync_id(sibling_id, side, points)? == handle { return Ok(true); }
                }
            }
            Ok(false)
        };

        // Get sibling IDs of the end point, which is where we will stop.
        let end_siblings = {
            let mut siblings = ControlPoint::get_siblings(end_id, &self.point_siblings.borrow().items);
            siblings.insert(0, end_id);
            siblings
        };

        // Helper method to find the exterior angle between two vectors in the right direction.
        let sweep_angle = {
            // +1.0 = interior is on the left of travel, so sweep counter-clockwise from back_dir.
            // -1.0 = interior is on the right, so sweep clockwise.
            let start_zone_id = self.get_point_info(id).unwrap().zone_id();
            let sweep = if (self.zone_signed_area(start_zone_id, true) > 0.0) == right { 1.0 } else { -1.0 };

            move |from: Vec2, to: Vec2| -> f32 {
                let a = (sweep * from.cross(to)).atan2(from.dot(to));
                if a <= 1e-4 { a + std::f32::consts::TAU } else { a }
            }
        };
        
        // Loop until we find a sibling of the end point
        let mut path = vec![];
        let mut seen = std::collections::HashSet::new();
        while !end_siblings.contains(&id) {
            // Bail if we are in an infinite loop.
            if !seen.insert((id, right)) {
                anyhow::bail!("Exterior path revisited state ({id}, {right}) from {start_id} to {end_id}");
            }
            let base_next_id = next(id, right);
            
            // Create the path segment that starts at this point and ends at the next point
            let segment = ExteriorPathSegment::new(id, right, base_next_id, !right);
            path.push(segment);

            // Find the next point in the zone and its siblings
            let mut next_id = base_next_id;
            let mut next_handle = !right; // assume we will keep going the same way
            let next_siblings = ControlPoint::get_siblings(next_id, &self.point_siblings.borrow().items);

            // If it has no siblings, it's definitely the next one to go to.
            // Otherwise it's definitely not the next one to go to (by assumptions).
            if !next_siblings.is_empty() {
                // Get the direction of the path we just took
                let back_dir = handle_dir(base_next_id, !right);
                
                // Get the full sibling ring
                let mut ring = next_siblings.clone();
                ring.push(base_next_id);
                
                // Pick the direction with the smallest angle
                let mut best_angle = f32::MAX;
                'ring: for candidate_id in ring { // 'ring is the loop handle, so we can control target of break
                    // Reaching the end point always wins
                    if end_siblings.contains(&candidate_id) {
                        next_id = candidate_id;
                        next_handle = !right;
                        break 'ring; // do not check any other candidates
                    }

                    // Check both handles
                    for out_right in [false, true] {
                        // Skip incoming edge and interior edges
                        if candidate_id == base_next_id && out_right == !right { continue; }
                        if edge_shared(candidate_id, out_right)? { continue; }

                        // Determine the exterior angle to this branch point
                        let angle = sweep_angle(back_dir, handle_dir(candidate_id, out_right));
                        if angle < best_angle {
                            best_angle = angle;
                            next_id = candidate_id;
                            next_handle = !out_right;
                        }
                    }
                }
            }
            
            // Traverse to the next point/handle
            id = next_id;
            right = !next_handle;
        }

        // Calculate area by checking each segment
        let mut signed_area = 0.0;
        for segment in &path[..] {
            let start = self.get_point_info(segment.start_id).unwrap();
            let end = self.get_point_info(segment.end_id).unwrap();

            let pos0 = start.position();
            let pos1 = if segment.start_handle { start.right_handle() } else { start.left_handle() };
            let pos2 = if segment.end_handle { end.right_handle() } else { end.left_handle() };
            let pos3 = end.position();

            signed_area += bezier_signed_area(pos0, pos1, pos2, pos3);
        }

        Ok((path, signed_area))
    }

    /// Find an exterior path for a zone along another zone.
    /// Searches for siblings from the start point to the end point, where a continuous exterior path could
    /// be created if the points from zone_id were to be added.
    /// Returns the IDs of the nodes along the path that should have interior nodes. Also returns true if the creating zone should be flipped.
    /// Does not traverse any interior nodes, meaning the path will only be on the outside.
    /// This means that as long as start_id and end_id are both on the outside of the overall zone, there is exactly one correct path.
    /// When a branch is encountered, returns the node with one handle synced, so that we can sync directly to it and have both handles synced.
    /// Bails if a path can't be found, i.e. the zones aren't connected or either point isn't on the outside.
    /// If start or end ID have siblings, traversal will start by going down to their free node.
    pub fn find_exterior_path(&self, start_id: u16, end_id: u16, zone_id: u16) -> anyhow::Result<(Vec<ExteriorPathSegment>, bool)> {
        // Find the signed area of the part we are creating, up to the endpoint which we take as linear
        let (creating_start, creating_count) = self.get_zone_info(zone_id).unwrap().range();
        let signed_area = {
            let mut area = self.zone_signed_area(zone_id, false);
            let before_join = self.get_point_info(creating_start + creating_count - 1).unwrap();
            let end = self.get_point_info(end_id).unwrap();
            area += bezier_signed_area(before_join.position(), before_join.right_handle(), end.position(), end.position());
            area
        };

        // Find the path along the exterior from end to start, incrementing IDs.
        // This travels along right handles and doesn't require a flip. Does not include end or start.
        let (path_1, area_1) = {
            let (path, area) = self.exterior_path_helper(end_id, start_id, true)?;
            (path, (signed_area + area).abs())
        };

        // Find the path along the exterior from end to start, decrementing IDs.
        // This travels along left handles and would require a flip. Does not include end or start.
        let (mut path_2, area_2) = {
            let (path, area) = self.exterior_path_helper(end_id, start_id, false)?;
            (path, (signed_area + area).abs())
        };
        
        // If path 2 has a smaller absolute area, flip it, pick it, and mark the zone for flipping. Area will never be equal.
        let (path, flip) =
            if area_1 < area_2 {
                (path_1, false)
            } else {
                path_2.reverse();
                for s in &mut path_2[..] { s.flip() }
                (path_2, true)
            };

        Ok((path, flip))
    }

    /// Flip the zone with the requested ID. Reverses all handles and point order.
    /// Returns the new IDs of the start and end points.
    /// Should only be used for a brand new zone, that doesn't have anything synced to it.
    pub fn flip_zone(&mut self, zone_id: u16) -> anyhow::Result<(u16, u16)> {
        if zone_id != self.zone_count() - 1 { anyhow::bail!("Can only run flip_zone on the final zone, not zone {}!", zone_id); }
        if self.point_count() == super::MAX_OBJECT_ID { anyhow::bail!("No space to flip the points in a zone!"); } // we should have checked at the beginning of whatever function
        let (start, count) = self.get_zone_info(zone_id).unwrap().range();
        let points = &mut self.points.borrow_mut().items;
        let siblings = &mut self.point_siblings.borrow_mut().items;

        for i in 0..(count / 2) {
            let first_id = start + i;
            let last_id = start + count - 1 - i;

            // Flip the first point
            points[first_id as usize].flip(first_id);

            // Flip the last point
            points[last_id as usize].flip(last_id);

            // Swap the points themselves
            points.swap(last_id as usize, first_id as usize);
            siblings.swap(first_id as usize, last_id as usize);

            // Swap the sibling IDs. Use id 65535 as a swap space which we checked earlier.
            ControlPoint::update_id(last_id, 65535, points, siblings);
            ControlPoint::update_id(first_id, last_id, points, siblings);
            ControlPoint::update_id(65535, first_id, points, siblings);
        }

        // Flip middle point if it didn't get flipped
        if count % 2 == 1 {
            let middle_id = start + count / 2;
            points[middle_id as usize].flip(middle_id);
            // don't need to move it
        }

        // Resync all the handles
        for id in start..(start + count) {
            ControlPoint::resync_handles(id, points)?;
        }

        Ok((start, start + count - 1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Flipping a segment swaps its ends and which handles are used
    #[test]
    fn segment_flip() {
        let mut segment = ExteriorPathSegment::new(1, true, 2, false);
        segment.flip();
        assert_eq!((segment.start_id, segment.start_handle), (2, false));
        assert_eq!((segment.end_id, segment.end_handle), (1, true));
    }

    use studio_math::Vec2;
    use super::super::test_utils::{add_zone, add_square};

    /// Points in the same zone are always connected
    #[test]
    fn connected_in_same_zone() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        assert!(objects.check_points_connected(0, 2).unwrap());
        assert!(objects.check_points_connected(3, 3).unwrap());
    }

    /// Zones are only connected if they share points, directly or through other zones
    #[test]
    fn connected_through_siblings() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let (branch_zone, _) = objects.create_branching_zone(1).unwrap();
        objects.create_point(branch_zone, Vec2::new(20.0, 0.0)).unwrap();
        let other_zone = add_zone(&mut objects, &[(100.0, 0.0), (110.0, 0.0)]);
        let (other_start, _) = objects.get_zone_info(other_zone).unwrap().range();

        assert!(objects.check_points_connected(0, 5).unwrap());
        assert!(objects.check_points_connected(5, 3).unwrap());
        assert!(!objects.check_points_connected(0, other_start).unwrap());
        assert!(!objects.check_points_connected(other_start + 1, 5).unwrap());
    }

    /// A counter-clockwise zone has positive area, equal to its size
    #[test]
    fn zone_signed_area() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(0.0, 0.0), (0.0, 10.0), (10.0, 10.0), (10.0, 0.0)]);

        assert!((objects.zone_signed_area(0, true) - 100.0).abs() < 1e-3);
        assert!((objects.zone_signed_area(1, true) + 100.0).abs() < 1e-3);
    }

    /// An open path only sums the curves between its points, so it has less area than when closed
    #[test]
    fn zone_signed_area_open() {
        let mut objects = ObjectBuffers::headless();
        add_zone(&mut objects, &[(1.0, 0.0), (3.0, 0.0), (3.0, 2.0)]);

        assert!((objects.zone_signed_area(0, false) - 3.0).abs() < 1e-3);
        assert!((objects.zone_signed_area(0, true) - 2.0).abs() < 1e-3);
    }

    /// Flipping reverses point order, which flips the area, and returns the new start and end
    #[test]
    fn flip_zone() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0)]);

        assert_eq!(objects.flip_zone(1).unwrap(), (4, 6));
        assert_eq!(objects.get_point_info(4).unwrap().position(), Vec2::new(30.0, 10.0));
        assert_eq!(objects.get_point_info(5).unwrap().position(), Vec2::new(30.0, 0.0));
        assert_eq!(objects.get_point_info(6).unwrap().position(), Vec2::new(20.0, 0.0));
        assert!((objects.zone_signed_area(1, true) + 50.0).abs() < 1e-3);
    }

    /// Flipping swaps the handles of each point
    #[test]
    fn flip_zone_swaps_handles() {
        let mut objects = ObjectBuffers::headless();
        add_zone(&mut objects, &[(0.0, 0.0), (10.0, 0.0), (10.0, 10.0)]);
        objects.update_point(1, None, Some(crate::objects::ControlPointMode::Broken), Some(Vec2::new(-3.0, 0.0)), Some(Vec2::new(0.0, 4.0))).unwrap();

        objects.flip_zone(0).unwrap();
        let point = objects.get_point_info(1).unwrap();
        assert_eq!(point.position(), Vec2::new(10.0, 0.0));
        assert_eq!(point.left_handle(), Vec2::new(10.0, 4.0));
        assert_eq!(point.right_handle(), Vec2::new(7.0, 0.0));
    }

    /// Only the final zone can be flipped
    #[test]
    fn flip_zone_not_last() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0)]);
        assert!(objects.flip_zone(0).is_err());
    }
}
