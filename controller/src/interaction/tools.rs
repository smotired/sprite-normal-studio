use crate::Controller;

/// Options for which tool we are using in the editor.
/// Tool options may later be added as tuple parameters in this enum.
#[derive(PartialEq, Copy, Clone, Debug)]
pub enum EditorTool {
    /// Allows selecting and manipulating zones by their control point.
    Zone,

    /// Allows selecting and manipulating individual control points.
    Point,

    /// Allows clicking/dragging to create control points for a zone.
    /// If canceled or completed swaps back to Zone mode and selects the created path.
    Pen,
}

impl Controller {
    pub fn selected_tool(&self) -> EditorTool { self.tool }

    /// Select a tool, and perform related side-effects
    pub fn select_tool(&mut self, tool: EditorTool) {
        // If this is the current tool, don't do anything.
        // Unless we have sub modes in which case cycle those instead of this stuff.
        if tool == self.tool { return; }

        // On exit from current tool, reset in-progress actions like creating shapes
        match self.tool {
            EditorTool::Pen => {
                if let Some(_selection) = self.selected_zone {
                    // Delete the shape we are currently creating
                }
            },
            _ => { },
        }

        // On entry to new tool, reset things like selection
        match tool {
            EditorTool::Zone => {
                self.selected_point = None;
            },
            EditorTool::Pen => {
                self.selected_zone = None;
                self.selected_point = None;
            },
            _ => { },
        };

        self.tool = tool;
    }
}