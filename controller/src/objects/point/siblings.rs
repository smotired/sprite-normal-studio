use anyhow::Ok;

use super::{ControlPoint, ControlPointHandleSyncMode, PointsList, SiblingsList};

impl ControlPoint {
    /// Run a function across all siblings until we make it back to the start sibling, which does not change the control points or siblings list
    pub(super) fn traverse_siblings<F>(start_id: u16, points: &PointsList, siblings: &SiblingsList, mut action: F) -> anyhow::Result<()>
        where F: FnMut(&Self, u16)
    {
        // Traverse through the list
        let mut id = start_id;
        let mut visited = vec![];
        let point_count = points.len();
        while !visited.contains(&id) {
            visited.push(id);

            if (id as usize) >= point_count {
                anyhow::bail!("Point {} does not exist!", id);
            }

            // Run the action on the current node
            let node = &points[id as usize];
            action(node, id);

            // Stop traversal if we reach the start point
            if siblings[id as usize] == start_id {
                return Ok(());
            }

            // Continue traversal
            id = siblings[id as usize];
        }
        anyhow::bail!("Infinite loop in traverse_siblings around {}.", id);
    }

    /// Run a function across all siblings until we make it back to the start sibling, which may change the control points (but not the siblings list)
    pub(super) fn traverse_siblings_mut<F>(start_id: u16, points: &mut PointsList, siblings: &SiblingsList, mut action: F) -> anyhow::Result<()>
        where F: FnMut(&mut Self, u16)
    {
        // Traverse through the list
        let mut id = start_id;
        let mut visited = vec![];
        let point_count = points.len();
        while !visited.contains(&id) {
            visited.push(id);

            if (id as usize) >= point_count {
                anyhow::bail!("Point {} does not exist!", id);
            }

            // Run the action on the current node
            let node = &mut points[id as usize];
            action(node, id);

            // Stop traversal if we reach the start point
            if siblings[id as usize] == start_id {
                return Ok(());
            }

            // Continue traversal
            id = siblings[id as usize];
        }
        anyhow::bail!("Infinite loop in traverse_siblings around {}.", id);
    }

    /// Get a list of IDs of points that are siblings with a given point.
    /// Doesn't include the sibling itself
    pub fn get_siblings(point_id: u16, siblings: &SiblingsList) -> Vec<u16> {
        let mut my_siblings = vec![];
        let mut id = siblings[point_id as usize];
        while id != point_id {
            my_siblings.push(id);
            id = siblings[id as usize];
        }
        my_siblings
    }

    /// Get the ID of the point in the sibling list that points to this point
    pub fn get_precursor(point_id: u16, siblings: &SiblingsList) -> anyhow::Result<u16> {
        let mut id = point_id;
        for _ in 0..=65535 {
            let next = siblings[id as usize];
            if next == point_id { return Ok(id); }
            id = next;
        }
        anyhow::bail!("Could not find precursor for point {}! Siblings loop is broken.", point_id);
    }

    /// Get a list of IDs of points that watch a given point in a given direction
    pub(super) fn get_watchers(target_id: u16, points: &Vec<ControlPoint>, siblings: &SiblingsList, right: bool) -> Vec<(u16, ControlPointHandleSyncMode)> {
        let get_sync_id = |point: &ControlPoint| if right { point.right_sync_id } else { point.left_sync_id };
        let get_sync_mode = |point: &ControlPoint| {
            let (left_mode, right_mode) = point.sync_modes();
            if right { right_mode } else { left_mode }
        };
        
        let mut watchers = vec![];
        Self::traverse_siblings(target_id, points, siblings, |point, point_id| {
            if point_id != target_id && get_sync_id(point) == target_id {
                watchers.push((point_id, get_sync_mode(point)));
            }
        }).unwrap();
        watchers
    }

    /// Find the sibling to a point that's in a zone.
    pub fn find_in_zone(sibling_id: u16, zone_id: u16, points: &Vec<ControlPoint>, siblings: &SiblingsList) -> anyhow::Result<Option<u16>> {
        let mut point_id = None;
        Self::traverse_siblings(sibling_id, points, siblings, |point, id| {
            if point.zone_id() == zone_id {
                point_id = Some(id);
            }
        })?;
        Ok(point_id)
    }

