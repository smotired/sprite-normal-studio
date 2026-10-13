use std::path::{Component, Path, PathBuf};

use anyhow::Context;
use controller::{LoadedObjects, SavedScene};
use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::app::{Project, StudioApp};
use crate::app::file::loader::SpriteFileSelection;
use crate::app::relink::RelinkDialog;

/// Texture label for the spritesheet
pub(super) const SPRITE_LABEL: &str = "spritesheet";

/// Texture label for the normal map
pub(super) const NORMAL_LABEL: &str = "normal_map";

/// Identifies a workspace file
const FORMAT: &str = "sprite-normal-studio";

/// The newest workspace file version this build can read and the one it writes
const VERSION: u32 = 1;

/// Extension of workspace files
pub(super) const EXTENSION: &str = "sns";

/// Base title of the window
const APP_TITLE: &str = "Sprite Normal Studio";

/// Buttons of the unsaved changes warning
const SAVE: &str = "Save";
const DONT_SAVE: &str = "Don't Save";
const CANCEL: &str = "Cancel";

/// A workspace file: the input files, plus the objects and light. Not the camera or the view.
/// Comparable, so a project has unsaved changes when saving it would write a different file.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
pub(super) struct WorkspaceFile {
    format: String,
    version: u32,

    /// Paths are relative to the workspace file's folder where possible, with / separators
    spritesheet: String,
    normal_map: String,

    /// Size of the spritesheet when saved, to warn if it changed
    sprite_size: (u32, u32),

    #[serde(flatten)]
    scene: SavedScene,
}

/// Commands for the workspace file, from the File menu or a shortcut
#[derive(Copy, Clone, PartialEq, Debug)]
pub(super) enum WorkspaceCommand {
    New,
    Open,
    Save,
    SaveAs,
    Close,
}

impl WorkspaceCommand {
    /// Keyboard shortcut for this command
    pub(super) const fn shortcut(self) -> egui::KeyboardShortcut {
        match self {
            Self::New    => egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::N),
            Self::Open   => egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O),
            Self::Save   => egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S),
            Self::SaveAs => egui::KeyboardShortcut::new(egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT), egui::Key::S),
            Self::Close  => egui::KeyboardShortcut::new(egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT), egui::Key::W),
        }
    }

    /// If this command does anything when there is no open project
    pub(super) const fn works_without_project(self) -> bool {
        matches!(self, Self::New | Self::Open)
    }

    /// Every command, with the most specific shortcuts first so they are matched before ones they contain
    pub(super) const ALL: [Self; 5] = [Self::SaveAs, Self::Close, Self::Save, Self::Open, Self::New];
}

impl StudioApp {
    /// Run a workspace command, showing a dialog for anything that went wrong
    pub(super) fn run_workspace_command(&mut self, ctx: &egui::Context, command: WorkspaceCommand) {
        if !command.works_without_project() && self.project.is_none() { return; }
        if self.new_project_dialog.is_some() || self.relink_dialog.is_some() { return; }

        match command {
            WorkspaceCommand::New => {
                if self.confirm_discard(ctx) {
                    self.new_project_dialog = Some(Default::default());
                }
            },
            WorkspaceCommand::Close => {
                if self.confirm_discard(ctx) {
                    self.set_project(ctx, None);
                }
            },
            WorkspaceCommand::Open => {
                if self.confirm_discard(ctx)
                    && let Some(path) = self.workspace_file_dialog().pick_file()
                {
                    let result = self.open_workspace(ctx, &path).with_context(|| format!("Could not open {}", path.display()));
                    show_result(result);
                }
            },
            WorkspaceCommand::Save => { self.save(ctx, false); },
            WorkspaceCommand::SaveAs => { self.save(ctx, true); },
        }
    }

    /// Open a workspace file from the recent files list, showing a dialog for anything that went wrong
    pub(super) fn open_recent(&mut self, ctx: &egui::Context, path: &Path) {
        if !self.confirm_discard(ctx) { return; }
        let result = self.open_workspace(ctx, path).with_context(|| format!("Could not open {}", path.display()));
        show_result(result);
    }

    /// Save the open project, asking where if it has never been saved or `save_as` is set.
    /// Shows a dialog for anything that went wrong. Returns true if the file was written.
    fn save(&mut self, ctx: &egui::Context, save_as: bool) -> bool {
        let Some(project) = &self.project else { return false; };
        let path = if project.saved && !save_as {
            project.path.clone()
        } else {
            match self.save_as_dialog().save_file() {
                Some(path) => path,
                None => return false,
            }
        };

        let result = self.save_workspace(&path).with_context(|| format!("Could not save {}", path.display()));
        let saved = result.is_ok();
        show_result(result);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
        saved
    }

