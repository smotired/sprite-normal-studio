use anyhow::Ok;
use studio_math::Vec2;

use super::{ControlPoint, ControlPointMode, ControlPointHandleSyncMode, SiblingsList};

impl ControlPoint {
    /// Set the position of a control point. Updates its siblings as well.
    pub fn set_position(point_id: u16, position: Vec2, points: &mut Vec<ControlPoint>, siblings: &SiblingsList) -> anyhow::Result<()> {
        Self::traverse_siblings_mut(point_id, points, siblings, |point, _| { point.position = position; })
    }

    /// Add a delta to the position of a control point. Updates its siblings as well.
    pub fn add_position_delta(point_id: u16, delta: Vec2, points: &mut Vec<ControlPoint>, siblings: &SiblingsList) -> anyhow::Result<()> {
        Self::traverse_siblings_mut(point_id, points, siblings, |point, _| { point.position += delta; })
    }

    /// Push the root's stored handle value to every handle (on its siblings) that follows it.
    fn propagate_handle(root_id: u16, root_right: bool, points: &mut [ControlPoint], siblings: &SiblingsList) {
        // Determine the handle position from sync mode and handle mode
        let root = points[root_id as usize];
        let value = if root.mode() == ControlPointMode::Linear { Vec2::ZERO }
            else if root_right { root.right_handle } else { root.left_handle };

        // Propagate to all siblings of the root
        for sibling_id in Self::get_siblings(root_id, siblings) {
            // Get the point and its sync modes
            let (left_mode, right_mode) = points[sibling_id as usize].sync_modes();
            let point = &mut points[sibling_id as usize];

            // For each handle, sync if it matches
            for side in [false, true] {
                let sync_id = if side { point.right_sync_id } else { point.left_sync_id };
                if sync_id != root_id { continue; }

                let flipped = if side { right_mode } else { left_mode }.is_flipped();
                if (side != flipped) != root_right { continue; } // this handle follows the other root handle

                if side { point.right_handle = value; } else { point.left_handle = value; }
            }
        }
    }

    /// Set the handle mode of a control point. Updates siblings only if changing to linear
    pub fn set_handle_mode(point_id: u16, mode: ControlPointMode, points: &mut [ControlPoint], siblings: &SiblingsList) -> anyhow::Result<()> {
        if (point_id as usize) >= points.len() { anyhow::bail!("Point {} does not exist!", point_id); }

        let point_info = points[point_id as usize];
        // TODO: Can probably actually make that work
        if point_info.left_sync_id != point_id || point_info.right_sync_id != point_id { anyhow::bail!("Can't set mode of a synced point! It must be Broken to sync correctly."); }
        points[point_id as usize].set_mode(mode);
        
        // Propagate handle info to all watchers
        Self::propagate_handle(point_id, false, points, siblings);
        Self::propagate_handle(point_id, true, points, siblings);

        Ok(())
    }

    pub fn set_left_handle(point_id: u16, left_handle: Vec2, points: &mut [ControlPoint], siblings: &SiblingsList) -> anyhow::Result<()> {
        let (root_id, root_right) = Self::get_sync_id(point_id, false, points)?;
        if root_right { points[root_id as usize].right_handle = left_handle; }
        else          { points[root_id as usize].left_handle = left_handle; }
        Self::propagate_handle(root_id, root_right, points, siblings);
        Ok(())
    }

    pub fn set_right_handle(point_id: u16, right_handle: Vec2, points: &mut [ControlPoint], siblings: &SiblingsList) -> anyhow::Result<()> {
        let (root_id, root_right) = Self::get_sync_id(point_id, true, points)?;
        if root_right { points[root_id as usize].right_handle = right_handle; }
        else          { points[root_id as usize].left_handle = right_handle; }
        Self::propagate_handle(root_id, root_right, points, siblings);
        Ok(())
    }

    /// Replace IDs for points by going through the whole list globally. Doesn't move the point.
    pub fn update_id(point_id: u16, new_point_id: u16, points: &mut [ControlPoint], siblings: &mut SiblingsList) {
        let point_count = points.len();
        for i in 0..point_count {
            let point = &mut points[i];
            if siblings[i] == point_id { siblings[i] = new_point_id; }
            if point.left_sync_id == point_id { point.left_sync_id = new_point_id; }
            if point.right_sync_id == point_id { point.right_sync_id = new_point_id; }
        }
    }