    /// Get the ID and handle direction of the handle we are eventually syncing this handle to.
    pub fn get_sync_id(point_id: u16, mut right: bool, points: &[ControlPoint]) -> anyhow::Result<(u16, bool)> {
        // Helper to return true if a control point's mode syncs in a given direction
        let point_syncs_in = |point_id: u16, point: &ControlPoint, right: bool| {
            if right { point.right_sync_id != point_id }
            else     { point.left_sync_id != point_id }
        };

        let mut id = point_id;
        let mut visited = vec![];
        let point_count = points.len();
        while !visited.contains(&id) {
            visited.push(id);

            if (id as usize) >= point_count { anyhow::bail!("Point {} does not exist!", id); }
            let point = points[id as usize];

            // If the point doesn't sync in this direction
            if !point_syncs_in(id, &point, right) {
                return Ok((id, right));
            }

            // Otherwise traverse to sync target, and possibly flip
            let (left_mode, right_mode) = point.sync_modes();
            let sync_mode = if right { right_mode } else { left_mode };
            id = if right { point.right_sync_id } else { point.left_sync_id };
            if sync_mode.is_flipped() { right = !right; }
        }
        anyhow::bail!("Infinite loop in get_sync_id around {}.", id);
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::{solo_points, link_ring};

    /// Siblings are listed in ring order, not including the point itself
    #[test]
    fn get_siblings() {
        let (_, mut siblings) = solo_points(4);
        link_ring(&[0, 2, 3], &mut siblings);
        assert_eq!(ControlPoint::get_siblings(0, &siblings), vec![2, 3]);
        assert_eq!(ControlPoint::get_siblings(3, &siblings), vec![0, 2]);
        assert!(ControlPoint::get_siblings(1, &siblings).is_empty());
    }

    /// The precursor is the point that points to the given point
    #[test]
    fn get_precursor() {
        let (_, mut siblings) = solo_points(4);
        link_ring(&[0, 2, 3], &mut siblings);
        assert_eq!(ControlPoint::get_precursor(0, &siblings).unwrap(), 3);
        assert_eq!(ControlPoint::get_precursor(2, &siblings).unwrap(), 0);
        assert_eq!(ControlPoint::get_precursor(1, &siblings).unwrap(), 1);
    }

    /// If the point is not in a loop, there is no precursor
    #[test]
    fn get_precursor_broken_ring() {
        let siblings = vec![1, 1];
        assert!(ControlPoint::get_precursor(0, &siblings).is_err());
    }

    /// Traversal starts at the start point and loops through the ring once
    #[test]
    fn traverse_siblings() {
        let (points, mut siblings) = solo_points(4);
        link_ring(&[0, 2, 3], &mut siblings);

        let mut visited = vec![];
        ControlPoint::traverse_siblings(2, &points, &siblings, |_, id| visited.push(id)).unwrap();
        assert_eq!(visited, vec![2, 3, 0]);
    }

    /// Traversal fails on missing points and on loops that don't return to the start
    #[test]
    fn traverse_siblings_errors() {
        let (points, mut siblings) = solo_points(2);
        assert!(ControlPoint::traverse_siblings(5, &points, &siblings, |_, _| {}).is_err());

        // 0 -> 1 -> 1 never returns to 0
        siblings[0] = 1;
        assert!(ControlPoint::traverse_siblings(0, &points, &siblings, |_, _| {}).is_err());
    }

    /// Mutable traversal can change every point in the ring
    #[test]
    fn traverse_siblings_mut() {
        let (mut points, mut siblings) = solo_points(4);
        link_ring(&[0, 2, 3], &mut siblings);

        ControlPoint::traverse_siblings_mut(0, &mut points, &siblings, |point, _| point.set_zone_id(9)).unwrap();
        let zones: Vec<u16> = points.iter().map(|point| point.zone_id()).collect();
        assert_eq!(zones, vec![9, 0, 9, 9]);
    }

    /// Watchers are the siblings whose handle syncs to the target's handle
    #[test]
    fn get_watchers() {
        let (mut points, mut siblings) = solo_points(3);
        link_ring(&[0, 1, 2], &mut siblings);
        ControlPoint::retarget(1, false, 0, false, &mut points).unwrap(); // synced
        ControlPoint::retarget(2, false, 0, true, &mut points).unwrap();  // flipped

        let left = ControlPoint::get_watchers(0, &points, &siblings, false);
        assert_eq!(left, vec![(1, ControlPointHandleSyncMode::Synced), (2, ControlPointHandleSyncMode::Flipped)]);
        assert!(ControlPoint::get_watchers(0, &points, &siblings, true).is_empty());
    }

    /// Find the sibling of a point that is in a given zone
    #[test]
    fn find_in_zone() {
        let (mut points, mut siblings) = solo_points(3);
        link_ring(&[0, 1, 2], &mut siblings);
        points[1].set_zone_id(1);
        points[2].set_zone_id(2);

        assert_eq!(ControlPoint::find_in_zone(0, 1, &points, &siblings).unwrap(), Some(1));
        assert_eq!(ControlPoint::find_in_zone(2, 0, &points, &siblings).unwrap(), Some(0));
        assert_eq!(ControlPoint::find_in_zone(0, 7, &points, &siblings).unwrap(), None);
    }

    /// A handle that doesn't sync to anything is its own sync root
    #[test]
    fn get_sync_id_solo() {
        let (points, _) = solo_points(1);
        assert_eq!(ControlPoint::get_sync_id(0, false, &points).unwrap(), (0, false));
        assert_eq!(ControlPoint::get_sync_id(0, true, &points).unwrap(), (0, true));
    }

    /// Follow chains of syncs, flipping the handle direction each time a flipped link is crossed
    #[test]
    fn get_sync_id_chain() {
        let (mut points, _) = solo_points(3);
        ControlPoint::retarget(1, false, 0, false, &mut points).unwrap(); // 1 left -> 0 left
        assert_eq!(ControlPoint::get_sync_id(1, false, &points).unwrap(), (0, false));

        ControlPoint::retarget(2, true, 1, false, &mut points).unwrap(); // 2 right -> 1 left, flipped
        assert_eq!(ControlPoint::get_sync_id(2, true, &points).unwrap(), (0, false));

        ControlPoint::retarget(1, false, 0, true, &mut points).unwrap(); // 1 left -> 0 right, flipped
        assert_eq!(ControlPoint::get_sync_id(1, false, &points).unwrap(), (0, true));
        assert_eq!(ControlPoint::get_sync_id(2, true, &points).unwrap(), (0, true));
    }

    /// Missing points and sync loops are errors
    #[test]
    fn get_sync_id_errors() {
        let (mut points, _) = solo_points(2);
        assert!(ControlPoint::get_sync_id(5, false, &points).is_err());

        // 0 left -> 1 left -> 0 left
        ControlPoint::retarget(0, false, 1, false, &mut points).unwrap();
        ControlPoint::retarget(1, false, 0, false, &mut points).unwrap();
        assert!(ControlPoint::get_sync_id(0, false, &points).is_err());
    }
}
