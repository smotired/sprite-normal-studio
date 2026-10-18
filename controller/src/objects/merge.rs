use std::collections::{BTreeMap, HashMap};

use studio_math::Vec2;

use super::{ObjectsHandle, ControlPoint};

/// A run of merged points in one zone that collapses into its first point
struct Collapse {
    /// The point that is kept
    first: u16,

    /// The other points in the run, which are deleted
    others: Vec<u16>,

    /// The new handles of the kept point, relative to it
    left_handle: Vec2,
    right_handle: Vec2,
}

impl ObjectsHandle {
    /// Merge points into a single group of siblings at their average position, along with all of their siblings.
    /// Points in the same zone must be next to each other, and they collapse into one point.
    /// Zones where every point is merged are deleted. Returns the IDs of the merged points.
    pub fn merge_points(&mut self, selected: &[u16]) -> anyhow::Result<Vec<u16>> {
        // Get every point being merged, and the sibling rings they come from
        let mut group: Vec<u16> = vec![];
        let mut rings = vec![];
        for &point_id in selected {
            self.get_point_info(point_id)?;
            if group.contains(&point_id) { continue; }
            let mut ring = vec![point_id];
            ring.extend(self.get_siblings(point_id));
            group.extend(&ring);
            rings.push(ring);
        }
        if rings.len() < 2 { anyhow::bail!("Select points from more than one group to merge them!"); }

        // Each ring counts once towards the average, even if its zone is deleted
        let mut sum = Vec2::ZERO;
        for ring in &rings {
            sum += self.get_point_info(ring[0])?.position();
        }
        let average = sum * (1.0 / rings.len() as f32);

        // Find the zones where every point is merged, and make sure the merged points in the rest are next to each other
        let mut deleted_zones = vec![];
        let zone_members = self.members_by_zone(&group)?;
        for (&zone_id, members) in &zone_members {
            let (start, count) = self.get_zone_info(zone_id)?.range();
            if members.len() == count as usize { deleted_zones.push(zone_id); }
            else if members.len() > 1 { self.find_collapse(members, start, count)?; }
        }
        if deleted_zones.len() == zone_members.len() { anyhow::bail!("Merging these points would delete every zone they're in!"); }

        // Delete whole zones, from the highest zone down so the lower ones keep their IDs
        deleted_zones.sort();
        let mut tracked = group;
        for &zone_id in deleted_zones.iter().rev() {
            let (start, count) = self.get_zone_info(zone_id)?.range();
            self.delete_zone(zone_id)?;
            remove_tracked(&mut tracked, start, count);
        }

        // Find the runs to collapse with the new IDs, from the highest zone down so the lower ones keep their IDs
        let mut collapses = vec![];
        for (&zone_id, members) in self.members_by_zone(&tracked)?.iter().rev() {
            let (start, count) = self.get_zone_info(zone_id)?.range();
            if members.len() > 1 { collapses.push(self.find_collapse(members, start, count)?); }
        }

        for collapse in collapses {
            {
                let points = &mut self.points.borrow_mut().items;
                let siblings = &mut self.point_siblings.borrow_mut().items;
                ControlPoint::separate_siblings(&[collapse.first], points, siblings)?;
                ControlPoint::set_handles_with_mode(collapse.first, collapse.left_handle, collapse.right_handle, points, siblings)?;
            }

            let mut others = collapse.others;
            others.sort();
            for &point_id in others.iter().rev() {
                // The zone may have been deleted along with an earlier point, if it got too small
                if !tracked.contains(&point_id) { continue; }
                self.delete_point_tracking(point_id, &mut tracked)?;
            }
        }
        if tracked.is_empty() { anyhow::bail!("Merging these points would delete every zone they're in!"); }

        // Link everything into one sibling ring at the average position
        {
            let points = &mut self.points.borrow_mut().items;
            let siblings = &mut self.point_siblings.borrow_mut().items;
            for (i, &point_id) in tracked.iter().enumerate() {
                siblings[point_id as usize] = tracked[(i + 1) % tracked.len()];
            }
            ControlPoint::set_position(tracked[0], average, points, siblings)?;
        }

        self.sync_merged_handles(&tracked)?;
        Ok(tracked)
    }

    /// Sort points by the zone they're in.
    fn members_by_zone(&self, point_ids: &[u16]) -> anyhow::Result<BTreeMap<u16, Vec<u16>>> {
        let mut zone_members: BTreeMap<u16, Vec<u16>> = BTreeMap::new();
        for &point_id in point_ids {
            zone_members.entry(self.get_point_info(point_id)?.zone_id()).or_default().push(point_id);
        }
        Ok(zone_members)
    }

