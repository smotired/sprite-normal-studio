use controller::{Action, EditorToolKind, PastePlacement, SavedObjects};
use eframe::egui;
use serde::{Deserialize, Serialize};
use studio_math::Vec2;

use crate::app::StudioApp;

/// Copied zones, as stored on the system clipboard so they can be pasted into any open workspace
#[derive(Serialize, Deserialize)]
struct ClipboardZones {
    sprite_normal_studio_zones: SavedObjects,
}

/// Write copied zones as text for the clipboard
fn to_text(objects: SavedObjects) -> String {
    serde_json::to_string(&ClipboardZones { sprite_normal_studio_zones: objects }).unwrap_or_default()
}

/// Read copied zones from clipboard text, or None if it isn't copied zones
fn from_text(text: &str) -> Option<SavedObjects> {
    serde_json::from_str::<ClipboardZones>(text).ok().map(|clipboard| clipboard.sprite_normal_studio_zones)
}

/// A clipboard shortcut, or a clipboard item clicked in the menu
pub(super) enum ClipboardEvent {
    Cut,
    Copy,
    Paste(String, PastePlacement),
}

/// The last zones pasted from the clipboard, so pasting them again can be offset from where they went
pub(super) struct LastPaste {
    /// The clipboard text that was pasted
    text: String,

    /// Where the center of the pasted zones ended up, in world space
    center: Vec2,
}

impl StudioApp {
    /// Handle a clipboard event in the current project
    pub(super) fn handle_clipboard_event(&mut self, ctx: &egui::Context, event: ClipboardEvent) {
        match event {
            ClipboardEvent::Cut => self.copy_selection(ctx, true),
            ClipboardEvent::Copy => self.copy_selection(ctx, false),
            ClipboardEvent::Paste(text, placement) => self.paste(&text, placement),
        }
    }

    /// Copy the selected zones to the clipboard, deleting them if cutting
    fn copy_selection(&mut self, ctx: &egui::Context, cut: bool) {
        let Some(project) = self.projects.get_mut(self.current) else { return; };
        let Some(objects) = project.controller.copy_selection() else { return; };
        let count = objects.zone_count();
        ctx.copy_text(to_text(objects));

        // The next paste is new content, so it should be offset from where it was copied from
        self.last_paste = None;

        let zones = if count == 1 { "zone" } else { "zones" };
        if cut {
            project.controller.handle_action(Action::Delete);
            project.controller.set_feedback(format!("Cut {count} {zones}"));
        } else {
            project.controller.set_feedback(format!("Copied {count} {zones}"));
        }
    }

    /// Paste zones from clipboard text into the current project. Pasting the same text again with an offset
    /// cascades from where it last went, instead of where it was copied from.
    fn paste(&mut self, text: &str, placement: PastePlacement) {
        let Some(project) = self.projects.get_mut(self.current) else { return; };
        if project.controller.selected_tool() != EditorToolKind::Zone { return; }
        let Some(objects) = from_text(text) else {
            project.controller.set_feedback("Clipboard has no zones to paste");
            return;
        };

        let placement = match (placement, &self.last_paste) {
            (PastePlacement::Offset(None), Some(last)) if last.text == text => PastePlacement::Offset(Some(last.center)),
            (placement, _) => placement,
        };
        if let Some(center) = project.controller.paste(&objects, placement) {
            self.last_paste = Some(LastPaste { text: text.to_owned(), center });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Copied zones survive being written to the clipboard and read back
    #[test]
    fn round_trip() {
        let text = to_text(SavedObjects::default());
        assert_eq!(from_text(&text), Some(SavedObjects::default()));
    }

    /// Other clipboard text isn't copied zones
    #[test]
    fn other_text() {
        assert_eq!(from_text("hello"), None);
        assert_eq!(from_text("{\"zones\": []}"), None);
        assert_eq!(from_text(""), None);
    }
}
