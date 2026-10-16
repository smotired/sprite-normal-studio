use std::path::PathBuf;

use controller::{Action, Axis, EditorToolKind, PastePlacement, SpecialAction};
use eframe::egui::{self, Key, KeyboardShortcut, Modifiers};
use studio_math::Vec3;

use crate::app::{recent, StudioApp};
use crate::app::workspace::WorkspaceCommand;

// Shortcuts shown in the menus. The keys themselves are handled in input.rs.
const SELECT_ALL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::A);
const DESELECT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Escape);
const UNDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Z);
const REDO: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Y);
const REGENERATE_NORMALS: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::G);
const CUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::X);
const COPY: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::C);
const PASTE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::V);
const DUPLICATE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::D);
const DELETE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Delete);
const ZOOM_IN: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Equals);
const ZOOM_OUT: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::Minus);
const RECENTER: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::Home);
const LIGHTING: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::L);
const NORMAL_MAP: KeyboardShortcut = KeyboardShortcut::new(Modifiers::SHIFT, Key::L);
const OVERLAY: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::O);
const PATHS: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::H);
const SHAPES: KeyboardShortcut = KeyboardShortcut::new(Modifiers::NONE, Key::J);
const RESET_NORMAL: KeyboardShortcut = KeyboardShortcut::new(Modifiers::SHIFT, Key::Z);
const MERGE: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::M);
const DETACH: KeyboardShortcut = KeyboardShortcut::new(Modifiers::CTRL, Key::K);

/// Something clicked in the menu bar, which is run after the menu is shown since the menu borrows the app
enum MenuAction {
    Command(WorkspaceCommand),
    OpenRecent(PathBuf),
    LoadSpritesheet,
    LoadNormalMap,
    ResetBaseNormals,
    Controller(Action),

    /// Cut, copy or paste, which is sent back as the same event as its shortcut
    Clipboard(egui::ViewportCommand),
}

impl StudioApp {
    /// Render the menu bar across the top of the window. Must be shown before every other panel so it spans the whole window.
    /// Returns the workspace command that was clicked, if any. Everything else clicked is run here.
    pub(super) fn show_menu_bar(&mut self, ui: &mut egui::Ui) -> Option<WorkspaceCommand> {
        let mut clicked = None;
        let has_project = !self.projects.is_empty();
        egui::Panel::top("menu").show(ui, |ui| {
            egui::MenuBar::new().ui(ui, |ui| {
                ui.menu_button("File", |ui| self.file_menu(ui, &mut clicked));
                ui.add_enabled_ui(has_project, |ui| {
                    ui.menu_button("Edit", |ui| self.edit_menu(ui, &mut clicked));
                    ui.menu_button("View", |ui| self.view_menu(ui, &mut clicked));
                    ui.menu_button("Path", |ui| self.path_menu(ui, &mut clicked));
                });
            });
        });

        match clicked? {
            MenuAction::Command(command) => return Some(command),
            MenuAction::OpenRecent(path) => self.open_recent(&ui.ctx().clone(), &path),
            MenuAction::LoadSpritesheet => self.load_spritesheet(),
            MenuAction::LoadNormalMap => self.load_normal_map(),
            MenuAction::ResetBaseNormals => self.reset_base_normals(),
            MenuAction::Controller(action) => if let Some(project) = self.project_mut() {
                project.controller.handle_action(action);
            },
            MenuAction::Clipboard(command) => {
                // The event arrives on the next frame, which shouldn't wait for more input
                ui.ctx().send_viewport_cmd(command);
                ui.ctx().request_repaint();
            },
        }
        None
    }

    /// File menu: the workspace file, and the images it uses
    fn file_menu(&self, ui: &mut egui::Ui, clicked: &mut Option<MenuAction>) {
        let has_project = !self.projects.is_empty();
        command_button(ui, "New…", WorkspaceCommand::New, has_project, clicked);
        command_button(ui, "Open…", WorkspaceCommand::Open, has_project, clicked);

        // Recently opened or saved files, opened directly
        let recent = &self.recent.paths()[..self.recent.paths().len().min(recent::MENU_LIMIT)];
        ui.add_enabled_ui(!recent.is_empty(), |ui| {
            ui.menu_button("Recent Files", |ui| {
                for path in recent {
                    let name = path.file_name().unwrap_or(path.as_os_str()).to_string_lossy();
                    if ui.button(name).on_hover_text(path.display().to_string()).clicked() {
                        *clicked = Some(MenuAction::OpenRecent(path.clone()));
                    }
                }
            });
        });

        command_button(ui, "Close", WorkspaceCommand::Close, has_project, clicked);
        ui.separator();
        command_button(ui, "Save", WorkspaceCommand::Save, has_project, clicked);
        command_button(ui, "Save As…", WorkspaceCommand::SaveAs, has_project, clicked);
        ui.separator();
        command_button(ui, "Export", WorkspaceCommand::Export, has_project, clicked);
        command_button(ui, "Export As…", WorkspaceCommand::ExportAs, has_project, clicked);
        ui.separator();
        if menu_item(ui, "Load Spritesheet…", None, has_project, None) {
            *clicked = Some(MenuAction::LoadSpritesheet);
        }
        if menu_item(ui, "Load Base Normal Map…", None, has_project, None) {
            *clicked = Some(MenuAction::LoadNormalMap);
        }
        let has_base_normals = self.project().is_some_and(|project| project.normal.is_some());
        if menu_item(ui, "Reset Base Normals", None, has_base_normals, None) {
            *clicked = Some(MenuAction::ResetBaseNormals);
        }
    }

