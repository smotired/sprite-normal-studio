use std::path::{Component, Path, PathBuf};

use anyhow::Context;
use controller::{LoadedObjects, SavedLight, SavedScene};
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

/// A workspace file: the input files, plus the objects and light. Not the camera or the view.
#[derive(Serialize, Deserialize)]
struct WorkspaceFile {
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

        let result = match command {
            WorkspaceCommand::New => {
                self.new_project_dialog = Some(Default::default());
                return;
            },
            WorkspaceCommand::Close => {
                self.set_project(ctx, None);
                return;
            },
            WorkspaceCommand::Open => match self.workspace_file_dialog().pick_file() {
                Some(path) => self.open_workspace(ctx, &path).with_context(|| format!("Could not open {}", path.display())),
                None => return,
            },
            WorkspaceCommand::Save if self.project.as_ref().is_some_and(|project| project.saved) => {
                let path = self.project.as_ref().unwrap().path.clone();
                self.save_workspace(&path).with_context(|| format!("Could not save {}", path.display()))
            },
            WorkspaceCommand::Save | WorkspaceCommand::SaveAs => match self.save_as_dialog().save_file() {
                Some(path) => self.save_workspace(&path).with_context(|| format!("Could not save {}", path.display())),
                None => return,
            },
        };

        show_result(result);
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
    }

    /// Open a workspace file from the recent files list, showing a dialog for anything that went wrong
    pub(super) fn open_recent(&mut self, ctx: &egui::Context, path: &Path) {
        let result = self.open_workspace(ctx, path).with_context(|| format!("Could not open {}", path.display()));
        show_result(result);
    }

    /// Title for the window, with the name of the workspace file if there is one
    pub(super) fn window_title(&self) -> String {
        match self.project.as_ref().and_then(|project| project.path.file_name()) {
            Some(name) => format!("{} - {APP_TITLE}", name.to_string_lossy()),
            None => APP_TITLE.to_owned(),
        }
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

    /// Save the workspace to a file and make it the current workspace file
    fn save_workspace(&mut self, path: &Path) -> anyhow::Result<Option<String>> {
        let Some(project) = &mut self.project else { anyhow::bail!("There is no open project to save."); };
        let base = path.parent().unwrap_or(Path::new(""));

        let file = WorkspaceFile {
            format: FORMAT.to_owned(),
            version: VERSION,
            spritesheet: to_relative(project.sprite.path(), base),
            normal_map: to_relative(project.normal.path(), base),
            sprite_size: project.sprite.size(),
            scene: self.controller.save_scene(),
        };
        let json = serde_json::to_string_pretty(&file)?;

        // Write to a temporary file first, so a failed write can't leave a broken workspace file behind
        let mut temp_path = path.as_os_str().to_owned();
        temp_path.push(".tmp");
        std::fs::write(&temp_path, json).context("Could not write the file")?;
        std::fs::rename(&temp_path, path).context("Could not replace the file")?;

        project.path = path.to_owned();
        project.saved = true;
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
            light: file.scene.light,
            objects,
            sprite_size: file.sprite_size,
            sprite_path: resolve(&file.spritesheet, base),
            normal_path: resolve(&file.normal_map, base),
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
        self.set_project(ctx, Some(Project { sprite, normal, path: pending.path.clone(), saved: true }));
        self.controller.load_scene(pending.light, pending.objects);
        self.recent.mark_used(&pending.path);

        (size != pending.sprite_size).then(|| format!(
            "The spritesheet is {}x{}, but was {}x{} when the workspace was saved. Zones may not line up with it.",
            size.0, size.1, pending.sprite_size.0, pending.sprite_size.1,
        ))
    }
}

/// A workspace file that has been read, but whose images haven't all been loaded yet
pub(super) struct PendingOpen {
    /// The workspace file
    pub(super) path: PathBuf,
    light: SavedLight,
    objects: LoadedObjects,
    /// Size of the spritesheet when saved
    sprite_size: (u32, u32),

    /// Where the workspace file says the images are
    pub(super) sprite_path: PathBuf,
    pub(super) normal_path: PathBuf,
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

    /// Newer versions and other formats are rejected
    #[test]
    fn read_rejects() {
        assert!(read_workspace(&workspace_json(FORMAT, VERSION + 1)).is_err());
        assert!(read_workspace(&workspace_json("something-else", VERSION)).is_err());
        assert!(read_workspace("not json").is_err());
    }
}
