pub struct EditorToolActionResult {
    pub new_tool: super::EditorToolKind,
    pub normals_stale: bool,
}

impl EditorToolActionResult {
    pub fn new(new_tool: super::EditorToolKind, normals_stale: bool) -> Self {
        Self { new_tool, normals_stale }
    }
}

pub type ToolResult = anyhow::Result<EditorToolActionResult>;

pub trait DefaultResults : super::EditorTool {
    /// Create a result for an action that doesn't cause any other tool changes.
    fn ok(&self) -> ToolResult {
        Ok(EditorToolActionResult::new(self.kind(), false))
    }

    /// Create a result for an action that doesn't cause any other tool changes,
    /// but does mark the normal map as stale.
    fn stale(&self) -> ToolResult {
        Ok(EditorToolActionResult::new(self.kind(), true))
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
        assert!(!result.normals_stale);
    }

    /// A stale result keeps the current tool and marks the normals stale
    #[test]
    fn stale_result() {
        let result = EditorToolZone::init().stale().unwrap();
        assert_eq!(result.new_tool, EditorToolKind::Zone);
        assert!(result.normals_stale);
    }
}
