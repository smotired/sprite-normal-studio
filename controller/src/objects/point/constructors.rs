use studio_math::Vec2;

use super::{ControlPoint, ControlPointMode, ControlPointHandleSyncMode, PointsList, SiblingsList};
use crate::objects::save::{SavedPoint, SavedSync};

impl ControlPoint {
    /// Create a control point from how it was saved to a file. A handle with no saved sync target syncs to the point itself.
    pub fn from_saved(id: u16, zone_id: u16, saved: &SavedPoint) -> Self {
        let left_sync = saved.left_sync.unwrap_or(SavedSync { point: id, flipped: false });
        let right_sync = saved.right_sync.unwrap_or(SavedSync { point: id, flipped: false });
        Self {
            position: saved.position,
            left_handle: saved.left_handle,
            right_handle: saved.right_handle,
            mode: u8::from(saved.mode),
            flags: ControlPointHandleSyncMode::create_flags((
                ControlPointHandleSyncMode::from(left_sync.flipped),
                ControlPointHandleSyncMode::from(right_sync.flipped),
            )),
            zone_id,
            left_sync_id: left_sync.point,
            right_sync_id: right_sync.point,
        }
    }

    /// Get how this control point is saved to a file, using `file_id` to convert point IDs to the ones in the file.
    pub fn saved(&self, id: u16, file_id: impl Fn(u16) -> u16) -> SavedPoint {
        let (left_mode, right_mode) = self.sync_modes();
        let saved_sync = |sync_id: u16, mode: ControlPointHandleSyncMode| {
            (sync_id != id || mode.is_flipped()).then(|| SavedSync { point: file_id(sync_id), flipped: mode.is_flipped() })
        };
        SavedPoint {
            position: self.position,
            left_handle: self.left_handle,
            right_handle: self.right_handle,
            mode: self.mode(),
            left_sync: saved_sync(self.left_sync_id, left_mode),
            right_sync: saved_sync(self.right_sync_id, right_mode),
        }
    }

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



#[cfg(test)]
mod tests {
    use super::*;
    use super::super::test_utils::{solo_points, link_ring};

    /// A solo point is linear, belongs to its zone, and syncs to itself
    #[test]
    fn new_solo() {
        let point = ControlPoint::new_solo(4, 2, Vec2::new(1.0, 2.0));
        assert_eq!(point.position(), Vec2::new(1.0, 2.0));
        assert_eq!(point.zone_id(), 2);
        assert_eq!(point.mode(), ControlPointMode::Linear);
        assert_eq!((point.left_sync_id(), point.right_sync_id()), (4, 4));
        assert_eq!(point.sync_modes(), (ControlPointHandleSyncMode::Synced, ControlPointHandleSyncMode::Synced));
        assert_eq!((point.left_handle, point.right_handle), (Vec2::ZERO, Vec2::ZERO));
    }

    /// The start of a branch copies the sibling's left handle, and is spliced into the sibling ring
    #[test]
    fn branch_start() {
        let (mut points, mut siblings) = solo_points(2);
        points[0].left_handle = Vec2::new(1.0, 2.0);

        let point = ControlPoint::new_sibling_branch_start(1, 1, 0, &points, &mut siblings);
        assert_eq!(point.position(), points[0].position());
        assert_eq!(point.zone_id(), 1);
        assert_eq!(point.mode(), ControlPointMode::Broken);
        assert_eq!(point.left_handle, Vec2::new(1.0, 2.0));
        assert_eq!(point.right_handle, Vec2::ZERO);
        assert_eq!((point.left_sync_id(), point.right_sync_id()), (0, 1));
        assert_eq!(point.sync_modes(), (ControlPointHandleSyncMode::Synced, ControlPointHandleSyncMode::Synced));
        assert_eq!(siblings, vec![1, 0]);
    }

    /// If the sibling's left handle follows a right handle, the new point's handle is flipped
    #[test]
    fn branch_start_resolves_flipped_sync() {
        let (mut points, mut siblings) = solo_points(3);
        link_ring(&[0, 1], &mut siblings);
        points[0].right_handle = Vec2::new(5.0, 5.0);
        ControlPoint::retarget(1, false, 0, true, &mut points).unwrap(); // 1 left follows 0 right

        let point = ControlPoint::new_sibling_branch_start(2, 1, 1, &points, &mut siblings);
        assert_eq!(point.left_handle, Vec2::new(5.0, 5.0));
        assert_eq!(point.left_sync_id(), 0);
        assert_eq!(point.sync_modes().0, ControlPointHandleSyncMode::Flipped);
        assert_eq!(ControlPoint::get_siblings(0, &siblings).len(), 2);
    }

    /// The end of a branch copies the sibling's handle onto its right handle, and syncs to the sibling on the right
    #[test]
    fn branch_end() {
        let (mut points, mut siblings) = solo_points(2);
        points[0].right_handle = Vec2::new(3.0, 4.0);

        let point = ControlPoint::new_sibling_branch_end(1, 1, 0, &points, &mut siblings);
        assert_eq!(point.position(), points[0].position());
        assert_eq!(point.mode(), ControlPointMode::Broken);
        assert_eq!(point.left_handle, Vec2::ZERO);
        assert_eq!(point.right_handle, Vec2::new(3.0, 4.0));
        assert_eq!((point.left_sync_id(), point.right_sync_id()), (1, 0));
        assert_eq!(point.sync_modes(), (ControlPointHandleSyncMode::Synced, ControlPointHandleSyncMode::Synced));
        assert_eq!(siblings, vec![1, 0]);
    }

    /// An interior point copies both handles from the points it syncs to
    #[test]
    fn branch_interior() {
        let (mut points, mut siblings) = solo_points(3);
        points[0].left_handle = Vec2::new(1.0, 1.0);
        points[1].right_handle = Vec2::new(2.0, 2.0);

        let point = ControlPoint::new_sibling_branch_interior(2, 1, 0, 0, false, 1, true, &points, &mut siblings);
        assert_eq!(point.position(), points[0].position());
        assert_eq!(point.mode(), ControlPointMode::Broken);
        assert_eq!(point.left_handle, Vec2::new(1.0, 1.0));
        assert_eq!(point.right_handle, Vec2::new(2.0, 2.0));
        assert_eq!((point.left_sync_id(), point.right_sync_id()), (0, 1));
        assert_eq!(point.sync_modes(), (ControlPointHandleSyncMode::Synced, ControlPointHandleSyncMode::Synced));
        assert_eq!(siblings[0], 2);
        assert_eq!(siblings[2], 0);
    }
}
