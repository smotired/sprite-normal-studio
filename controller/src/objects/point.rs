use anyhow::Ok;
use studio_math::Vec2;

/// Defines the mode of a control point. Kind of works like bitflags,
/// where first bit = "moving 1 control point moves both"
/// and 2nd bit = "the control points are at the node itself"
/// in the assignment and viewport shaders we only care about that second bit though
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ControlPointMode {
    /// Creates a continuous curve. The handles and control point are all on a line.
    Continuous = 0,

    /// Creates a broken curve. The handles may not be on a line with the control point.
    Broken     = 1,

    /// Ignores the handles, and treats the curve as if the handles were both at the point.
    Linear     = 2,
}

impl From<u8> for ControlPointMode {
    fn from(value: u8) -> Self {
        match value & 0b11 {
            0 => Self::Continuous,
            2 => Self::Linear,
            _ => Self::Broken, // fallback for any unexpected value
        }
    }
}

impl From<ControlPointMode> for u8 {
    fn from(value: ControlPointMode) -> Self { value as u8 }
}

/// Defines the sync mode of a control point's handle. It should always keep this
/// handle synced with the point it is referencing.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ControlPointHandleSyncMode {
    /// Sync the handle normally.
    Synced   = 0,

    /// Sync the handle with the target's opposite handle.
    Flipped  = 1,
}

impl From<u8> for ControlPointHandleSyncMode {
    fn from(value: u8) -> Self {
        match value & 0x1 {
            1 => Self::Flipped,
            _ => Self::Synced,
        }
    }
}

impl From<bool> for ControlPointHandleSyncMode {
    fn from(value: bool) -> Self {
        match value {
            false => Self::Synced,
            true => Self::Flipped,
        }
    }
}

impl From<ControlPointHandleSyncMode> for u8 {
    fn from(value: ControlPointHandleSyncMode) -> Self { value as Self }
}

impl ControlPointHandleSyncMode {
    /// Return true if the handle sync mode is flipped
    pub fn is_flipped(&self) -> bool { *self == ControlPointHandleSyncMode::Flipped }

    /// Flip a handle sync mode
    pub fn flip(&self) -> Self { Self::from(1 - u8::from(*self)) }

    /// Create u8 flags from a tuple of left and right handle mode
    fn create_flags((left, right): (Self, Self)) -> u8 { (u8::from(right) << 1) | (u8::from(left)) }

    /// Create a tuple of left and right handle mode from u8 flags
    fn from_flags(flags: u8) -> (Self, Self) {
        let flags = flags & 0b11;
        (Self::from(flags & 0x1), Self::from(flags >> 1))
    }

    /// Create u8 flags for both handles being synced.
    fn both_synced() -> u8 { 0b00 }
}

type PointsList = Vec<ControlPoint>;
type SiblingsList = [u16; 65536];

/// A Zone is made up of a list of control points and a list of shapes.
#[repr(C)]
#[derive(Copy, Clone, bytemuck::Pod, bytemuck::Zeroable)]
pub struct ControlPoint {
    /// The position of this control point, which is the 0th and 3rd control point for the adjacent curves.
    position: Vec2,

    /// The 2nd control point for the incoming curve, relative to position.
    left_handle: Vec2,

    /// The 1st control point of the outgoing curve, relative to position.
    right_handle: Vec2,
    
    /// The handle mode of the control point, used for rendering and control.
    mode: u8, // use ControlPointMode::from and u8::from
    // TODO: Could merge with flags below, and have it like HasLeftHandle, HasRightHandle, and HandlesSynced
    // because having a broken node with a handle at 0 doesn't work too good in the interface

    /// Extra flags for this control point.
    /// 0 - left handle sync mode
    /// 1 - right handle sync mode
    /// Later: 2 - selected
    flags: u8,

    /// The zone this control point is a part of.
    zone_id: u16,

    /// The ID of the sibling we are syncing our left handle to.
    left_sync_id: u16,

    /// The ID of the sibling we are syncing our right handle to.
    right_sync_id: u16,
}

impl ControlPoint {
    /// Get the position of this control point.
    pub fn position(&self) -> Vec2 { self.position }

    /// Calculate the distance from this control point to the given position.
    pub fn distance(&self, position: Vec2) -> f32 { self.position.distance(position) }