    /// Remove a point ID from the point list
    pub fn remove_point(point_id: u16, points: &mut Vec<ControlPoint>, siblings: &mut SiblingsList) -> anyhow::Result<()> {
        if (point_id as usize) >= points.len() { anyhow::bail!("Point {} does not exist!", point_id); }
        
        // Remove syncing dependency on this point without breaking anything else.
        for right in [false, true] {            
            // Get the handle this handle is syncing to
            let (mut sync_id, sync_right) = Self::get_sync_id(point_id, right, points)?;
            let mut sync_flipped = sync_right != right;
            
            // If we have nothing syncing to this handle we don't have to do anything else
            let watchers = Self::get_watchers(point_id, points, siblings, right);
            if watchers.is_empty() { continue; }

            // If this handle was syncing to itself, promote a watcher to the sync target
            if sync_id == point_id {
                if sync_flipped { anyhow::bail!("Point {} was syncing flipped to itself!", point_id); }

                // Promote the very first watcher to the candidate point
                let (promoted_id, promoted_mode) = watchers[0];
                sync_id = promoted_id;

                // If this watcher was flipped, we must flip all the other watchers
                if promoted_mode.is_flipped() { sync_flipped = true; }
            }

            // Make all watchers sync to the target, handling flip logic correctly
            for (watcher_id, watcher_mode) in watchers {
                // Update the sync ID
                if right {
                    points[watcher_id as usize].right_sync_id = sync_id;
                } else {
                    points[watcher_id as usize].left_sync_id = sync_id;
                }

                // If the point is flipped, flip the watcher's flipped status.
                // A flipped watcher should become synced, and vice versa.
                if sync_flipped {
                    points[watcher_id as usize].set_sync_mode(right, watcher_mode.flip());
                }
            }
        }

        // Skip the point in the siblings loop
        let index = point_id as usize;
        if index >= points.len() { anyhow::bail!("Point {} does not exist!", index); }
        let precursor_id = Self::get_precursor(point_id, siblings)?;
        siblings[precursor_id as usize] = siblings[index];

        // Remove the point from the vector
        points.remove(index);

        // Shift siblings list back one
        let old_len = points.len() + 1;
        siblings.copy_within(index + 1..old_len, index);

        // Shift every point ID back one
        let new_count = points.len();
        for i in index..new_count {
            Self::update_id(i as u16 + 1, i as u16, points, siblings);
        }

        Ok(())
    }

    /// Flip this point, including which handles it's syncing.
    /// Should only be called when completing a path that connects a zone.
    /// Especially because it DOES NOT update siblings.
    pub fn flip(&mut self, point_id: u16) {
        std::mem::swap(&mut self.left_handle, &mut self.right_handle);
        std::mem::swap(&mut self.left_sync_id, &mut self.right_sync_id);

        // Swap the modes between sides, but not what they reference.
        let (l, r) = self.sync_modes();
        let new_left = if self.left_sync_id == point_id { ControlPointHandleSyncMode::Synced } else { r.flip() };
        let new_right = if self.right_sync_id == point_id { ControlPointHandleSyncMode::Synced } else { l.flip() };
        self.set_sync_modes(ControlPointHandleSyncMode::create_flags((new_left, new_right)));
    }

    /// Point one handle of `point_id` at a specific handle of another point.
    /// The relation is flipped when the two handles are in different directions.
    pub fn retarget(point_id: u16, right: bool, target_id: u16, target_right: bool, points: &mut [ControlPoint]) -> anyhow::Result<()> {
        if point_id as usize >= points.len() { anyhow::bail!("Point {} does not exist!", point_id); }
        let point = &mut points[point_id as usize];
        if right { point.right_sync_id = target_id; } else { point.left_sync_id = target_id; }
        point.set_sync_mode(right, ControlPointHandleSyncMode::from(target_right != right));
        Ok(())
    }
    
    /// Refresh the stored value of every synced handle from the handle it eventually syncs to.
    /// Should be done after flipping.
    pub fn resync_handles(point_id: u16, points: &mut [ControlPoint]) -> anyhow::Result<()> {
        for right in [false, true] {
            let (root_id, root_right) = Self::get_sync_id(point_id, right, points)?;
            if root_id == point_id { continue; } // the point syncs to its own handle

            let root = points[root_id as usize];
            let handle = if root.mode() == ControlPointMode::Linear { Vec2::ZERO }
                else if root_right { root.right_handle } else { root.left_handle };

            if right { points[point_id as usize].right_handle = handle; }
            else     { points[point_id as usize].left_handle = handle; }
        }
        Ok(())
    }
}



