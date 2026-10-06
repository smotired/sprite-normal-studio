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
