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
    fn propagate_handle(root_id: u16, root_right: bool, points: &mut Vec<ControlPoint>, siblings: &SiblingsList) {
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
    pub fn set_handle_mode(point_id: u16, mode: ControlPointMode, points: &mut Vec<ControlPoint>, siblings: &SiblingsList) -> anyhow::Result<()> {
        if (point_id as usize) >= points.len() { anyhow::bail!("Point {} does not exist!", point_id); }

        let point_info = points[point_id as usize].clone();
        // TODO: Can probably actually make that work
        if point_info.left_sync_id != point_id || point_info.right_sync_id != point_id { anyhow::bail!("Can't set mode of a synced point! It must be Broken to sync correctly."); }
        points[point_id as usize].set_mode(mode);
        
        // Propagate handle info to all watchers
        Self::propagate_handle(point_id, false, points, siblings);
        Self::propagate_handle(point_id, true, points, siblings);

        Ok(())
    }

    pub fn set_left_handle(point_id: u16, left_handle: Vec2, points: &mut Vec<ControlPoint>, siblings: &SiblingsList) -> anyhow::Result<()> {
        let (root_id, root_right) = Self::get_sync_id(point_id, false, points)?;
        if root_right { points[root_id as usize].right_handle = left_handle; }
        else          { points[root_id as usize].left_handle = left_handle; }
        Self::propagate_handle(root_id, root_right, points, siblings);
        Ok(())
    }

    pub fn set_right_handle(point_id: u16, right_handle: Vec2, points: &mut Vec<ControlPoint>, siblings: &SiblingsList) -> anyhow::Result<()> {
        let (root_id, root_right) = Self::get_sync_id(point_id, true, points)?;
        if root_right { points[root_id as usize].right_handle = right_handle; }
        else          { points[root_id as usize].left_handle = right_handle; }
        Self::propagate_handle(root_id, root_right, points, siblings);
        Ok(())
    }

    /// Replace IDs for points by going through the whole list globally. Doesn't move the point.
    pub fn update_id(point_id: u16, new_point_id: u16, points: &mut Vec<ControlPoint>, siblings: &mut SiblingsList) {
        let point_count = points.len();
        for i in 0..point_count {
            let point = &mut points[i];
            if siblings[i as usize] == point_id { siblings[i as usize] = new_point_id; }
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
    pub fn retarget(point_id: u16, right: bool, target_id: u16, target_right: bool, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        if point_id as usize >= points.len() { anyhow::bail!("Point {} does not exist!", point_id); }
        let point = &mut points[point_id as usize];
        if right { point.right_sync_id = target_id; } else { point.left_sync_id = target_id; }
        point.set_sync_mode(right, ControlPointHandleSyncMode::from(target_right != right));
        Ok(())
    }
    
    /// Refresh the stored value of every synced handle from the handle it eventually syncs to.
    /// Should be done after flipping.
    pub fn resync_handles(point_id: u16, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
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
