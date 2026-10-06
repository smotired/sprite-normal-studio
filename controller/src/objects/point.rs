use studio_math::Vec2;

mod mode;
mod constructors;
mod siblings;
mod modify;

pub use mode::{ControlPointMode, ControlPointHandleSyncMode};

type PointsList = Vec<ControlPoint>;
type SiblingsList = Vec<u16>;

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
        !Self::get_watchers(point_id, points, siblings, right).is_empty()
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

    /// Set a zone ID. Should be called when zone ordering changes.
    pub fn set_zone_id(&mut self, zone_id: u16) { self.zone_id = zone_id; }
}



/// Helpers for building control points and sibling lists in tests
#[cfg(test)]
mod test_utils {
    use studio_math::Vec2;

    use super::{ControlPoint, PointsList, SiblingsList};

    /// Create `count` solo points in zone 0 along the x axis, and a siblings list where every point is alone
    pub(super) fn solo_points(count: u16) -> (PointsList, SiblingsList) {
        let points = (0..count).map(|id| ControlPoint::new_solo(id, 0, Vec2::new(id as f32, 0.0))).collect();
        let siblings = (0..count).collect();
        (points, siblings)
    }

    /// Link the given points into a single sibling ring, in the order given
    pub(super) fn link_ring(ids: &[u16], siblings: &mut SiblingsList) {
        for (i, &id) in ids.iter().enumerate() {
            siblings[id as usize] = ids[(i + 1) % ids.len()];
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::test_utils::solo_points;

    /// Distance should be measured from the position of the point
    #[test]
    fn distance() {
        let point = ControlPoint::new_solo(0, 0, Vec2::new(1.0, 2.0));
        assert_eq!(point.position(), Vec2::new(1.0, 2.0));
        assert_eq!(point.distance(Vec2::new(4.0, 6.0)), 5.0);
    }

    /// Axis distance is the larger of the x and y differences
    #[test]
    fn absolute_axis_distance() {
        let point = ControlPoint::new_solo(0, 0, Vec2::new(1.0, 2.0));
        assert_eq!(point.absolute_axis_distance(Vec2::new(4.0, 3.0)), 3.0);
        assert_eq!(point.absolute_axis_distance(Vec2::new(0.0, -8.0)), 10.0);
        assert_eq!(point.absolute_axis_distance(point.position()), 0.0);
    }

    /// Handles are in world space, except for linear points where they sit on the point
    #[test]
    fn handles_by_mode() {
        let mut point = ControlPoint::new_solo(0, 0, Vec2::new(10.0, 10.0));
        point.left_handle = Vec2::new(-1.0, 2.0);
        point.right_handle = Vec2::new(3.0, -4.0);

        point.set_mode(ControlPointMode::Linear);
        assert_eq!(point.left_handle(), Vec2::new(10.0, 10.0));
        assert_eq!(point.right_handle(), Vec2::new(10.0, 10.0));

        for mode in [ControlPointMode::Broken, ControlPointMode::Continuous] {
            point.set_mode(mode);
            assert_eq!(point.left_handle(), Vec2::new(9.0, 12.0));
            assert_eq!(point.right_handle(), Vec2::new(13.0, 6.0));
        }
    }

    /// Mode and zone ID should read back what was set
    #[test]
    fn mode_and_zone() {
        let mut point = ControlPoint::new_solo(0, 3, Vec2::ZERO);
        assert_eq!(point.zone_id(), 3);
        point.set_zone_id(5);
        assert_eq!(point.zone_id(), 5);

        assert_eq!(point.mode(), ControlPointMode::Linear);
        point.set_mode(ControlPointMode::Continuous);
        assert_eq!(point.mode(), ControlPointMode::Continuous);
    }

    /// Setting one handle's sync mode shouldn't touch the other
    #[test]
    fn set_sync_mode() {
        let mut point = ControlPoint::new_solo(0, 0, Vec2::ZERO);
        assert_eq!(point.sync_modes(), (ControlPointHandleSyncMode::Synced, ControlPointHandleSyncMode::Synced));

        point.set_sync_mode(true, ControlPointHandleSyncMode::Flipped);
        assert_eq!(point.sync_modes(), (ControlPointHandleSyncMode::Synced, ControlPointHandleSyncMode::Flipped));

        point.set_sync_mode(false, ControlPointHandleSyncMode::Flipped);
        assert_eq!(point.sync_modes(), (ControlPointHandleSyncMode::Flipped, ControlPointHandleSyncMode::Flipped));
    }

    /// Setting sync modes should leave the higher flag bits alone
    #[test]
    fn set_sync_modes_keeps_other_flags() {
        let mut point = ControlPoint::new_solo(0, 0, Vec2::ZERO);
        point.flags = 0b100;
        point.set_sync_modes(0b11);
        assert_eq!(point.flags, 0b111);
        point.set_sync_modes(0b00);
        assert_eq!(point.flags, 0b100);
    }

    /// A point syncs "in" a direction if that handle follows a different point
    #[test]
    fn self_syncs_in() {
        let mut point = ControlPoint::new_solo(3, 0, Vec2::ZERO);
        assert!(!point.self_syncs_in(3, false));
        assert!(!point.self_syncs_in(3, true));

        point.left_sync_id = 1;
        assert!(point.self_syncs_in(3, false));
        assert!(!point.self_syncs_in(3, true));
    }

    /// Forcing free syncs both handles to itself and zeroes the left handle
    #[test]
    fn force_free() {
        let mut point = ControlPoint::new_solo(0, 0, Vec2::ZERO);
        point.left_handle = Vec2::new(1.0, 1.0);
        point.right_handle = Vec2::new(2.0, 2.0);
        point.left_sync_id = 4;
        point.right_sync_id = 5;
        point.set_sync_mode(true, ControlPointHandleSyncMode::Flipped);

        point.force_free(7);
        assert_eq!((point.left_sync_id(), point.right_sync_id()), (7, 7));
        assert_eq!(point.sync_modes(), (ControlPointHandleSyncMode::Synced, ControlPointHandleSyncMode::Synced));
        assert_eq!(point.left_handle, Vec2::ZERO);
        assert_eq!(point.right_handle, Vec2::new(2.0, 2.0));
    }

    /// Forcing synced makes the point broken and follow the target without touching handles
    #[test]
    fn force_synced() {
        let mut point = ControlPoint::new_solo(0, 0, Vec2::ZERO);
        point.right_handle = Vec2::new(2.0, 2.0);

        point.force_synced(4);
        assert_eq!(point.mode(), ControlPointMode::Broken);
        assert_eq!((point.left_sync_id(), point.right_sync_id()), (4, 4));
        assert_eq!(point.right_handle, Vec2::new(2.0, 2.0));
    }

    /// A handle is in a sync group if it syncs to something, or if something syncs to it
    #[test]
    fn any_syncs_in() {
        let (mut points, mut siblings) = solo_points(2);
        super::test_utils::link_ring(&[0, 1], &mut siblings);
        assert!(!ControlPoint::any_syncs_in(0, false, &points, &siblings));
        assert!(!ControlPoint::any_syncs_in(1, false, &points, &siblings));

        // Left handle of 1 follows 0
        points[1].left_sync_id = 0;
        assert!(ControlPoint::any_syncs_in(0, false, &points, &siblings));
        assert!(ControlPoint::any_syncs_in(1, false, &points, &siblings));
        assert!(!ControlPoint::any_syncs_in(0, true, &points, &siblings));
    }
}
