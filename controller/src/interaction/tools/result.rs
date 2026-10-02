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