    /// Edit menu: the selection, undo history, and the clipboard
    fn edit_menu(&self, ui: &mut egui::Ui, clicked: &mut Option<MenuAction>) {
        let Some(project) = self.project() else { return; };
        let controller = &project.controller;
        let in_shape_tool = controller.selected_tool() == EditorToolKind::Shape;
        let selecting_tool = in_shape_tool || matches!(controller.selected_tool(), EditorToolKind::Zone | EditorToolKind::Point);
        let has_selection = if in_shape_tool { !controller.selected_shape_vertices().is_empty() } else { !controller.selected_points().is_empty() };

        let items = [
            ("Select All", SELECT_ALL, selecting_tool && controller.has_objects(), Action::SelectAll),
            // With nothing being dragged while the menu is open, cancelling just clears the selection
            ("Deselect", DESELECT, selecting_tool && has_selection, Action::Cancel),
        ];
        controller_items(ui, items, clicked);
        ui.separator();
        controller_items(ui, [
            ("Undo", UNDO, controller.can_undo(), Action::Undo),
            ("Redo", REDO, controller.can_redo(), Action::Redo),
        ], clicked);
        ui.separator();
        controller_items(ui, [
            ("Regenerate Normals", REGENERATE_NORMALS, true, Action::RegenerateNormals),
        ], clicked);
        ui.separator();

        // Only zones can be copied. The mouse is over the menu, so pasting offsets from where the zones were copied.
        let in_zone_tool = controller.selected_tool() == EditorToolKind::Zone;
        for (label, shortcut, enabled, command) in [
            ("Cut", CUT, in_zone_tool && has_selection, egui::ViewportCommand::RequestCut),
            ("Copy", COPY, in_zone_tool && has_selection, egui::ViewportCommand::RequestCopy),
            ("Paste", PASTE, in_zone_tool, egui::ViewportCommand::RequestPaste),
        ] {
            if menu_item(ui, label, Some(shortcut), enabled, None) {
                *clicked = Some(MenuAction::Clipboard(command));
            }
        }
        controller_items(ui, [
            ("Duplicate", DUPLICATE, in_zone_tool && has_selection, Action::Duplicate(PastePlacement::Offset(None))),
            ("Delete", DELETE, selecting_tool && has_selection, Action::Delete),
        ], clicked);
    }

    /// View menu: the camera, and what is drawn. Every item leaves room for a checkmark so the shortcuts line up.
    fn view_menu(&self, ui: &mut egui::Ui, clicked: &mut Option<MenuAction>) {
        let Some(project) = self.project() else { return; };
        let controller = &project.controller;
        for (label, shortcut, action) in [
            ("Zoom In", ZOOM_IN, Action::CameraScale(1, None)),
            ("Zoom Out", ZOOM_OUT, Action::CameraScale(-1, None)),
            ("Recenter Camera", RECENTER, Action::Recenter),
        ] {
            if menu_item(ui, label, Some(shortcut), true, Some(false)) {
                *clicked = Some(MenuAction::Controller(action));
            }
        }
        ui.separator();

        for (label, shortcut, checked, action) in [
            ("Lighting", LIGHTING, controller.lighting_on(), Action::LightingToggled(Some(true))),
            ("Normal Map", NORMAL_MAP, controller.normals_on(), Action::LightingToggled(Some(false))),
        ] {
            if menu_item(ui, label, Some(shortcut), true, Some(checked)) {
                *clicked = Some(MenuAction::Controller(action));
            }
        }
        ui.separator();

        let overlay_on = controller.overlay_on();
        if menu_item(ui, "Overlay", Some(OVERLAY), true, Some(overlay_on)) {
            *clicked = Some(MenuAction::Controller(Action::OverlayToggled));
        }

        // Parts of the overlay, disabled while what they're drawn with is off. Shapes are drawn with the paths.
        let paths_on = controller.paths_on();
        for (label, shortcut, enabled, checked, action) in [
            ("Paths", PATHS, overlay_on, paths_on, Action::PathsToggled),
            ("Shapes", SHAPES, overlay_on && paths_on, controller.shapes_on(), Action::ShapesToggled),
        ] {
            if menu_item(ui, label, Some(shortcut), enabled, Some(checked)) {
                *clicked = Some(MenuAction::Controller(action));
            }
        }
    }