    /// Calculate the square distance from this control point to the given position.
    pub fn absolute_axis_distance(&self, position: Vec2) -> f32 {
        let dx = self.position.x - position.x;
        let dy = self.position.y - position.y;
        dx.abs().max(dy.abs())
    }

    /// Get the zone ID of this control point.
    pub fn zone_id(&self) -> u16 { self.zone_id }

    /// Get the mode of this control point.
    pub fn mode(&self) -> ControlPointMode { ControlPointMode::from(self.mode) }
    fn set_mode(&mut self, mode: ControlPointMode) { self.mode = u8::from(mode); } 

    pub fn left_sync_id(&self) -> u16 { self.left_sync_id }
    pub fn right_sync_id(&self) -> u16 { self.right_sync_id }
    pub fn sync_modes(&self) -> (ControlPointHandleSyncMode, ControlPointHandleSyncMode) { ControlPointHandleSyncMode::from_flags(self.flags) }
    pub fn self_syncs_in(&self, point_id: u16, right: bool) -> bool { if right { self.right_sync_id != point_id } else { self.left_sync_id != point_id } }
    fn set_sync_modes(&mut self, flags: u8) { self.flags = (self.flags & 0b11111100) | flags; }
    fn set_sync_mode(&mut self, right: bool, mode: ControlPointHandleSyncMode) {
        let (left_mode, right_mode) = self.sync_modes();
        let flags = if right { (left_mode, mode) } else { (mode, right_mode) };
        self.set_sync_modes(ControlPointHandleSyncMode::create_flags(flags));
    }

    // Return true if any handle syncs to the same handle as this one.
    pub fn any_syncs_in(point_id: u16, right: bool, points: &Vec<ControlPoint>, siblings: &SiblingsList) -> bool {
        if points[point_id as usize].self_syncs_in(point_id, right) { return true; }
        Self::get_watchers(point_id, points, siblings, right).len() > 0
    }

    /// Force a point into free mode, syncing to its own ID for both handles. Should only be used when joining a branch path to itself.
    /// Zeroes out the left handle
    pub fn force_free(&mut self, point_id: u16) {
        self.set_sync_modes(ControlPointHandleSyncMode::both_synced());
        self.left_sync_id = point_id;
        self.right_sync_id = point_id;
        self.left_handle = Vec2::ZERO;
    }

    /// Force a point into synced mode tomatch both handles to another ID. Should only be used when inserting between synced points.
    /// Does not update the handles.
    pub fn force_synced(&mut self, point_id: u16) {
        self.mode = u8::from(ControlPointMode::Broken); // all synced points must be broken
        self.set_sync_modes(ControlPointHandleSyncMode::both_synced());
        self.left_sync_id = point_id;
        self.right_sync_id = point_id;
    }

    /// Get world space position of the left handle unless linear.
    pub fn left_handle(&self) -> Vec2 { if let ControlPointMode::Linear = self.mode() { self.position } else { self.left_handle + self.position } }

    /// Get world space position of the right handle unless linear.
    pub fn right_handle(&self) -> Vec2 { if let ControlPointMode::Linear = self.mode() { self.position } else { self.right_handle + self.position } }

    /// Create a new control point node with no siblings
    pub fn new_solo(id: u16, zone_id: u16, position: Vec2) -> Self {
        Self {
            position,
            left_handle: Vec2::ZERO,
            right_handle: Vec2::ZERO,
            mode: u8::from(ControlPointMode::Linear),
            flags: ControlPointHandleSyncMode::both_synced(),
            zone_id,
            left_sync_id: id,
            right_sync_id: id,
        }
    }

    /// Create a new node from a sibling node, where we branch off the other path
    pub fn new_sibling_branch_start(id: u16, zone_id: u16, sibling_id: u16, points: &PointsList, siblings: &mut SiblingsList) -> Self {
        // Resolve left handle sync target
        let (sibling_id, sibling_right) = Self::get_sync_id(sibling_id, false, points).unwrap();

        // Find whatever points to the sibling, and make it point to the new node instead.
        let precursor_id = Self::get_precursor(sibling_id, siblings).unwrap();
        siblings[precursor_id as usize] = id;
        siblings[id as usize] = sibling_id;
        
        // Get a reference to the sibling to complete setup
        let sibling = points[sibling_id as usize];

        Self {
            position: sibling.position,
            left_handle: if sibling_right { sibling.right_handle } else { sibling.left_handle },
            right_handle: Vec2::ZERO,
            mode: u8::from(ControlPointMode::Broken),
            flags: ControlPointHandleSyncMode::create_flags((
                ControlPointHandleSyncMode::from(sibling_right),
                ControlPointHandleSyncMode::Synced,
            )),
            zone_id,
            left_sync_id: sibling_id,
            right_sync_id: id,
        }
    }