    /// Check it's OK to leave the open project. If it has unsaved changes, ask whether to save them first.
    /// Returns false if the user cancelled, or chose to save and it wasn't saved.
    pub(super) fn confirm_discard(&mut self, ctx: &egui::Context) -> bool {
        if !self.has_unsaved_changes() { return true; }
        let name = self.project.as_ref()
            .and_then(|project| project.path.file_name())
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();

        let choice = rfd::MessageDialog::new()
            .set_level(rfd::MessageLevel::Warning)
            .set_title("Unsaved Changes")
            .set_description(format!("Save changes to {name} before closing it?"))
            .set_buttons(rfd::MessageButtons::YesNoCancelCustom(SAVE.to_owned(), DONT_SAVE.to_owned(), CANCEL.to_owned()))
            .show();

        // Platforms report custom buttons by their label, or as the standard button in the same place
        match choice {
            rfd::MessageDialogResult::Yes => self.save(ctx, false),
            rfd::MessageDialogResult::Custom(label) if label == SAVE => self.save(ctx, false),
            rfd::MessageDialogResult::No => true,
            rfd::MessageDialogResult::Custom(label) if label == DONT_SAVE => true,
            _ => false,
        }
    }

    /// If the open project has changes that aren't in its workspace file
    pub(super) fn has_unsaved_changes(&self) -> bool {
        let Some(project) = &self.project else { return false; };
        self.workspace_file(&project.path).is_some_and(|file| file != project.clean)
    }

    /// Recheck if the open project has unsaved changes, and update the window title if that changed
    pub(super) fn update_dirty(&mut self, ctx: &egui::Context) {
        let dirty = self.has_unsaved_changes();
        if let Some(project) = &mut self.project
            && project.dirty != dirty
        {
            project.dirty = dirty;
            ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
        }
    }

    /// Treat the open project as having no unsaved changes, as if it was just saved to its path
    pub(super) fn mark_clean(&mut self) {
        let Some(path) = self.project.as_ref().map(|project| project.path.clone()) else { return; };
        let file = self.workspace_file(&path);
        if let Some(project) = &mut self.project
            && let Some(file) = file
        {
            project.clean = file;
            project.dirty = false;
        }
    }

    /// Title for the window, with the name of the workspace file if there is one, and * if it has unsaved changes
    pub(super) fn window_title(&self) -> String {
        let Some(project) = &self.project else { return APP_TITLE.to_owned(); };
        let name = project.path.file_name().unwrap_or_default().to_string_lossy();
        let dirty = if project.dirty { "*" } else { "" };
        format!("{name}{dirty} - {APP_TITLE}")
    }

    /// File dialog for workspace files, starting in the current workspace's folder, or the most recent one's
    fn workspace_file_dialog(&self) -> rfd::FileDialog {
        let current = self.project.as_ref().map(|project| project.path.as_path())
            .or(self.recent.paths().first().map(PathBuf::as_path));
        let dialog = rfd::FileDialog::new().add_filter("Sprite Normal Studio workspace", &[EXTENSION]);
        match current.and_then(Path::parent).filter(|directory| directory.is_dir()) {
            Some(directory) => dialog.set_directory(directory),
            None => dialog,
        }
    }

    /// Save dialog, suggesting the current workspace's name
    fn save_as_dialog(&self) -> rfd::FileDialog {
        let name = self.project.as_ref()
            .and_then(|project| project.path.file_stem())
            .map(|stem| format!("{}.{EXTENSION}", stem.to_string_lossy()));
        let dialog = self.workspace_file_dialog();
        match name {
            Some(name) => dialog.set_file_name(name),
            None => dialog,
        }
    }

    /// The workspace file the open project would be saved as at a path, if there is a project
    fn workspace_file(&self, path: &Path) -> Option<WorkspaceFile> {
        let project = self.project.as_ref()?;
        Some(build_workspace_file(&project.sprite, &project.normal, self.controller.save_scene(), path))
    }

    /// Save the workspace to a file and make it the current workspace file
    fn save_workspace(&mut self, path: &Path) -> anyhow::Result<Option<String>> {
        let Some(file) = self.workspace_file(path) else { anyhow::bail!("There is no open project to save."); };
        let json = serde_json::to_string_pretty(&file)?;

        // Write to a temporary file first, so a failed write can't leave a broken workspace file behind
        let mut temp_path = path.as_os_str().to_owned();
        temp_path.push(".tmp");
        std::fs::write(&temp_path, json).context("Could not write the file")?;
        std::fs::rename(&temp_path, path).context("Could not replace the file")?;

        if let Some(project) = &mut self.project {
            project.path = path.to_owned();
            project.saved = true;
            project.clean = file;
            project.dirty = false;
        }
        self.recent.mark_used(path);
        Ok(None)
    }

