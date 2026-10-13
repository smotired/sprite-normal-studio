use crate::{Controller, ObjectBuffers, ObjectDiff, Bounds, EditorChange, Status};
use crate::objects::merge_bounds;
use super::tools::EditorTool;

/// How a tool action relates to a drag. Decides how errors are kept and how changes are grouped for undo.
#[derive(Copy, Clone, PartialEq, Debug)]
pub enum DragStep {
    /// Starts a new group of changes, which later steps of a drag can continue
    None,

    /// Continues a drag, joining the group of changes it started. A success doesn't hide an error from earlier in the drag.
    Continue,

    /// Releases a drag, joining the group of changes it started and then closing it.
    Release,
}

/// One step that can be undone: the changes from one action, or from a whole drag
struct HistoryEntry {
    objects: ObjectDiff,
    tool_before: Box<dyn EditorTool>,
    tool_after: Box<dyn EditorTool>,
    normals_stale: Option<Bounds>,
}

/// A group of changes that is still being made, e.g. by a drag in progress
struct PendingEntry {
    diffs: Vec<ObjectDiff>,
    tool_before: Box<dyn EditorTool>,
    normals_stale: Option<Bounds>,
}

/// The stacks of changes that can be undone and redone
#[derive(Default)]
pub struct History {
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,

    /// The group of changes still being made, which goes on the undo stack once it is closed
    pending: Option<PendingEntry>,
}

impl History {
    /// Add a change to the open group of changes, or open a new group with it
    pub fn record(&mut self, change: EditorChange, normals_stale: Option<Bounds>) {
        match &mut self.pending {
            Some(pending) => {
                pending.diffs.push(change.objects);
                pending.normals_stale = merge_bounds(pending.normals_stale, normals_stale);
            },
            None => self.pending = Some(PendingEntry { diffs: vec![change.objects], tool_before: change.tool, normals_stale }),
        }
    }

    /// Collapse the open group of changes into one entry on the undo stack, if it changed any objects.
    /// The objects and tool should be as the last change in the group left them.
    pub fn close(&mut self, objects: &ObjectBuffers, tool: &dyn EditorTool) {
        let Some(pending) = self.pending.take() else { return; };
        let diff = ObjectDiff::collapse(pending.diffs, objects);
        if diff.is_empty() { return; }

        self.undo.push(HistoryEntry { objects: diff, tool_before: pending.tool_before, tool_after: tool.clone_box(), normals_stale: pending.normals_stale });
        self.redo.clear();
    }

    /// If the open group of changes changed any objects, so closing it would add an entry
    fn pending_has_changes(&self) -> bool {
        self.pending.as_ref().is_some_and(|pending| pending.diffs.iter().any(|diff| !diff.is_empty()))
    }

    /// If there is anything to undo, including changes that haven't been closed into an entry yet
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.pending_has_changes()
    }

    /// If there is anything to redo. Changes that haven't been closed yet will clear the redo stack, so there is nothing to redo.
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty() && !self.pending_has_changes()
    }

    /// Undo the latest entry, restoring the objects and tool state from before it.
    /// Returns the region where normals might have changed, or None if there was nothing to undo.
    pub fn undo(&mut self, objects: &ObjectBuffers, tool: &mut Box<dyn EditorTool>) -> Option<Option<Bounds>> {
        self.close(objects, tool.as_ref());
        let entry = self.undo.pop()?;
        entry.objects.revert(objects);
        *tool = entry.tool_before.clone();
        let normals_stale = entry.normals_stale;
        self.redo.push(entry);
        Some(normals_stale)
    }

    /// Redo the latest undone entry, restoring the objects and tool state from after it.
    /// Returns the region where normals might have changed, or None if there was nothing to redo.
    pub fn redo(&mut self, objects: &ObjectBuffers, tool: &mut Box<dyn EditorTool>) -> Option<Option<Bounds>> {
        self.close(objects, tool.as_ref());
        let entry = self.redo.pop()?;
        entry.objects.apply(objects);
        *tool = entry.tool_after.clone();
        let normals_stale = entry.normals_stale;
        self.undo.push(entry);
        Some(normals_stale)
    }
}

impl Controller {
    /// Undo the latest action or drag
    pub fn undo(&mut self) {
        if let Some(normals_stale) = self.history.undo(&self.objects, &mut self.tool) {
            self.mark_normals_stale(normals_stale);
            self.status = Status::Ok;
        }
    }

    /// Redo the latest undone action or drag
    pub fn redo(&mut self) {
        if let Some(normals_stale) = self.history.redo(&self.objects, &mut self.tool) {
            self.mark_normals_stale(normals_stale);
            self.status = Status::Ok;
        }
    }

    /// If there is anything to undo
    pub fn can_undo(&self) -> bool { self.history.can_undo() }

    /// If there is anything to redo
    pub fn can_redo(&self) -> bool { self.history.can_redo() }
}

#[cfg(test)]
mod tests {
    use studio_math::Vec2;

    use crate::{EditorToolKind, InputModifiers};
    use crate::objects::test_utils::add_square;
    use super::*;
    use super::super::tools::{EditorToolZone, EditorToolPen, ControllerStateInput, ToolResult, run_tool_action, switch_tool, test_utils::state};

    /// Run a tool action and record it like the controller does
    fn run(history: &mut History, tool: &mut Box<dyn EditorTool>, objects: &ObjectBuffers, step: DragStep, action: impl FnOnce(&mut dyn EditorTool, ControllerStateInput) -> ToolResult) {
        if step == DragStep::None { history.close(objects, tool.as_ref()); }
        if let Some((result, change)) = run_tool_action(tool, state(objects), &mut Status::Ok, step == DragStep::Continue, action) {
            history.record(change, result.normals_stale);
            switch_tool(tool, history, result.new_tool, state(objects), state(objects));
        }
        if step == DragStep::Release { history.close(objects, tool.as_ref()); }
    }

