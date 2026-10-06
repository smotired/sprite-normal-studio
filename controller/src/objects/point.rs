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

    /// The ID of another control point. If this point is updated, some other siblings may be updated.
    /// Should create a cycle between all shared control points.
    /// All siblings share position, some may share one or both handles depending on watch ID.
    /// Self-referential if this control point has no siblings.
    sibling_id: u16,

    _pad: [u16; 3], // padding from 34 to 40 bytes.
    // TODO: Because sibling_id is pretty much only used in traverse_siblings, we could save memory
    // by having a separate list that just tracks the nodes' siblings.
    // Exchanges for a lot of complexity though.
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

    /// Get the sibling ID of this control point.
    pub fn sibling_id(&self) -> u16 { self.sibling_id }

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
    pub fn any_syncs_in(point_id: u16, right: bool, points: &Vec<ControlPoint>) -> bool {
        if points[point_id as usize].self_syncs_in(point_id, right) { return true; }
        Self::get_watchers(point_id, points, right).len() > 0
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

    /// Force set the sibling id. Makes no guarantees about the integrity of the sibling chain after this, so be careful.
    pub fn set_sibling_id(&mut self, sibling_id: u16) { self.sibling_id = sibling_id; }

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
            sibling_id: id,
            _pad: Default::default(),
        }
    }

    /// Create a new node from a sibling node, where we branch off the other path
    pub fn new_sibling_branch_start(id: u16, zone_id: u16, sibling_id: u16, points: &mut Vec<ControlPoint>) -> Self {
        // Resolve left handle sync target
        let (sibling_id, sibling_right) = Self::get_sync_id(sibling_id, false, points).unwrap();

        // Find whatever points to the sibling, and make it point to the new node instead.
        let mut precursor_id = sibling_id;
        Self::traverse_siblings(sibling_id, points, |point, point_id| {
            if point.sibling_id == sibling_id {
                precursor_id = point_id;
            }
        }).unwrap();
        points[precursor_id as usize].sibling_id = id;
        
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
            sibling_id,
            _pad: Default::default(),
        }
    }

    /// Create a new node from a sibling node, where we return to the other path.
    pub fn new_sibling_branch_end(id: u16, zone_id: u16, sibling_id: u16, points: &mut Vec<ControlPoint>) -> Self {
        // Resolve right handle sync target
        let (sibling_id, sibling_right) = Self::get_sync_id(sibling_id, true, points).unwrap();

        // Find whatever points to the sibling, and make it point to the new node instead.
        let mut precursor_id = sibling_id;
        Self::traverse_siblings(sibling_id, points, |point, point_id| {
            if point.sibling_id == sibling_id {
                precursor_id = point_id;
            }
        }).unwrap();
        points[precursor_id as usize].sibling_id = id;
        
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
            sibling_id,
            _pad: Default::default(),
        }
    }

    /// Create a new node from a sibling node, where we are in the shared path.
    pub fn new_sibling_branch_interior(
        id: u16, zone_id: u16, sibling_id: u16,
        left_sync_id: u16, left_sync_handle: bool,
        right_sync_id: u16, right_sync_handle: bool,
        points: &mut Vec<ControlPoint>,
    ) -> Self {
        // Resolve handle sync roots
        let (left_sync_id, left_sync_handle) = Self::get_sync_id(left_sync_id, left_sync_handle, points).unwrap();
        let (right_sync_id, right_sync_handle) = Self::get_sync_id(right_sync_id, right_sync_handle, points).unwrap();

        // Find whatever points to the sibling, and make it point to the new node instead.
        let mut precursor_id = sibling_id;
        Self::traverse_siblings(sibling_id, points, |point, point_id| {
            if point.sibling_id == sibling_id {
                precursor_id = point_id;
            }
        }).unwrap();
        points[precursor_id as usize].sibling_id = id;
        
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
            mode: sibling.mode,
            flags: ControlPointHandleSyncMode::create_flags((left_mode, right_mode)),
            zone_id,
            left_sync_id,
            right_sync_id,
            sibling_id,
            _pad: Default::default(),
        }
    }

    /// Set a zone ID. Should be called when zone ordering changes.
    pub fn set_zone_id(&mut self, zone_id: u16) { self.zone_id = zone_id; }

    /// Run a function across all siblings until we make it back to the start sibling, which does not change the control points
    fn traverse_siblings<F>(start_id: u16, points: &Vec<ControlPoint>, mut action: F) -> anyhow::Result<()>
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
            if node.sibling_id == start_id {
                return Ok(());
            }

            // Continue traversal
            id = node.sibling_id;
        }
        anyhow::bail!("Infinite loop in traverse_siblings around {}.", id);
    }

    /// Run a function across all siblings until we make it back to the start sibling, which may change the control points
    fn traverse_siblings_mut<F>(start_id: u16, points: &mut Vec<ControlPoint>, mut action: F) -> anyhow::Result<()>
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
            if node.sibling_id == start_id {
                return Ok(());
            }

            // Continue traversal
            id = node.sibling_id;
        }
        anyhow::bail!("Infinite loop in traverse_siblings around {}.", id);
    }

    /// Get a list of IDs of points that are siblings with a given point.
    /// Doesn't include the sibling itself
    pub fn get_siblings(point_id: u16, points: &Vec<ControlPoint>) -> Vec<u16> {
        let mut siblings = vec![];
        Self::traverse_siblings(point_id, points, |_, sibling_id| {
            if sibling_id != point_id {
                siblings.push(sibling_id);
            }
        }).unwrap();
        siblings
    }

    /// Get a list of IDs of points that watch a given point in a given direction
    fn get_watchers(target_id: u16, points: &Vec<ControlPoint>, right: bool) -> Vec<(u16, ControlPointHandleSyncMode)> {
        let get_sync_id = |point: &ControlPoint| if right { point.right_sync_id } else { point.left_sync_id };
        let get_sync_mode = |point: &ControlPoint| {
            let (left_mode, right_mode) = point.sync_modes();
            if right { right_mode } else { left_mode }
        };
        
        let mut watchers = vec![];
        Self::traverse_siblings(target_id, points, |point, point_id| {
            if point_id != target_id && get_sync_id(point) == target_id {
                watchers.push((point_id, get_sync_mode(point)));
            }
        }).unwrap();
        watchers
    }

    /// Set the position of a control point. Updates its siblings as well.
    pub fn set_position(point_id: u16, position: Vec2, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        Self::traverse_siblings_mut(point_id, points, |point, _| { point.position = position; })
    }

    /// Add a delta to the position of a control point. Updates its siblings as well.
    pub fn add_position_delta(point_id: u16, delta: Vec2, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        Self::traverse_siblings_mut(point_id, points, |point, _| { point.position += delta; })
    }

    /// Set the handle mode of a control point. Updates siblings only if changing to linear
    pub fn set_handle_mode(point_id: u16, mode: ControlPointMode, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        if (point_id as usize) >= points.len() { anyhow::bail!("Point {} does not exist!", point_id); }

        let point_info = points[point_id as usize].clone();
        // TODO: Can probably actually make that work
        if point_info.left_sync_id != point_id || point_info.right_sync_id != point_id { anyhow::bail!("Can't set mode of a synced point! It must be Broken to sync correctly."); }
        points[point_id as usize].set_mode(mode);
        
        // If we are setting to Linear, sync handles to zero for all watchers
        if mode == ControlPointMode::Linear {
            for (watcher_id, _) in Self::get_watchers(point_id, points, false) {
                points[watcher_id as usize].left_handle = Vec2::ZERO;
            }
            for (watcher_id, _) in Self::get_watchers(point_id, points, true) {
                points[watcher_id as usize].right_handle = Vec2::ZERO;
            }
        }

        // Otherwise sync handles to what they actually are for all watchers
        else {
            for (watcher_id, sync_mode) in Self::get_watchers(point_id, points, false) {
                if sync_mode.is_flipped() {
                    points[watcher_id as usize].right_handle = point_info.left_handle;
                } else {
                    points[watcher_id as usize].left_handle = point_info.left_handle;
                }
            }
            for (watcher_id, sync_mode) in Self::get_watchers(point_id, points, true) {
                if sync_mode.is_flipped() {
                    points[watcher_id as usize].left_handle = point_info.right_handle;
                } else {
                    points[watcher_id as usize].right_handle = point_info.right_handle;
                }
            }
        }

        Ok(())
    }

    /// Set the left handle of a control point unless it's linear. Updates its siblings as well.
    pub fn set_left_handle(point_id: u16, left_handle: Vec2, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        // If this point is being synced to a left sibling, do this on that point instead
        if points[point_id as usize].left_sync_id != point_id {
            if points[point_id as usize].sync_modes().0.is_flipped() {
                return Self::set_right_handle(points[point_id as usize].left_sync_id, left_handle, points);
            } else {
                return Self::set_left_handle(points[point_id as usize].left_sync_id, left_handle, points);
            }
        }

        // Set the left handle
        points[point_id as usize].left_handle = left_handle;

        // Set it for all watchers as well
        let watcher_handle = if points[point_id as usize].mode() == ControlPointMode::Linear { Vec2::ZERO } else { left_handle };
        for (watcher_id, sync_mode) in Self::get_watchers(point_id, points, false) {
            if sync_mode.is_flipped() {
                points[watcher_id as usize].right_handle = watcher_handle;
            } else {
                points[watcher_id as usize].left_handle = watcher_handle;
            }
        }
        
        Ok(())
    }

    /// Set the right handle of a control point unless it's linear. Updates its siblings as well.
    pub fn set_right_handle(point_id: u16, right_handle: Vec2, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        // If this point is being synced to a left sibling, do this on that point instead
        if points[point_id as usize].right_sync_id != point_id {
            if points[point_id as usize].sync_modes().1.is_flipped() {
                return Self::set_left_handle(points[point_id as usize].right_sync_id, right_handle, points);
            } else {
                return Self::set_right_handle(points[point_id as usize].right_sync_id, right_handle, points);
            }
        }

        // Set the righit handle
        points[point_id as usize].right_handle = right_handle;

        // Set it for all watchers as well
        let watcher_handle = if points[point_id as usize].mode() == ControlPointMode::Linear { Vec2::ZERO } else { right_handle };
        for (watcher_id, sync_mode) in Self::get_watchers(point_id, points, true) {
            if sync_mode.is_flipped() {
                points[watcher_id as usize].left_handle = watcher_handle;
            } else {
                points[watcher_id as usize].right_handle = watcher_handle;
            }
        }
        
        Ok(())
    }

    /// Replace IDs for points by going through the whole list globally. Doesn't move the point.
    pub fn update_id(point_id: u16, new_point_id: u16, points: &mut Vec<ControlPoint>) {
        let point_count = points.len();
        for i in 0..point_count {
            let point = &mut points[i];
            if point.sibling_id == point_id { point.sibling_id = new_point_id; }
            if point.left_sync_id == point_id { point.left_sync_id = new_point_id; }
            if point.right_sync_id == point_id { point.right_sync_id = new_point_id; }
        }
    }

    /// Remove a point ID from the point list
    pub fn remove_point(point_id: u16, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        if (point_id as usize) >= points.len() { anyhow::bail!("Point {} does not exist!", point_id); }
        
        // Remove syncing dependency on this point without breaking anything else.
        for right in [false, true] {            
            // Get the handle this handle is syncing to
            let (mut sync_id, sync_right) = Self::get_sync_id(point_id, right, points)?;
            let mut sync_flipped = sync_right != right;
            
            // If we have nothing syncing to this handle we don't have to do anything else
            let watchers = Self::get_watchers(point_id, points, right);
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
        let sibling_id = points[index].sibling_id;
        Self::traverse_siblings_mut(sibling_id, points, |point, _| {
            if point.sibling_id == point_id { point.sibling_id = sibling_id; }
        })?;

        // Remove the point from the vector
        points.remove(index);

        // Shift every point ID back one
        let new_count = points.len();
        for i in index..new_count {
            Self::update_id(i as u16 + 1, i as u16, points);
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
    pub fn find_in_zone(sibling_id: u16, zone_id: u16, points: &Vec<ControlPoint>) -> anyhow::Result<Option<u16>> {
        let mut point_id = None;
        Self::traverse_siblings(sibling_id, points, |point, id| {
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