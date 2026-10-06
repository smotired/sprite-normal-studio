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
    pub(super) fn create_flags((left, right): (Self, Self)) -> u8 { (u8::from(right) << 1) | (u8::from(left)) }

    /// Create a tuple of left and right handle mode from u8 flags
    pub(super) fn from_flags(flags: u8) -> (Self, Self) {
        let flags = flags & 0b11;
        (Self::from(flags & 0x1), Self::from(flags >> 1))
    }

    /// Create u8 flags for both handles being synced.
    pub(super) fn both_synced() -> u8 { 0b00 }
}
