use vector::Vec2;

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

impl ControlPointMode {
    /// Convert a ControlPointMode to a u16
    pub fn to_raw(self) -> u16 { self as u16 }

    /// Convert a u16 to a ControlPointMode
    pub fn from_raw(raw: u16) -> Self {
        match raw {
            0 => Self::Continuous,
            2 => Self::Linear,
            _ => Self::Broken, // fallback for any unexpected value
        }
    }
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
    mode: u16, // use ControlPointMode::to_raw() / from_raw()

    /// The zone this control point is a part of.
    zone_id: u16,

    /// The ID of this control point in the list.
    id: u16,

    /// The ID of another control point. If this point is updated, its sibling must also be updated equivalently.
    /// Should create a cycle between all shared control points.
    /// Siblings share position, and somehow need to share handles as well IF they aren't where the path diverges, but i will cross that bridge later. Maybe with mode.
    /// Siblings do not share zone ID.
    /// Self-referential if this control point has no siblings.
    sibling_id: u16,
}

impl ControlPoint {
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
    pub fn mode(&self) -> ControlPointMode { ControlPointMode::from_raw(self.mode) }

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
            mode: ControlPointMode::Linear.to_raw(),
            zone_id,
            id,
            sibling_id: id,
        }
    }

    /// Create a new node from a sibling node
    pub fn new_sibling(id: u16, zone_id: u16, sibling: &Self) -> Self {
        Self {
            position: sibling.position,
            left_handle: sibling.left_handle,
            right_handle: sibling.right_handle,
            mode: sibling.mode,
            zone_id,
            id,
            sibling_id: sibling.id,
        }
    }

    /// Run a function across all siblings until we make it back to the start sibling
    fn traverse_siblings<F>(start_id: u16, points: &mut Vec<ControlPoint>, action: F) -> anyhow::Result<()>
        where F: Fn(&mut Self)
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
            action(node);

            // Stop traversal if we reach the front
            if node.sibling_id == start_id {
                break;
            }

            // Continue traversal
            id = node.sibling_id;
        }

        Ok(())
    }

    /// Set the position of a control point. Updates its siblings as well.
    pub fn set_position(point_id: u16, position: Vec2, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        Self::traverse_siblings(point_id, points, |point| { point.position = position; })
    }

    /// Set the handle mode of a control point. Updates its siblings as well.
    pub fn set_handle_mode(point_id: u16, mode: ControlPointMode, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        Self::traverse_siblings(point_id, points, |point| { point.mode = mode.to_raw(); })
    }

    /// Set the left handle of a control point unless it's linear. Updates its siblings as well.
    pub fn set_left_handle(point_id: u16, left_handle: Vec2, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        Self::traverse_siblings(point_id, points, |point| {
            let mode = ControlPointMode::from_raw(point.mode);
            if let ControlPointMode::Broken = mode {
                // Just set the control point
                point.left_handle = left_handle;
            } else if let ControlPointMode::Continuous = mode {
                // Set the left handle, and set the right handle's direction but not magnitude
                point.left_handle = left_handle;
                point.right_handle = -left_handle.normalized() * point.right_handle.magnitude();
            }
        })
    }

    /// Set the left handle of a control point unless it's linear. Updates its siblings as well.
    pub fn set_right_handle(point_id: u16, right_handle: Vec2, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        Self::traverse_siblings(point_id, points, |point| {
            let mode = ControlPointMode::from_raw(point.mode);
            if let ControlPointMode::Broken = mode {
                // Just set the control point
                point.right_handle = right_handle;
            } else if let ControlPointMode::Continuous = mode {
                // Set the right handle, and set the left handle's direction but not magnitude
                point.right_handle = right_handle;
                point.left_handle = -right_handle.normalized() * point.left_handle.magnitude();
            }
        })
    }

    /// Update an ID for a point
    pub fn update_id(point_id: u16, new_point_id: u16, points: &mut Vec<ControlPoint>) -> anyhow::Result<()> {
        // When this is called in create_ or insert_point, the indices are already updated, so this should be correct.
        Self::traverse_siblings(point_id, points, |point| {
            if point.id == point_id { point.id = new_point_id; }
            if point.sibling_id == point_id { point.sibling_id = new_point_id; }
        })
    }
}