    /// Drag the square's corner at (10, 10) through each position, with the zone tool
    fn drag(history: &mut History, tool: &mut Box<dyn EditorTool>, objects: &ObjectBuffers, positions: &[Vec2]) {
        run(history, tool, objects, DragStep::None, |tool, state| tool.handle_drag_start(state, Vec2::new(10.0, 10.0), InputModifiers::none()));
        for &pos in positions {
            run(history, tool, objects, DragStep::Continue, |tool, state| tool.handle_dragging_to(state, pos, InputModifiers::none()));
        }
        run(history, tool, objects, DragStep::Release, |tool, state| tool.handle_drag_released(state, InputModifiers::none()));
    }

    /// A whole drag is undone at once, restoring the objects and tool state
    #[test]
    fn drag_is_one_entry() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool: Box<dyn EditorTool> = EditorToolZone::init();
        let mut history = History::default();

        drag(&mut history, &mut tool, &objects, &[Vec2::new(12.0, 10.0), Vec2::new(14.0, 11.0), Vec2::new(15.0, 12.0)]);
        assert_eq!(history.undo.len(), 1);
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 12.0));

        let normals_stale = history.undo(&objects, &mut tool).unwrap();
        assert_eq!(normals_stale, Some((Vec2::ZERO, Vec2::new(15.0, 12.0))));
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(10.0, 10.0));
        assert!(tool.selection().is_empty());
        assert!(history.undo(&objects, &mut tool).is_none());
    }

    /// Redoing restores the objects and tool state from after the entry, and new changes clear what can be redone
    #[test]
    fn redo() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool: Box<dyn EditorTool> = EditorToolZone::init();
        let mut history = History::default();

        drag(&mut history, &mut tool, &objects, &[Vec2::new(15.0, 12.0)]);
        history.undo(&objects, &mut tool).unwrap();
        history.redo(&objects, &mut tool).unwrap();
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(15.0, 12.0));
        assert_eq!(tool.selection().len(), 4);
        assert!(history.redo(&objects, &mut tool).is_none());

        history.undo(&objects, &mut tool).unwrap();
        drag(&mut history, &mut tool, &objects, &[Vec2::new(20.0, 20.0)]);
        assert!(history.redo(&objects, &mut tool).is_none());
        assert_eq!(objects.get_point_info(2).unwrap().position(), Vec2::new(20.0, 20.0));
    }

    /// Undo and redo are only available when there is something to undo or redo
    #[test]
    fn can_undo_redo() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool: Box<dyn EditorTool> = EditorToolZone::init();
        let mut history = History::default();
        assert!(!history.can_undo() && !history.can_redo());

        // A click only changes the selection, so there is still nothing to undo
        run(&mut history, &mut tool, &objects, DragStep::None, |tool, state| tool.handle_click(state, Vec2::ZERO, InputModifiers::none()));
        assert!(!history.can_undo());

        drag(&mut history, &mut tool, &objects, &[Vec2::new(15.0, 12.0)]);
        assert!(history.can_undo() && !history.can_redo());

        history.undo(&objects, &mut tool).unwrap();
        assert!(!history.can_undo() && history.can_redo());
    }

    /// Actions that only change the selection aren't recorded
    #[test]
    fn selection_not_recorded() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool: Box<dyn EditorTool> = EditorToolZone::init();
        let mut history = History::default();

        run(&mut history, &mut tool, &objects, DragStep::None, |tool, state| tool.handle_click(state, Vec2::ZERO, InputModifiers::none()));
        assert!(!tool.selection().is_empty());
        assert!(history.undo(&objects, &mut tool).is_none());
    }

    /// A drag that ends where it started isn't recorded
    #[test]
    fn drag_back_not_recorded() {
        let mut objects = ObjectBuffers::headless();
        add_square(&mut objects, 10.0);
        let mut tool: Box<dyn EditorTool> = EditorToolZone::init();
        let mut history = History::default();

        drag(&mut history, &mut tool, &objects, &[Vec2::new(15.0, 12.0), Vec2::new(10.0, 10.0)]);
        assert!(history.undo.is_empty());
    }

    /// Switching away from the pen while drawing a path deletes it, which can be undone
    #[test]
    fn switching_tool_recorded() {
        let objects = ObjectBuffers::headless();
        let mut tool: Box<dyn EditorTool> = EditorToolPen::init();
        let mut history = History::default();

        run(&mut history, &mut tool, &objects, DragStep::None, |tool, state| tool.handle_click(state, Vec2::ZERO, InputModifiers::none()));
        run(&mut history, &mut tool, &objects, DragStep::None, |tool, state| tool.handle_click(state, Vec2::new(10.0, 0.0), InputModifiers::none()));
        assert_eq!(objects.object_counts(), (1, 2));

        history.close(&objects, tool.as_ref());
        switch_tool(&mut tool, &mut history, EditorToolKind::Zone, state(&objects), state(&objects));
        history.close(&objects, tool.as_ref());
        assert_eq!(objects.object_counts(), (0, 0));

        history.undo(&objects, &mut tool).unwrap();
        assert_eq!(objects.object_counts(), (1, 2));
        assert_eq!(tool.kind(), EditorToolKind::Pen);
        assert_eq!(tool.creating_path(), Some(false));

        // Then each point can be undone
        history.undo(&objects, &mut tool).unwrap();
        assert_eq!(objects.object_counts(), (1, 1));
        history.undo(&objects, &mut tool).unwrap();
        assert_eq!(objects.object_counts(), (0, 0));
        assert_eq!(tool.creating_path(), None);
    }
}
