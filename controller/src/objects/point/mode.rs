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



#[cfg(test)]
mod tests {
    use super::*;

    /// Only the lowest two bits matter, and unknown values fall back to broken
    #[test]
    fn mode_from_u8() {
        assert_eq!(ControlPointMode::from(0), ControlPointMode::Continuous);
        assert_eq!(ControlPointMode::from(1), ControlPointMode::Broken);
        assert_eq!(ControlPointMode::from(2), ControlPointMode::Linear);
        assert_eq!(ControlPointMode::from(3), ControlPointMode::Broken);
        assert_eq!(ControlPointMode::from(4), ControlPointMode::Continuous);
    }

    /// Converting to u8 and back should give the same mode
    #[test]
    fn mode_round_trip() {
        for mode in [ControlPointMode::Continuous, ControlPointMode::Broken, ControlPointMode::Linear] {
            assert_eq!(ControlPointMode::from(u8::from(mode)), mode);
        }
    }

    /// Only the lowest bit matters for sync modes
    #[test]
    fn sync_mode_from_u8_and_bool() {
        assert_eq!(ControlPointHandleSyncMode::from(0u8), ControlPointHandleSyncMode::Synced);
        assert_eq!(ControlPointHandleSyncMode::from(1u8), ControlPointHandleSyncMode::Flipped);
        assert_eq!(ControlPointHandleSyncMode::from(2u8), ControlPointHandleSyncMode::Synced);
        assert_eq!(ControlPointHandleSyncMode::from(false), ControlPointHandleSyncMode::Synced);
        assert_eq!(ControlPointHandleSyncMode::from(true), ControlPointHandleSyncMode::Flipped);
    }

    /// Flipping twice should be the identity
    #[test]
    fn sync_mode_flip() {
        assert_eq!(ControlPointHandleSyncMode::Synced.flip(), ControlPointHandleSyncMode::Flipped);
        assert_eq!(ControlPointHandleSyncMode::Flipped.flip(), ControlPointHandleSyncMode::Synced);
        assert!(ControlPointHandleSyncMode::Flipped.is_flipped());
        assert!(!ControlPointHandleSyncMode::Synced.is_flipped());
    }

    /// Left is the lowest bit, right is the next bit
    #[test]
    fn flags_layout() {
        use ControlPointHandleSyncMode::{Synced, Flipped};
        assert_eq!(ControlPointHandleSyncMode::create_flags((Synced, Synced)), 0b00);
        assert_eq!(ControlPointHandleSyncMode::create_flags((Flipped, Synced)), 0b01);
        assert_eq!(ControlPointHandleSyncMode::create_flags((Synced, Flipped)), 0b10);
        assert_eq!(ControlPointHandleSyncMode::create_flags((Flipped, Flipped)), 0b11);
        assert_eq!(ControlPointHandleSyncMode::both_synced(), 0b00);
    }

    /// Creating flags then reading them should give the same modes, ignoring other bits
    #[test]
    fn flags_round_trip() {
        use ControlPointHandleSyncMode::{Synced, Flipped};
        for left in [Synced, Flipped] {
            for right in [Synced, Flipped] {
                let flags = ControlPointHandleSyncMode::create_flags((left, right));
                assert_eq!(ControlPointHandleSyncMode::from_flags(flags), (left, right));
                assert_eq!(ControlPointHandleSyncMode::from_flags(flags | 0b100), (left, right));
            }
        }
    }
}