    /// Open a workspace file and make it the current workspace file. Nothing changes if it can't be opened.
    /// If its spritesheet or normal map can't be loaded, the Missing Files dialog is opened to finish opening it instead.
    /// Returns a warning if it opened but something may be wrong.
    fn open_workspace(&mut self, ctx: &egui::Context, path: &Path) -> anyhow::Result<Option<String>> {
        let file = read_workspace(&std::fs::read_to_string(path)?)?;
        let objects = file.scene.objects.validate().context("The workspace file has invalid objects")?;
        let base = path.parent().unwrap_or(Path::new(""));

        let pending = PendingOpen {
            path: path.to_owned(),
            objects,
            sprite_path: resolve(&file.spritesheet, base),
            normal_path: resolve(&file.normal_map, base),
            file,
        };

        // Load the images, which are the last things that can fail
        let sprite = SpriteFileSelection::open(SPRITE_LABEL, pending.sprite_path.clone(), &self.render_state).ok();
        let normal = SpriteFileSelection::open(NORMAL_LABEL, pending.normal_path.clone(), &self.render_state).ok();
        match (sprite, normal) {
            (Some(sprite), Some(normal)) => Ok(self.finish_open(ctx, pending, sprite, normal)),
            (sprite, normal) => {
                self.relink_dialog = Some(RelinkDialog::new(pending, sprite, normal));
                Ok(None)
            },
        }
    }

    /// Make a workspace file the current one, once its images are loaded.
    /// Returns a warning if the spritesheet's size changed since it was saved.
    pub(super) fn finish_open(&mut self, ctx: &egui::Context, pending: PendingOpen, sprite: SpriteFileSelection, normal: SpriteFileSelection) -> Option<String> {
        let size = sprite.size();
        let saved_size = pending.file.sprite_size;
        let light = pending.file.scene.light;

        // The file as it was read is what's on disk, so images picked somewhere else count as unsaved changes
        self.set_project(ctx, Some(Project { sprite, normal, path: pending.path.clone(), saved: true, clean: pending.file, dirty: false }));
        self.controller.load_scene(light, pending.objects);
        self.recent.mark_used(&pending.path);

        (size != saved_size).then(|| format!(
            "The spritesheet is {}x{}, but was {}x{} when the workspace was saved. Zones may not line up with it.",
            size.0, size.1, saved_size.0, saved_size.1,
        ))
    }
}

/// A workspace file that has been read, but whose images haven't all been loaded yet
pub(super) struct PendingOpen {
    /// Path of the workspace file
    pub(super) path: PathBuf,
    /// The workspace file as it was read
    file: WorkspaceFile,
    /// The objects in it, already checked
    objects: LoadedObjects,

    /// Where the workspace file says the images are
    pub(super) sprite_path: PathBuf,
    pub(super) normal_path: PathBuf,
}

/// Build the workspace file for a project's images and scene, as it would be saved at a path
pub(super) fn build_workspace_file(sprite: &SpriteFileSelection, normal: &SpriteFileSelection, scene: SavedScene, path: &Path) -> WorkspaceFile {
    let base = path.parent().unwrap_or(Path::new(""));
    WorkspaceFile {
        format: FORMAT.to_owned(),
        version: VERSION,
        spritesheet: to_relative(sprite.path(), base),
        normal_map: to_relative(normal.path(), base),
        sprite_size: sprite.size(),
        scene,
    }
}

/// Parse a workspace file, checking it is one this build can read
fn read_workspace(json: &str) -> anyhow::Result<WorkspaceFile> {
    // Check the format and version first, so newer files give a clear error instead of a parse error
    let header: serde_json::Value = serde_json::from_str(json).context("The file is not valid JSON")?;
    if header.get("format").and_then(|format| format.as_str()) != Some(FORMAT) {
        anyhow::bail!("This is not a Sprite Normal Studio workspace file.");
    }
    let version = header.get("version").and_then(|version| version.as_u64()).context("The workspace file has no version")?;
    if version > VERSION as u64 {
        anyhow::bail!("This workspace file is version {version}, but only version {VERSION} or older can be opened. Update Sprite Normal Studio to open it.");
    }

    serde_json::from_value(header).context("The workspace file is broken")
}

/// Get a path relative to a folder, with / separators so it works on every platform.
/// Only paths in the folder or below it are made relative. Anything else is kept absolute.
fn to_relative(path: &Path, base: &Path) -> String {
    match path.strip_prefix(base) {
        Ok(relative) if base.components().next().is_some() => relative.components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        _ => path.to_string_lossy().into_owned(),
    }
}

/// Show a message box for the result of opening or saving a workspace file, if there is anything to show
pub(super) fn show_result(result: anyhow::Result<Option<String>>) {
    match result {
        Ok(Some(warning)) => show_message(rfd::MessageLevel::Warning, "Warning", &warning),
        Ok(None) => {},
        Err(e) => show_message(rfd::MessageLevel::Error, "Error", &format!("{e:#}")),
    }
}