    /// Find the run of merged points in a zone, which must all be next to each other.
    fn find_collapse(&self, members: &[u16], start: u16, count: u16) -> anyhow::Result<Collapse> {
        let next = |point_id: u16| start + (point_id - start + 1) % count;
        let prev = |point_id: u16| start + (point_id - start + count - 1) % count;

        // The run starts at the member that comes after a point that isn't merged
        let Some(&first) = members.iter().find(|&&point_id| !members.contains(&prev(point_id))) else {
            anyhow::bail!("Can't merge points that have unselected points between them on a path!");
        };
        let mut run = vec![first];
        while members.contains(&next(*run.last().unwrap())) && run.len() < members.len() {
            run.push(next(*run.last().unwrap()));
        }
        if run.len() != members.len() { anyhow::bail!("Can't merge points that have unselected points between them on a path!"); }

        // The kept point gets the outer handles of the run
        let first_info = self.get_point_info(first)?;
        let last_info = self.get_point_info(*run.last().unwrap())?;
        Ok(Collapse {
            first,
            others: run[1..].to_vec(),
            left_handle: first_info.left_handle() - first_info.position(),
            right_handle: last_info.right_handle() - last_info.position(),
        })
    }

    /// Delete a point, and update a list of point IDs to match.
    /// Removes the point from the list, or its whole zone if the zone was deleted with it, and shifts later IDs back.
    pub(super) fn delete_point_tracking(&mut self, point_id: u16, tracked: &mut Vec<u16>) -> anyhow::Result<()> {
        let zone_id = self.get_point_info(point_id)?.zone_id();
        let (start, count) = self.get_zone_info(zone_id)?.range();
        let point_count = self.point_count();
        self.delete_point(point_id)?;

        if point_count - self.point_count() > 1 {
            remove_tracked(tracked, start, count);
        } else {
            remove_tracked(tracked, point_id, 1);
        }
        Ok(())
    }

    /// Sync the handles of merged points that aren't branching, so they all follow the same pair of handles.
    /// Points whose two handles sync to different points are branching, and keep syncing to the same handles.
    fn sync_merged_handles(&mut self, merged: &[u16]) -> anyhow::Result<()> {
        // Get the roots of every handle
        let mut roots = HashMap::new();
        {
            let points = &self.points.borrow().items;
            for &point_id in merged {
                for right in [false, true] {
                    roots.insert((point_id, right), ControlPoint::get_sync_id(point_id, right, points)?);
                }
            }
        }

        // Find the roots of the points that aren't branching, in order
        let mut trees = vec![];
        for &point_id in merged {
            let (left_root, _) = roots[&(point_id, false)];
            let (right_root, _) = roots[&(point_id, true)];
            if left_root == right_root && !trees.contains(&left_root) {
                trees.push(left_root);
            }
        }
        if trees.len() < 2 { return Ok(()); }

        // Each tree is synced or flipped relative to the first one, depending on if their paths go the same way through the point.
        // Zones don't overlap, so where they touch they're on opposite sides of the point. If they wind the same way around,
        // their paths go opposite ways through it, so they're flipped. If they wind opposite ways, their paths go the same way.
        let reference = trees[0];
        let reference_winding = self.point_winding(reference)?;
        let mut aligned = HashMap::new();
        for &root_id in &trees {
            aligned.insert(root_id, self.point_winding(root_id)? != reference_winding);
        }

        // Sync every handle in those trees directly to the reference, including handles of branching points that sync into them
        let points = &mut self.points.borrow_mut().items;
        let mut synced = vec![];
        for &point_id in merged {
            if point_id == reference { continue; }
            for right in [false, true] {
                let (root_id, root_right) = roots[&(point_id, right)];
                let Some(&aligned) = aligned.get(&root_id) else { continue; };
                let target_right = if aligned { root_right } else { !root_right };
                ControlPoint::sync_handle(point_id, right, reference, target_right, points)?;
                if !synced.contains(&point_id) { synced.push(point_id); }
            }
        }
        for point_id in synced {
            ControlPoint::resync_handles(point_id, points)?;
        }
        Ok(())
    }

    /// Get which way the zone a point is in winds around: true if its signed area is positive.
    fn point_winding(&self, point_id: u16) -> anyhow::Result<bool> {
        let zone_id = self.get_point_info(point_id)?.zone_id();
        Ok(self.zone_signed_area(zone_id, true)? > 0.0)
    }
}

/// Remove a range of point IDs from a list, and shift the IDs after it back.
fn remove_tracked(tracked: &mut Vec<u16>, start: u16, count: u16) {
    tracked.retain(|&point_id| point_id < start || point_id >= start + count);
    for point_id in tracked.iter_mut() {
        if *point_id >= start + count { *point_id -= count; }
    }
}


#[cfg(test)]
mod tests {
    use studio_math::Vec2;

