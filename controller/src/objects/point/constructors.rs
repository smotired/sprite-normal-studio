use studio_math::Vec2;

use super::{ControlPoint, ControlPointMode, ControlPointHandleSyncMode, PointsList, SiblingsList};

impl ControlPoint {
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
    #[allow(clippy::too_many_arguments)]
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
}