#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::{solo_points, link_ring};

    /// Two points where the left handle of 1 follows the left handle of 0, and handles can be edited
    fn synced_pair() -> (Vec<ControlPoint>, SiblingsList) {
        let (mut points, mut siblings) = solo_points(2);
        link_ring(&[0, 1], &mut siblings);
        ControlPoint::retarget(1, false, 0, false, &mut points).unwrap();
        points[0].set_mode(ControlPointMode::Broken);
        points[1].set_mode(ControlPointMode::Broken);
        (points, siblings)
    }

    /// Setting a position should move every sibling but nothing else
    #[test]
    fn set_position() {
        let (mut points, mut siblings) = solo_points(4);
        link_ring(&[0, 1, 2], &mut siblings);

        ControlPoint::set_position(1, Vec2::new(7.0, 8.0), &mut points, &siblings).unwrap();
        for point in points.iter().take(3) {
            assert_eq!(point.position(), Vec2::new(7.0, 8.0));
        }
        assert_eq!(points[3].position(), Vec2::new(3.0, 0.0));
    }

    /// Adding a delta should move every sibling by the same amount
    #[test]
    fn add_position_delta() {
        let (mut points, mut siblings) = solo_points(4);
        link_ring(&[0, 1, 2], &mut siblings);

        ControlPoint::add_position_delta(0, Vec2::new(1.0, 1.0), &mut points, &siblings).unwrap();
        assert_eq!(points[0].position(), Vec2::new(1.0, 1.0));
        assert_eq!(points[1].position(), Vec2::new(2.0, 1.0));
        assert_eq!(points[2].position(), Vec2::new(3.0, 1.0));
        assert_eq!(points[3].position(), Vec2::new(3.0, 0.0));
    }

    /// Moving a point that doesn't exist is an error
    #[test]
    fn set_position_missing_point() {
        let (mut points, siblings) = solo_points(2);
        assert!(ControlPoint::set_position(5, Vec2::ZERO, &mut points, &siblings).is_err());
    }

    /// Editing a handle should write to the sync root and push to everything following it
    #[test]
    fn set_left_handle_propagates() {
        let (mut points, siblings) = synced_pair();

        // Editing through the watcher edits the root
        ControlPoint::set_left_handle(1, Vec2::new(1.0, 2.0), &mut points, &siblings).unwrap();
        assert_eq!(points[0].left_handle, Vec2::new(1.0, 2.0));
        assert_eq!(points[1].left_handle, Vec2::new(1.0, 2.0));

        // The right handles are not synced to anything
        assert_eq!(points[0].right_handle, Vec2::ZERO);
        assert_eq!(points[1].right_handle, Vec2::ZERO);
    }

    /// A flipped handle follows the opposite handle of its root
    #[test]
    fn set_handle_flipped_propagates() {
        let (mut points, siblings) = synced_pair();
        ControlPoint::retarget(1, false, 0, true, &mut points).unwrap(); // 1 left follows 0 right

        ControlPoint::set_right_handle(0, Vec2::new(3.0, 4.0), &mut points, &siblings).unwrap();
        assert_eq!(points[1].left_handle, Vec2::new(3.0, 4.0));

        // The root's left handle shouldn't affect the watcher anymore
        ControlPoint::set_left_handle(0, Vec2::new(9.0, 9.0), &mut points, &siblings).unwrap();
        assert_eq!(points[1].left_handle, Vec2::new(3.0, 4.0));
    }

    /// Setting the mode to linear should zero handles that follow it
    #[test]
    fn set_handle_mode_linear_propagates() {
        let (mut points, siblings) = synced_pair();
        ControlPoint::set_left_handle(0, Vec2::new(3.0, 4.0), &mut points, &siblings).unwrap();
        assert_eq!(points[1].left_handle, Vec2::new(3.0, 4.0));

        ControlPoint::set_handle_mode(0, ControlPointMode::Linear, &mut points, &siblings).unwrap();
        assert_eq!(points[0].mode(), ControlPointMode::Linear);
        assert_eq!(points[1].left_handle, Vec2::ZERO);
    }

    /// Can't change the mode of missing points or points that are synced to something else
    #[test]
    fn set_handle_mode_errors() {
        let (mut points, siblings) = synced_pair();
        assert!(ControlPoint::set_handle_mode(5, ControlPointMode::Broken, &mut points, &siblings).is_err());
        assert!(ControlPoint::set_handle_mode(1, ControlPointMode::Linear, &mut points, &siblings).is_err());
    }

    /// Replacing an ID should update every reference to it, and nothing else
    #[test]
    fn update_id() {
        let (mut points, mut siblings) = solo_points(3);
        link_ring(&[0, 1, 2], &mut siblings);

        ControlPoint::update_id(2, 5, &mut points, &mut siblings);
        assert_eq!(siblings, vec![1, 5, 0]);
        assert_eq!((points[2].left_sync_id(), points[2].right_sync_id()), (5, 5));
        assert_eq!((points[0].left_sync_id(), points[0].right_sync_id()), (0, 0));
    }

    /// Removing a point shifts every later point back one
    #[test]
    fn remove_point_shifts_ids() {
        let (mut points, mut siblings) = solo_points(3);

        ControlPoint::remove_point(1, &mut points, &mut siblings).unwrap();
        assert_eq!(points.len(), 2);
        assert_eq!(points[1].position(), Vec2::new(2.0, 0.0));
        assert_eq!((points[1].left_sync_id(), points[1].right_sync_id()), (1, 1));
        assert_eq!(&siblings[..2], &[0, 1]);
    }

    /// Removing the root of a synced handle should promote a watcher to be the root
    #[test]
    fn remove_point_promotes_watcher() {
        let (mut points, mut siblings) = synced_pair();

        ControlPoint::remove_point(0, &mut points, &mut siblings).unwrap();
        assert_eq!(points.len(), 1);
        assert_eq!((points[0].left_sync_id(), points[0].right_sync_id()), (0, 0));
        assert_eq!(siblings[0], 0);
    }

    /// Can't remove a point that doesn't exist
    #[test]
    fn remove_point_missing_point() {
        let (mut points, mut siblings) = solo_points(2);
        assert!(ControlPoint::remove_point(5, &mut points, &mut siblings).is_err());
        assert_eq!(points.len(), 2);
    }

    /// Flipping a point swaps its handles and which points they sync to
    #[test]
    fn flip_solo() {
        let mut point = ControlPoint::new_solo(0, 0, Vec2::ZERO);
        point.left_handle = Vec2::new(1.0, 0.0);
        point.right_handle = Vec2::new(0.0, 2.0);

        point.flip(0);
        assert_eq!(point.left_handle, Vec2::new(0.0, 2.0));
        assert_eq!(point.right_handle, Vec2::new(1.0, 0.0));
        assert_eq!(point.sync_modes(), (ControlPointHandleSyncMode::Synced, ControlPointHandleSyncMode::Synced));
    }

    /// Flipping a point whose left handle syncs elsewhere makes its right handle sync flipped
    #[test]
    fn flip_synced() {
        let (mut points, _) = synced_pair();

        points[1].flip(1);
        assert_eq!((points[1].left_sync_id(), points[1].right_sync_id()), (1, 0));
        assert_eq!(points[1].sync_modes(), (ControlPointHandleSyncMode::Synced, ControlPointHandleSyncMode::Flipped));
    }

    /// Retargeting should be flipped only if the handles are in different directions
    #[test]
    fn retarget() {
        let (mut points, _) = solo_points(2);

        ControlPoint::retarget(1, true, 0, true, &mut points).unwrap();
        assert_eq!(points[1].right_sync_id(), 0);
        assert_eq!(points[1].sync_modes().1, ControlPointHandleSyncMode::Synced);

        ControlPoint::retarget(1, false, 0, true, &mut points).unwrap();
        assert_eq!(points[1].left_sync_id(), 0);
        assert_eq!(points[1].sync_modes().0, ControlPointHandleSyncMode::Flipped);

        assert!(ControlPoint::retarget(5, false, 0, false, &mut points).is_err());
    }

    /// Resyncing refreshes the stored handle from the root, and zeroes it if the root is linear
    #[test]
    fn resync_handles() {
        let (mut points, _) = synced_pair();
        ControlPoint::retarget(1, false, 0, true, &mut points).unwrap(); // 1 left follows 0 right
        points[0].right_handle = Vec2::new(3.0, 4.0);

        ControlPoint::resync_handles(1, &mut points).unwrap();
        assert_eq!(points[1].left_handle, Vec2::new(3.0, 4.0));

        points[0].set_mode(ControlPointMode::Linear);
        ControlPoint::resync_handles(1, &mut points).unwrap();
        assert_eq!(points[1].left_handle, Vec2::ZERO);
    }
}