    /// Create a new node from a sibling node, where we return to the other path.
    pub fn new_sibling_branch_end(id: u16, zone_id: u16, sibling_id: u16, points: &PointsList, siblings: &mut SiblingsList) -> Self {
        // Resolve right handle sync target
        let (sibling_id, sibling_right) = Self::get_sync_id(sibling_id, true, points).unwrap();

        // Find whatever points to the sibling, and make it point to the new node instead.
        let precursor_id = Self::get_precursor(sibling_id, siblings).unwrap();
        siblings[precursor_id as usize] = id;
        siblings[id as usize] = sibling_id;
        
        // Get a reference to the sibling to complete setup
        let sibling = points[sibling_id as usize];

        Self {
            position: sibling.position,
            left_handle: Vec2::ZERO,
            right_handle: if sibling_right { sibling.right_handle } else { sibling.left_handle },
            mode: u8::from(ControlPointMode::Broken),
            flags: ControlPointHandleSyncMode::create_flags((
                ControlPointHandleSyncMode::Synced,
                ControlPointHandleSyncMode::from(!sibling_right),
            )),
            zone_id,
            left_sync_id: id,
            right_sync_id: sibling_id,
        }
    }

    /// Create a new node from a sibling node, where we are in the shared path.
    pub fn new_sibling_branch_interior(
        id: u16, zone_id: u16, sibling_id: u16,
        left_sync_id: u16, left_sync_handle: bool,
        right_sync_id: u16, right_sync_handle: bool,
        points: &PointsList,
        siblings: &mut SiblingsList,
    ) -> Self {
        // Resolve handle sync roots
        let (left_sync_id, left_sync_handle) = Self::get_sync_id(left_sync_id, left_sync_handle, points).unwrap();
        let (right_sync_id, right_sync_handle) = Self::get_sync_id(right_sync_id, right_sync_handle, points).unwrap();

        // Find whatever points to the sibling, and make it point to the new node instead.
        let precursor_id = Self::get_precursor(sibling_id, siblings).unwrap();
        siblings[precursor_id as usize] = id;
        siblings[id as usize] = sibling_id;
        
        // Get a reference to the sibling and sync targets to complete setup
        let sibling = points[sibling_id as usize];
        let left_sync = points[left_sync_id as usize];
        let right_sync = points[right_sync_id as usize];

        let left_mode = ControlPointHandleSyncMode::from(left_sync_handle);
        let right_mode = ControlPointHandleSyncMode::from(!right_sync_handle);

        Self {
            position: sibling.position,
            left_handle: if left_sync_handle { left_sync.right_handle } else { left_sync.left_handle },
            right_handle: if right_sync_handle { right_sync.right_handle } else { right_sync.left_handle },
            mode: u8::from(ControlPointMode::Broken),
            flags: ControlPointHandleSyncMode::create_flags((left_mode, right_mode)),
            zone_id,
            left_sync_id,
            right_sync_id,
        }
    }

    /// Set a zone ID. Should be called when zone ordering changes.
    pub fn set_zone_id(&mut self, zone_id: u16) { self.zone_id = zone_id; }

    /// Run a function across all siblings until we make it back to the start sibling, which does not change the control points or siblings list
    fn traverse_siblings<F>(start_id: u16, points: &PointsList, siblings: &SiblingsList, mut action: F) -> anyhow::Result<()>
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
    fn traverse_siblings_mut<F>(start_id: u16, points: &mut PointsList, siblings: &SiblingsList, mut action: F) -> anyhow::Result<()>
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
    fn get_watchers(target_id: u16, points: &Vec<ControlPoint>, siblings: &SiblingsList, right: bool) -> Vec<(u16, ControlPointHandleSyncMode)> {
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