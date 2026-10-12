use studio_math::Vec2;

use crate::objects::Bounds;

pub struct EditorToolActionResult {
    pub new_tool: super::EditorToolKind,

    /// Bounding box of the region where normals might have changed, if any
    pub normals_stale: Option<Bounds>,
}

impl EditorToolActionResult {
    pub fn new(new_tool: super::EditorToolKind, normals_stale: Option<Bounds>) -> Self {
        Self { new_tool, normals_stale }
    }
}

pub type ToolResult = anyhow::Result<EditorToolActionResult>;

pub trait DefaultResults : super::EditorTool {
    /// Create a result for an action that doesn't cause any other tool changes.
    fn ok(&self) -> ToolResult {
        Ok(EditorToolActionResult::new(self.kind(), None))
    }

    /// Create a result for an action that doesn't cause any other tool changes,
    /// but does mark the normal map as stale within the box between two corners.
    fn stale(&self, corner0: Vec2, corner1: Vec2) -> ToolResult {
        Ok(EditorToolActionResult::new(self.kind(), Some((corner0, corner1))))
    }

    /// Like stale, but for a box that might not exist. Marks nothing stale if there is no box.
    fn stale_in(&self, bounds: Option<Bounds>) -> ToolResult {
        match bounds {
            Some((corner0, corner1)) => self.stale(corner0, corner1),
            None => self.ok(),
        }
    }
}

// Automatically implement DefaultResults for every EditorTool
impl<T: super::EditorTool + ?Sized> DefaultResults for T {}

#[cfg(test)]
mod tests {
    use crate::interaction::tools::{EditorTool, EditorToolKind, EditorToolZone};

    use super::*;

    /// An ok result keeps the current tool without marking the normals stale
    #[test]
    fn ok_result() {
        let result = EditorToolZone::init().ok().unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert!(result.normals_stale.is_none());
    }

    /// A stale result keeps the current tool and marks the normals stale within the box
    #[test]
    fn stale_result() {
        let result = EditorToolZone::init().stale(Vec2::ZERO, Vec2::ONE).unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert_eq!(result.normals_stale, Some((Vec2::ZERO, Vec2::ONE)));
    }

    /// A stale result without a box doesn't mark anything stale
    #[test]
    fn stale_in_result() {
        let tool = EditorToolZone::init();
        assert!(tool.stale_in(None).unwrap().normals_stale.is_none());
        assert_eq!(tool.stale_in(Some((Vec2::ZERO, Vec2::ONE))).unwrap().normals_stale, Some((Vec2::ZERO, Vec2::ONE)));
    }
}