    /// Path menu: changes to the selected zones or points. Each item is only enabled if it would do something.
    fn path_menu(&self, ui: &mut egui::Ui, clicked: &mut Option<MenuAction>) {
        let Some(project) = self.project() else { return; };
        let controller = &project.controller;

        let tool = controller.selected_tool();
        let zones_selected = tool == EditorToolKind::Zone && !controller.selected_points().is_empty();
        let in_point_tool = tool == EditorToolKind::Point;

        let items = [
            // The pen sets the normal for new zones
            ("Reset Normal", Some(RESET_NORMAL), zones_selected || tool == EditorToolKind::Pen, SpecialAction::NormalSet(Vec3::FORWARD, false)),
            ("Merge", Some(MERGE), in_point_tool && controller.can_merge_selection(), SpecialAction::PointsMerged),
            ("Detach", Some(DETACH), (tool == EditorToolKind::Zone || in_point_tool) && controller.can_separate_selection(), SpecialAction::SelectionSeparated),
            ("Split", None, in_point_tool && controller.selected_path().is_some(), SpecialAction::PathSplit),
        ];
        for (label, shortcut, enabled, action) in items {
            if menu_item(ui, label, shortcut, enabled, None) {
                *clicked = Some(MenuAction::Controller(Action::Special(action)));
            }
        }
        ui.separator();

        // Turning the selected zones around the center of their bounds
        for (label, action) in [
            ("Flip Horizontally", SpecialAction::SelectionFlipped(Axis::Vertical)),
            ("Flip Vertically", SpecialAction::SelectionFlipped(Axis::Horizontal)),
            ("Rotate Left", SpecialAction::SelectionRotated(false)),
            ("Rotate Right", SpecialAction::SelectionRotated(true)),
        ] {
            if menu_item(ui, label, None, zones_selected, None) {
                *clicked = Some(MenuAction::Controller(Action::Special(action)));
            }
        }
    }
}

/// Menu buttons that run controller actions
fn controller_items<const N: usize>(ui: &mut egui::Ui, items: [(&str, KeyboardShortcut, bool, Action); N], clicked: &mut Option<MenuAction>) {
    for (label, shortcut, enabled, action) in items {
        if menu_item(ui, label, Some(shortcut), enabled, None) {
            *clicked = Some(MenuAction::Controller(action));
        }
    }
}

/// A menu button for a workspace command, showing its shortcut. Disabled if it needs a project and there isn't one.
fn command_button(ui: &mut egui::Ui, label: &str, item: WorkspaceCommand, has_project: bool, clicked: &mut Option<MenuAction>) {
    if menu_item(ui, label, Some(item.shortcut()), has_project || item.works_without_project(), None) {
        *clicked = Some(MenuAction::Command(item));
    }
}

/// A menu button with its shortcut on the right. Returns true if clicked.
/// With `check`, a checkmark column is kept at the far right edge, showing ✔ when it is Some(true).
fn menu_item(ui: &mut egui::Ui, label: &str, shortcut: Option<KeyboardShortcut>, enabled: bool, check: Option<bool>) -> bool {
    let shortcut = shortcut.map(|shortcut| ui.ctx().format_shortcut(&shortcut)).unwrap_or_default();
    let check_id = ui.next_auto_id().with("check");
    let check_size = egui::Vec2::splat(ui.text_style_height(&egui::TextStyle::Button));

    let button = egui::Button::new(label);
    let button = match check {
        Some(_) => button.shortcut_text((shortcut, egui::Atom::custom(check_id, check_size))),
        None => button.shortcut_text(shortcut),
    };
    let response = ui.add_enabled_ui(enabled, |ui| button.atom_ui(ui)).inner;

    if check == Some(true) && let Some(rect) = response.rect(check_id) {
        let color = ui.style().interact(&response.response).text_color();
        let font = egui::TextStyle::Button.resolve(ui.style());
        ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, "✔", font, color);
    }
    response.response.clicked()
}
