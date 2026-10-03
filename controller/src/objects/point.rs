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

/// Defines the sync mode of a control point. It should always keep its handles synced
/// with its direct sibling.
#[repr(C)]
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum ControlPointSyncMode {
    /// Do not sync handles with the sibling, only node position.
    /// Only makes sense if a path is both created from and stopped at a sibling node
    Free      = 0,

    /// Sync only the left handle with the sibling.
    /// This node is the start of a branching path from another shape.
    /// Only makes sense for broken nodes (sibling might not be broken).
    SyncLeft  = 1,

    /// Sync only the right handle with the sibling.
    /// This node is the end of a branching path from another shape.
    /// Only makes sense for broken nodes (sibling might not be broken).
    SyncRight = 2,

    /// Sync both handles with the sibling.
    /// This node is the interior of a shared section of path between two zones.
    Synced    = 3,
}

impl ControlPointSyncMode {
    pub fn syncing_left(&self) -> bool {
        match self {
            Self::SyncLeft | Self::Synced => true,
            _ => false,
        }
    }

    pub fn syncing_right(&self) -> bool {
        match self {
            Self::SyncRight | Self::Synced => true,
            _ => false,
        }
    }
}

impl From<u8> for ControlPointSyncMode {
    fn from(value: u8) -> Self {
        match value & 0b11 {
            1 => Self::SyncLeft,
            2 => Self::SyncRight,
            3 => Self::Synced,
            _ => Self::Free, // fallback for any unexpected value
        }
    }
}

impl From<ControlPointSyncMode> for u8 {
    fn from(value: ControlPointSyncMode) -> Self { value as u8 }
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

    /// How this handle is synced with its sibling.
    /// bit 1 set = sync left, bit 2 set = sync right
    sync_mode: u8,

    /// The zone this control point is a part of.
    zone_id: u16,

    /// The ID of another control point. If this point is updated, some other siblings may be updated.
    /// Should create a cycle between all shared control points.
    /// All siblings share position, some may share one or both handles depending on watch ID.
    /// Self-referential if this control point has no siblings.
    sibling_id: u16,

    /// The ID of the sibling we are syncing to according to our sync_mode. Should be self-referential if free.
    sync_id: u16,
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

