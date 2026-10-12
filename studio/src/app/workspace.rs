use std::path::{Component, Path, PathBuf};

use anyhow::Context;
use controller::SavedScene;
use eframe::egui;
use serde::{Deserialize, Serialize};

use crate::app::StudioApp;
use crate::app::file::loader::FLAT_NORMAL_COLOR;

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
    Open,
    Save,
    SaveAs,
}

impl WorkspaceCommand {
    /// Keyboard shortcut for this command
    pub(super) const fn shortcut(self) -> egui::KeyboardShortcut {
        match self {
            Self::Open   => egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::O),
            Self::Save   => egui::KeyboardShortcut::new(egui::Modifiers::COMMAND, egui::Key::S),
            Self::SaveAs => egui::KeyboardShortcut::new(egui::Modifiers::COMMAND.plus(egui::Modifiers::SHIFT), egui::Key::S),
        }
    }

    /// Every command, with the most specific shortcuts first so they are matched before ones they contain
    pub(super) const ALL: [Self; 3] = [Self::SaveAs, Self::Save, Self::Open];
}

impl StudioApp {
    /// Run a workspace command, showing a dialog for anything that went wrong
    pub(super) fn run_workspace_command(&mut self, ctx: &egui::Context, command: WorkspaceCommand) {
        let result = match command {
            WorkspaceCommand::Open => match self.workspace_file_dialog().pick_file() {
                Some(path) => self.open_workspace(&path).with_context(|| format!("Could not open {}", path.display())),
                None => return,
            },
            WorkspaceCommand::Save if self.workspace_path.is_some() => {
                let path = self.workspace_path.clone().unwrap();
                self.save_workspace(&path).with_context(|| format!("Could not save {}", path.display()))
            },
            WorkspaceCommand::Save | WorkspaceCommand::SaveAs => match self.save_as_dialog().save_file() {
                Some(path) => self.save_workspace(&path).with_context(|| format!("Could not save {}", path.display())),
                None => return,
            },
        };

        match result {
            Ok(Some(warning)) => show_message(rfd::MessageLevel::Warning, "Warning", &warning),
            Ok(None) => {},
            Err(e) => show_message(rfd::MessageLevel::Error, "Error", &format!("{e:#}")),
        }
        ctx.send_viewport_cmd(egui::ViewportCommand::Title(self.window_title()));
    }

    /// Title for the window, with the name of the workspace file if there is one
    fn window_title(&self) -> String {
        match self.workspace_path.as_deref().and_then(Path::file_name) {
            Some(name) => format!("{} - {APP_TITLE}", name.to_string_lossy()),
            None => APP_TITLE.to_owned(),
        }
    }

    /// File dialog for workspace files, starting in the current workspace's folder or the spritesheet's folder
    fn workspace_file_dialog(&self) -> rfd::FileDialog {
        let directory = self.workspace_path.as_deref()
            .or(self.sprite_path.path())
            .and_then(Path::parent)
            .unwrap_or(Path::new("/"));
        rfd::FileDialog::new()
            .add_filter("Sprite Normal Studio workspace", &[EXTENSION])
            .set_directory(directory)
    }

    /// Save dialog, suggesting the current workspace's name or one named after the spritesheet
    fn save_as_dialog(&self) -> rfd::FileDialog {
        let name = self.workspace_path.as_deref()
            .or(self.sprite_path.path())
            .and_then(Path::file_stem)
            .map(|stem| format!("{}.{EXTENSION}", stem.to_string_lossy()));
        let dialog = self.workspace_file_dialog();
        match name {
            Some(name) => dialog.set_file_name(name),
            None => dialog,
        }
    }

    /// Save the workspace to a file and make it the current workspace file
    fn save_workspace(&mut self, path: &Path) -> anyhow::Result<Option<String>> {
        let Some(sprite) = self.sprite_path.path() else { anyhow::bail!("Select a spritesheet before saving."); };
        let Some(normal) = self.normal_path.path() else { anyhow::bail!("Select a normal map before saving."); };
        let base = path.parent().unwrap_or(Path::new(""));

        let file = WorkspaceFile {
            format: FORMAT.to_owned(),
            version: VERSION,
            spritesheet: to_relative(sprite, base),
            normal_map: to_relative(normal, base),
            sprite_size: self.sprite_path.size(),
            scene: self.controller.save_scene(),
        };
        let json = serde_json::to_string_pretty(&file)?;

        // Write to a temporary file first, so a failed write can't leave a broken workspace file behind
        let mut temp_path = path.as_os_str().to_owned();
        temp_path.push(".tmp");
        std::fs::write(&temp_path, json).context("Could not write the file")?;
        std::fs::rename(&temp_path, path).context("Could not replace the file")?;

        self.workspace_path = Some(path.to_owned());
        Ok(None)
    }

    /// Open a workspace file and make it the current workspace file. Nothing changes if it can't be opened.
    /// Returns a warning if it opened but something may be wrong.
    fn open_workspace(&mut self, path: &Path) -> anyhow::Result<Option<String>> {
        let file = read_workspace(&std::fs::read_to_string(path)?)?;
        let objects = file.scene.objects.validate().context("The workspace file has invalid objects")?;
        let base = path.parent().unwrap_or(Path::new(""));

        // Load the spritesheet, which is the first thing to change
        let sprite = resolve(&file.spritesheet, base);
        let size = self.sprite_path.try_select(sprite.clone(), &self.render_state)
            .with_context(|| format!("Could not load the spritesheet {}", sprite.display()))?;

        // Load the normal map, or start a flat one if it doesn't exist yet
        self.normal_path.select_or_fill(resolve(&file.normal_map, base), &self.render_state, size, FLAT_NORMAL_COLOR);

        self.update_input_textures();
        self.controller.load_scene(file.scene.light, objects);
        self.workspace_path = Some(path.to_owned());

        Ok((size != file.sprite_size).then(|| format!(
            "The spritesheet is {}x{}, but was {}x{} when the workspace was saved. Zones may not line up with it.",
            size.0, size.1, file.sprite_size.0, file.sprite_size.1,
        )))
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
/// Paths on a different drive or root are kept absolute.
fn to_relative(path: &Path, base: &Path) -> String {
    let same_root = path.components().next() == base.components().next();
    match pathdiff::diff_paths(path, base).filter(|_| same_root) {
        Some(relative) => relative.components()
            .map(|component| component.as_os_str().to_string_lossy())
            .collect::<Vec<_>>()
            .join("/"),
        None => path.to_string_lossy().into_owned(),
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

    /// Files in a sibling folder go up first
    #[test]
    fn relative_sibling() {
        let base = absolute(&["project", "workspaces"]);
        let sprite = absolute(&["project", "art", "hero.png"]);
        let relative = to_relative(&sprite, &base);
        assert_eq!(relative, "../art/hero.png");
        assert!(resolve(&relative, &base).ends_with(Path::new("art").join("hero.png")));
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