    use super::ControlPoint;
    use crate::objects::{ControlPointMode, ObjectsHandle};
    use crate::objects::test_utils::{add_square, add_zone};

    /// Make a square at (0, 0) and another at (20, 0), so points 1 and 4 are at (10, 0) and (20, 0)
    fn two_squares() -> ObjectsHandle {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0), (30.0, 10.0), (20.0, 10.0)]);
        objects
    }

    /// Make point b a sibling of point a, at the same position
    fn make_siblings(objects: &mut ObjectsHandle, a: u16, b: u16) {
        let position = objects.get_point_info(a).unwrap().position();
        objects.update_point(b, Some(position), None, None, None).unwrap();
        let siblings = &mut objects.point_siblings.borrow_mut().items;
        (siblings[a as usize], siblings[b as usize]) = (b, a);
    }

    /// Get a point's handles relative to its position
    fn handles(objects: &ObjectsHandle, point_id: u16) -> (Vec2, Vec2) {
        let point = objects.get_point_info(point_id).unwrap();
        (point.left_handle() - point.position(), point.right_handle() - point.position())
    }

    /// Points from separate zones become siblings
    #[test]
    fn merge_separate_zones_links_ring() {
        let mut objects = two_squares();
        let merged = objects.merge_points(&[1, 4]).unwrap();
        assert_eq!(merged, vec![1, 4]);
        assert_eq!(objects.get_siblings(1), vec![4]);
        assert_eq!(objects.object_counts(), (2, 8));
    }

    /// Merged points move to the average position, where each group of siblings only counts once
    #[test]
    fn merge_moves_to_average() {
        let mut objects = two_squares();
        add_zone(&mut objects, &[(40.0, 0.0), (50.0, 0.0), (50.0, 10.0)]);
        make_siblings(&mut objects, 1, 8);

        let merged = objects.merge_points(&[1, 8, 4]).unwrap();
        for point_id in merged {
            assert_eq!(objects.get_point_info(point_id).unwrap().position(), Vec2::new(15.0, 0.0));
        }
    }

    /// Neighboring points in a zone collapse into the first one, with the first one's left handle and the last one's right handle
    #[test]
    fn merge_adjacent_pair_collapses() {
        let mut objects = two_squares();
        objects.update_point(1, None, Some(ControlPointMode::Broken), Some(Vec2::new(-2.0, 0.0)), Some(Vec2::new(0.0, 9.0))).unwrap();
        objects.update_point(2, None, Some(ControlPointMode::Broken), Some(Vec2::new(0.0, -9.0)), Some(Vec2::new(0.0, 3.0))).unwrap();

        let merged = objects.merge_points(&[1, 2, 4]).unwrap();
        assert_eq!(objects.object_counts(), (2, 7));
        assert_eq!(merged[0], 1);
        assert_eq!(handles(&objects, 1), (Vec2::new(-2.0, 0.0), Vec2::new(0.0, 3.0)));
    }

    /// A collapsed point with no handles is linear
    #[test]
    fn merge_collapsed_mode_linear() {
        let mut objects = two_squares();
        objects.merge_points(&[1, 2, 4]).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().mode(), ControlPointMode::Linear);
    }

    /// A collapsed point with handles on a line through it is continuous
    #[test]
    fn merge_collapsed_mode_continuous() {
        let mut objects = two_squares();
        objects.update_point(1, None, Some(ControlPointMode::Broken), Some(Vec2::new(-2.0, 0.0)), None).unwrap();
        objects.update_point(2, None, Some(ControlPointMode::Broken), None, Some(Vec2::new(3.0, 0.0))).unwrap();
        objects.merge_points(&[1, 2, 4]).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().mode(), ControlPointMode::Continuous);
    }

    /// A collapsed point with handles at an angle is broken
    #[test]
    fn merge_collapsed_mode_broken() {
        let mut objects = two_squares();
        objects.update_point(1, None, Some(ControlPointMode::Broken), Some(Vec2::new(-2.0, 0.0)), None).unwrap();
        objects.update_point(2, None, Some(ControlPointMode::Broken), None, Some(Vec2::new(0.0, 3.0))).unwrap();
        objects.merge_points(&[1, 2, 4]).unwrap();
        assert_eq!(objects.get_point_info(1).unwrap().mode(), ControlPointMode::Broken);
    }

    /// Points in a zone with unselected points between them can't be merged
    #[test]
    fn merge_non_contiguous_errors() {
        let mut objects = two_squares();
        assert!(objects.merge_points(&[0, 2, 4]).is_err());
    }

    /// Zones where every point is merged are deleted, but their points still count towards the average
    #[test]
    fn merge_whole_zone_deleted_but_averaged() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (30.0, 0.0)]);

        let merged = objects.merge_points(&[1, 4, 5]).unwrap();
        assert_eq!(merged, vec![1]);
        assert_eq!(objects.object_counts(), (1, 4));
        assert_eq!(objects.get_point_info(1).unwrap().position(), Vec2::new(20.0, 0.0));
    }

    /// Merging can't delete every zone the points are in
    #[test]
    fn merge_everything_deleted_errors() {
        let mut objects = two_squares();
        assert!(objects.merge_points(&[0, 1, 2, 3]).is_err());
    }

    /// Points that are already one group of siblings can't be merged
    #[test]
    fn merge_single_group_errors() {
        let mut objects = two_squares();
        assert!(objects.merge_points(&[1]).is_err());
        make_siblings(&mut objects, 1, 4);
        assert!(objects.merge_points(&[1, 4]).is_err());
    }

    /// The returned IDs account for points deleted while collapsing
    #[test]
    fn merge_returns_shifted_ids() {
        let mut objects = two_squares();
        add_zone(&mut objects, &[(40.0, 0.0), (50.0, 0.0), (50.0, 10.0)]);

        let merged = objects.merge_points(&[1, 2, 9]).unwrap();
        assert_eq!(merged, vec![1, 8]);
        assert_eq!(objects.get_point_info(8).unwrap().zone_id(), 2);
        assert_eq!(objects.get_siblings(1), vec![8]);
    }

    /// Merged points in zones that wind the same way have paths going opposite ways through the point, so they sync flipped
    #[test]
    fn merge_syncs_same_winding_flipped() {
        let mut objects = two_squares();

        objects.merge_points(&[1, 4]).unwrap();
        let point = objects.get_point_info(4).unwrap();
        assert_eq!((point.left_sync_id(), point.right_sync_id()), (1, 1));
        assert!(point.sync_modes().0.is_flipped() && point.sync_modes().1.is_flipped());
        assert_eq!(objects.get_point_mode(4).unwrap(), Some(ControlPointMode::Linear));
    }

    /// Merged points in zones that wind opposite ways have paths going the same way through the point, so they sync directly
    #[test]
    fn merge_syncs_opposite_winding_synced() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(20.0, 0.0), (20.0, 10.0), (30.0, 10.0), (30.0, 0.0)]);

        objects.merge_points(&[1, 4]).unwrap();
        let point = objects.get_point_info(4).unwrap();
        assert_eq!((point.left_sync_id(), point.right_sync_id()), (1, 1));
        assert!(!point.sync_modes().0.is_flipped() && !point.sync_modes().1.is_flipped());
    }

    /// The sync direction only depends on how the zones wind, not on which way the paths happen to leave the point.
    /// These corners leave the point at right angles, so comparing directions there can't tell them apart.
    #[test]
    fn merge_syncs_corners_by_winding() {
        let mut objects = ObjectsHandle::headless();
        add_square(&mut objects, 10.0);
        add_zone(&mut objects, &[(10.0, 20.0), (20.0, 20.0), (20.0, 30.0), (10.0, 30.0)]);

        objects.merge_points(&[2, 4]).unwrap();
        let point = objects.get_point_info(4).unwrap();
        assert_eq!((point.left_sync_id(), point.right_sync_id()), (2, 2));
        assert!(point.sync_modes().0.is_flipped() && point.sync_modes().1.is_flipped());
    }

    /// Branching points keep syncing to the handles they synced to
    #[test]
    fn merge_keeps_branching_syncs() {
        let mut objects = two_squares();
        add_zone(&mut objects, &[(40.0, 0.0), (50.0, 0.0), (50.0, 10.0)]);
        make_siblings(&mut objects, 1, 8);
        ControlPoint::retarget(8, false, 1, false, &mut objects.points.borrow_mut().items).unwrap();

        objects.merge_points(&[1, 4]).unwrap();
        let branching = objects.get_point_info(8).unwrap();
        assert_eq!((branching.left_sync_id(), branching.right_sync_id()), (1, 8));
        assert_eq!(objects.get_point_info(4).unwrap().left_sync_id(), 1);
    }

    /// Deleting a point removes it from the tracked IDs and shifts the later ones back
    #[test]
    fn tracking_single_points() {
        let mut objects = two_squares();
        let mut tracked = vec![1, 5];
        objects.delete_point_tracking(0, &mut tracked).unwrap();
        assert_eq!(tracked, vec![0, 4]);
    }

    /// Deleting a point that deletes its whole zone removes the whole zone from the tracked IDs
    #[test]
    fn tracking_whole_zone() {
        let mut objects = ObjectsHandle::headless();
        add_zone(&mut objects, &[(0.0, 0.0), (10.0, 0.0)]);
        add_square(&mut objects, 10.0);
        let mut tracked = vec![1, 3];
        objects.delete_point_tracking(0, &mut tracked).unwrap();
        assert_eq!(tracked, vec![1]);
    }
}