    pub fn sync_id(&self) -> u16 { self.sync_id }
    pub fn sync_mode(&self) -> ControlPointSyncMode { ControlPointSyncMode::from(self.sync_mode) }
    pub fn force_free(&mut self) { self.sync_mode = u8::from(ControlPointSyncMode::Free); self.left_handle = Vec2::ZERO; }

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
            sync_mode: u8::from(ControlPointSyncMode::Free),
            zone_id,
            sibling_id: id,
            sync_id: id,
        }
    }

    /// Create a new node from a sibling node, where we branch off the other path
    pub fn new_sibling_branch_start(id: u16, zone_id: u16, mut sibling_id: u16, points: &mut Vec<ControlPoint>) -> Self {
        // Bubble sync target down to what it's actually syncing to
        let start_sibling_id = sibling_id;
        while points[sibling_id as usize].sync_mode().syncing_left() {
            sibling_id = points[sibling_id as usize].sync_id;
            if sibling_id == start_sibling_id { panic!("Point {sibling_id} is syncing left to itself!"); }
        }

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
            left_handle: sibling.left_handle,
            right_handle: Vec2::ZERO,
            mode: u8::from(ControlPointMode::Broken),
            sync_mode: u8::from(ControlPointSyncMode::SyncLeft),
            zone_id,
            sibling_id,
            sync_id: sibling_id,
        }
    }

    /// Create a new node from a sibling node, where we return to the other path.
    pub fn new_sibling_branch_end(id: u16, zone_id: u16, mut sibling_id: u16, points: &mut Vec<ControlPoint>) -> Self {
        // Bubble sync target down to what it's actually syncing to
        let start_sibling_id = sibling_id;
        while points[sibling_id as usize].sync_mode().syncing_left() {
            sibling_id = points[sibling_id as usize].sync_id;
            if sibling_id == start_sibling_id { panic!("Point {sibling_id} is syncing left to itself!"); }
        }

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
            right_handle: sibling.right_handle,
            mode: u8::from(ControlPointMode::Broken),
            sync_mode: u8::from(ControlPointSyncMode::SyncRight),
            zone_id,
            sibling_id,
            sync_id: sibling_id,
        }
    }

    /// Create a new node from a sibling node, where we are in the shared path
    pub fn new_sibling_branch_interior(id: u16, zone_id: u16, mut sibling_id: u16, points: &mut Vec<ControlPoint>) -> Self {
        // Bubble sync target down to what it's actually syncing to
        let start_sibling_id = sibling_id;
        while points[sibling_id as usize].sync_mode() != ControlPointSyncMode::Free {
            sibling_id = points[sibling_id as usize].sync_id;
            if sibling_id == start_sibling_id { panic!("Point {sibling_id} is syncing somehow to itself!"); }
        }

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
            left_handle: sibling.left_handle,
            right_handle: sibling.right_handle,
            mode: sibling.mode,
            sync_mode: u8::from(ControlPointSyncMode::Synced),
            zone_id,
            sibling_id,
            sync_id: sibling_id,
        }
    }

    /// Set a zone ID. Should be called when zone ordering changes.
    pub fn set_zone_id(&mut self, zone_id: u16) { self.zone_id = zone_id; }

    /// Run a function across all siblings until we make it back to the start sibling
    fn traverse_siblings<F>(start_id: u16, points: &mut Vec<ControlPoint>, mut action: F) -> anyhow::Result<()>
        where F: FnMut(&mut Self, u16)
    {
        // Traverse through the list
        let mut id = start_id;
        let point_count = points.len();
        loop {
            if (id as usize) >= point_count {
                anyhow::bail!("Point {} does not exist!", id);
            }

            // Run the action on the current node
            let node = &mut points[id as usize];
            action(node, id);

            // Stop traversal if we reach the 
            if node.sibling_id == start_id {
                break;
            }

            // Continue traversal
            id = node.sibling_id;
        }

        Ok(())
    }

    /// Get a list of IDs of points that watch a given point
    fn get_watchers(target_id: u16, points: &mut Vec<ControlPoint>) -> Vec<(u16, ControlPointSyncMode)> {
        let mut watchers = vec![];
        Self::traverse_siblings(target_id, points, |point, point_id| {
            if point_id != target_id && point.sync_id == target_id {
                watchers.push((point_id, ControlPointSyncMode::from(point.sync_mode)));
            }
        }).unwrap();
        watchers
    }

    /// Set the position of a control point. Updates its siblings as well.
    pub fn set_position(point_id: u16, position: Vec2, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        Self::traverse_siblings(point_id, points, |point, _| { point.position = position; })
    }

    /// Add a delta to the position of a control point. Updates its siblings as well.
    pub fn add_position_delta(point_id: u16, delta: Vec2, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        Self::traverse_siblings(point_id, points, |point, _| { point.position += delta; })
    }

    /// Set the handle mode of a control point. Updates siblings only if changing to linear
    pub fn set_handle_mode(point_id: u16, mode: ControlPointMode, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        if (point_id as usize) >= points.len() { anyhow::bail!("Point {} does not exist!", point_id); }

        let point_info = points[point_id as usize].clone();
        // TODO: Can probably actually make that work
        if point_info.sync_mode() != ControlPointSyncMode::Free { anyhow::bail!("Can't set mode of a synced point! It must be Broken to sync correctly."); }
        points[point_id as usize].set_mode(mode);
        
        // If we are setting to Linear, sync handles to zero for all watchers
        if mode == ControlPointMode::Linear {
            for (watcher_id, sync_mode) in Self::get_watchers(point_id, points) {
                if sync_mode.syncing_left() { points[watcher_id as usize].left_handle = Vec2::ZERO; }
                if sync_mode.syncing_right() { points[watcher_id as usize].right_handle = Vec2::ZERO; }
            }
        }

        // Otherwise sync handles to what they actually are for all watchers
        else {
            for (watcher_id, sync_mode) in Self::get_watchers(point_id, points) {
                if sync_mode.syncing_left() { points[watcher_id as usize].left_handle = point_info.left_handle; }
                if sync_mode.syncing_right() { points[watcher_id as usize].right_handle = point_info.right_handle; }
            }
        }

        Ok(())
    }

    /// Set the left handle of a control point unless it's linear. Updates its siblings as well.
    pub fn set_left_handle(mut point_id: u16, left_handle: Vec2, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        // If this point is being synced to a left sibling, do this on that point instead
        let start_point_id = point_id;
        while points[point_id as usize].sync_mode().syncing_left() {
            point_id = points[point_id as usize].sync_id;
            if point_id == start_point_id { anyhow::bail!("Point {point_id} is syncing left to itself!"); }
        }

        // Set the left handle
        points[point_id as usize].left_handle = left_handle;

        // Set it for all watchers as well
        let watcher_handle = if points[point_id as usize].mode() == ControlPointMode::Linear { Vec2::ZERO } else { left_handle };
        for (watcher_id, sync_mode) in Self::get_watchers(point_id, points) {
            if sync_mode.syncing_left() { points[watcher_id as usize].left_handle = watcher_handle; }
        }
        
        Ok(())
    }

    /// Set the right handle of a control point unless it's linear. Updates its siblings as well.
    pub fn set_right_handle(mut point_id: u16, right_handle: Vec2, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        // If this point is being synced to a right sibling, do this on that point instead
        let start_point_id = point_id;
        while points[point_id as usize].sync_mode().syncing_right() {
            point_id = points[point_id as usize].sync_id;
            if point_id == start_point_id { anyhow::bail!("Point {point_id} is syncing right to itself!"); }
        }

        // Set the left handle
        points[point_id as usize].right_handle = right_handle;

        // Set it for all watchers as well
        let watcher_handle = if points[point_id as usize].mode() == ControlPointMode::Linear { Vec2::ZERO } else { right_handle };
        for (watcher_id, sync_mode) in Self::get_watchers(point_id, points) {
            if sync_mode.syncing_right() { points[watcher_id as usize].right_handle = watcher_handle; }
        }
        
        Ok(())
    }

    /// Update an ID for a point
    pub fn update_id(start_id: u16, point_id: u16, new_point_id: u16, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        // When this is called in create_ insert_ or remove_point, the indices are already updated, so this should be correct.
        // We take in our corrected start ID, and we always update our target before traversing to it.
        Self::traverse_siblings(start_id, points, |point, _| {
            if point.sibling_id == point_id { point.sibling_id = new_point_id; }
            if point.sync_id == point_id { point.sync_id = new_point_id; }
        })
    }

    /// Remove a point ID from the point list
    pub fn remove_point(point_id: u16, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        if (point_id as usize) >= points.len() { anyhow::bail!("Point {} does not exist!", point_id); }
        let removing_info = points[point_id as usize].clone();

        // Remove syncing dependency on this point without breaking anything else.
        let watchers = Self::get_watchers(point_id, points);
        
        // Find a reference point to sync everything else to.
        let mut candidate: Option<(u16, ControlPointSyncMode)> = None;
        for (watcher_id, sync_mode) in &watchers[..] {
            if *sync_mode == ControlPointSyncMode::Free { continue; }
            if *sync_mode == ControlPointSyncMode::Synced {
                candidate = Some((*watcher_id, *sync_mode));
                break;
            }
            if candidate.is_none() {
                candidate = Some((*watcher_id, *sync_mode));
            }
        }
        
        // If we don't have a candidate, everything is syncing as free so leave them be.
        // Otherwise, put everything else to sync to the reference point.
        if let Some((sync_id, sync_mode)) = candidate {
            // Put this to Free mode and update accordingly
            let point = &mut points[sync_id as usize];
            point.sync_mode = u8::from(ControlPointSyncMode::Free);
            if sync_mode.syncing_left() { point.left_handle = removing_info.left_handle; }
            if sync_mode.syncing_right() { point.right_handle = removing_info.right_handle; }
            point.set_mode(removing_info.mode());

            // Make all watchers sync to this point instead, without changing anything else
            for (watcher_id, _) in watchers {
                points[watcher_id as usize].sync_id = sync_id;
            }
        }

        // Skip the point in the siblings loop
        let index = point_id as usize;
        if index >= points.len() { anyhow::bail!("Point {} does not exist!", index); }
        let sibling_id = points[index].sibling_id;
        Self::traverse_siblings(sibling_id, points, |point, _| {
            if point.sibling_id == point_id { point.sibling_id = sibling_id; }
        })?;

        // Remove the point from the vector
        points.remove(index);

        // Shift every point ID back one
        let new_count = points.len();
        for i in index..new_count {
            Self::update_id(i as u16, i as u16 + 1, i as u16, points)?;
        }

        Ok(())
    }

    /// Flip this point, including which handles it's syncing.
    /// Should only be called when completing a path that connects a zone.
    /// Especially because it DOES NOT update siblings.
    pub fn flip(&mut self, sync_target: ControlPoint) {
        match self.sync_mode() {
            ControlPointSyncMode::Free | ControlPointSyncMode::Synced => { // synced theoretically shouldn't need flipping
                let tmp = self.right_handle;
                self.right_handle = self.left_handle;
                self.left_handle = tmp;
            },
            ControlPointSyncMode::SyncLeft => {
                self.left_handle = self.right_handle;
                self.right_handle = sync_target.right_handle;
                self.sync_mode = u8::from(ControlPointSyncMode::SyncRight);
            },
            ControlPointSyncMode::SyncRight => {
                self.right_handle = self.left_handle;
                self.left_handle = sync_target.left_handle;
                self.sync_mode = u8::from(ControlPointSyncMode::SyncLeft);
            },
        };
    }

    /// Find the sibling to a point that's in a zone.
    pub fn find_in_zone(sibling_id: u16, zone_id: u16, points: &mut Vec<ControlPoint>) -> anyhow::Result<Option<u16>> {
        let mut point_id = None;
        Self::traverse_siblings(sibling_id, points, |point, id| {
            if point.zone_id() == zone_id {
                point_id = Some(id);
            }
        })?;
        Ok(point_id)
    }
}