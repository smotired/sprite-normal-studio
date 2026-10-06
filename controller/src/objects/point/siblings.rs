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
    pub fn get_sync_id(point_id: u16, mut right: bool, points: &Vec<ControlPoint>) -> anyhow::Result<(u16, bool)> {
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