/// Get a saved path, resolving relative paths against a folder
fn resolve(path: &str, base: &Path) -> PathBuf {
    let path = Path::new(path);
    if path.is_absolute() || path.components().next().is_some_and(|c| matches!(c, Component::Prefix(_) | Component::RootDir)) {
        path.to_owned()
    } else {
        base.join(path)
    }
}

/// Show a native message box
fn show_message(level: rfd::MessageLevel, title: &str, text: &str) {
    rfd::MessageDialog::new()
        .set_level(level)
        .set_title(title)
        .set_description(text)
        .set_buttons(rfd::MessageButtons::Ok)
        .show();
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An absolute path in a temporary folder, so tests work on every platform
    fn absolute(parts: &[&str]) -> PathBuf {
        parts.iter().fold(std::env::temp_dir(), |path, part| path.join(part))
    }

    /// Files in or below the workspace folder are saved relative to it
    #[test]
    fn relative_inside() {
        let base = absolute(&["project"]);
        assert_eq!(to_relative(&absolute(&["project", "hero.png"]), &base), "hero.png");
        assert_eq!(to_relative(&absolute(&["project", "art", "hero.png"]), &base), "art/hero.png");
    }

    /// Files outside the workspace folder, like in a sibling folder, are kept absolute
    #[test]
    fn relative_sibling() {
        let base = absolute(&["project", "workspaces"]);
        let sprite = absolute(&["project", "art", "hero.png"]);
        assert_eq!(to_relative(&sprite, &base), sprite.to_string_lossy());
        assert_eq!(to_relative(&absolute(&["hero.png"]), &base), absolute(&["hero.png"]).to_string_lossy());
    }

    /// Older workspace files may have paths that go up a folder, which still resolve
    #[test]
    fn resolve_parent() {
        let base = absolute(&["project", "workspaces"]);
        assert!(resolve("../art/hero.png", &base).ends_with(Path::new("art").join("hero.png")));
    }

    /// Relative paths round trip back to the same file
    #[test]
    fn relative_round_trip() {
        let base = absolute(&["project"]);
        let sprite = absolute(&["project", "art", "hero.png"]);
        assert_eq!(resolve(&to_relative(&sprite, &base), &base), sprite);
    }

    /// Absolute paths are kept as they are when resolving
    #[test]
    fn resolve_absolute() {
        let sprite = absolute(&["elsewhere", "hero.png"]);
        assert_eq!(resolve(&sprite.to_string_lossy(), &absolute(&["project"])), sprite);
    }

    /// Paths on another drive are kept absolute
    #[cfg(windows)]
    #[test]
    fn relative_other_drive() {
        assert_eq!(to_relative(Path::new(r"D:\art\hero.png"), Path::new(r"C:\project")), r"D:\art\hero.png");
    }

    /// A minimal workspace file with the given format and version
    fn workspace_json(format: &str, version: u32) -> String {
        serde_json::json!({
            "format": format,
            "version": version,
            "spritesheet": "hero.png",
            "normal_map": "hero_normal.png",
            "sprite_size": [64, 32],
            "light": { "position": { "x": 1.0, "y": 2.0 }, "height": 200.0, "color": [255, 255, 255] },
            "zones": [],
        }).to_string()
    }

    /// Files of the current version can be read
    #[test]
    fn read_current() {
        let file = read_workspace(&workspace_json(FORMAT, VERSION)).unwrap();
        assert_eq!(file.spritesheet, "hero.png");
        assert_eq!(file.sprite_size, (64, 32));
        assert!(file.scene.objects.validate().is_ok());
    }

    /// A file that is read and written back out is equal to itself when read again, so opening a file doesn't count as a change.
    /// Uses values that aren't exact in binary, to check floats survive the round trip.
    #[test]
    fn round_trip_equal() {
        let json = serde_json::json!({
            "format": FORMAT,
            "version": VERSION,
            "spritesheet": "art/hero.png",
            "normal_map": "art/hero_normal.png",
            "sprite_size": [64, 32],
            "light": { "position": { "x": 0.1, "y": 33.333333 }, "height": 123.456, "color": [10, 20, 30] },
            "zones": [],
        }).to_string();
        let file = read_workspace(&json).unwrap();
        let again = read_workspace(&serde_json::to_string_pretty(&file).unwrap()).unwrap();
        assert_eq!(file, again);
    }

    /// Newer versions and other formats are rejected
    #[test]
    fn read_rejects() {
        assert!(read_workspace(&workspace_json(FORMAT, VERSION + 1)).is_err());
        assert!(read_workspace(&workspace_json("something-else", VERSION)).is_err());
        assert!(read_workspace("not json").is_err());
    }
}
