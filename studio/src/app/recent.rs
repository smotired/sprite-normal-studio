use std::path::{Path, PathBuf};

use anyhow::Context;

/// Most files kept in the list
const LIMIT: usize = 10;

/// Most files shown in the File > Recent Files menu
pub(super) const MENU_LIMIT: usize = 8;

/// Folder for the app's local data, inside the platform's local data folder
const APP_FOLDER: &str = "SpriteNormalStudio";

/// Name of the recent files list inside the app's folder
const FILE_NAME: &str = "recentfiles.json";

/// Workspace files that were recently opened or saved, newest first.
/// Kept in the platform's local data folder, i.e. %LOCALAPPDATA%\SpriteNormalStudio\recentfiles.json on Windows.
#[derive(Default)]
pub(super) struct RecentFiles {
    paths: Vec<PathBuf>,
}

impl RecentFiles {
    /// Load the list from disk. A missing or broken file gives an empty list.
    pub(super) fn load() -> Self {
        let paths = file_path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default();
        Self { paths }
    }

    /// The files, newest first
    pub(super) fn paths(&self) -> &[PathBuf] { &self.paths }

    /// Move a file to the front of the list after it was opened or saved, and save the list.
    /// The file is only ever in the list once.
    pub(super) fn mark_used(&mut self, path: &Path) {
        self.insert(path);
        self.save();
    }

    /// Remove a file from the list and save the list
    pub(super) fn remove(&mut self, path: &Path) {
        self.paths.retain(|recent| !same_file(recent, path));
        self.save();
    }

    /// Put a file at the front of the list, removing any other entry for the same file and keeping to the limit
    fn insert(&mut self, path: &Path) {
        let path = std::path::absolute(path).unwrap_or_else(|_| path.to_owned());
        self.paths.retain(|recent| !same_file(recent, &path));
        self.paths.insert(0, path);
        self.paths.truncate(LIMIT);
    }

    /// Write the list to disk. Failing to remember recent files shouldn't get in the way, so errors are only logged.
    fn save(&self) {
        if let Err(e) = self.try_save() {
            eprintln!("Could not save the recent files list: {e:#}");
        }
    }

    fn try_save(&self) -> anyhow::Result<()> {
        let path = file_path().context("There is no local data folder")?;
        if let Some(folder) = path.parent() {
            std::fs::create_dir_all(folder)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(&self.paths)?)?;
        Ok(())
    }
}

/// Where the list is kept, if the platform has a local data folder
fn file_path() -> Option<PathBuf> {
    dirs::data_local_dir().map(|dir| dir.join(APP_FOLDER).join(FILE_NAME))
}

/// If two paths name the same file, ignoring separators, and case on Windows
fn same_file(a: &Path, b: &Path) -> bool {
    normalize(a) == normalize(b)
}

/// A path in a form that can be compared
fn normalize(path: &Path) -> String {
    let normalized: PathBuf = path.components().collect();
    let normalized = normalized.to_string_lossy().into_owned();
    if cfg!(windows) { normalized.to_lowercase() } else { normalized }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An absolute path in a temporary folder, so tests work on every platform
    fn absolute(name: &str) -> PathBuf {
        std::env::temp_dir().join(name)
    }

    /// New files go to the front
    #[test]
    fn newest_first() {
        let mut recent = RecentFiles::default();
        recent.insert(&absolute("a.sns"));
        recent.insert(&absolute("b.sns"));
        assert_eq!(recent.paths(), [absolute("b.sns"), absolute("a.sns")]);
    }

    /// Using a file again moves it to the front instead of adding it twice
    #[test]
    fn no_duplicates() {
        let mut recent = RecentFiles::default();
        recent.insert(&absolute("a.sns"));
        recent.insert(&absolute("b.sns"));
        recent.insert(&absolute("a.sns"));
        assert_eq!(recent.paths(), [absolute("a.sns"), absolute("b.sns")]);
    }

    /// The same file written differently is still only in the list once
    #[cfg(windows)]
    #[test]
    fn no_duplicates_case_and_separators() {
        let mut recent = RecentFiles::default();
        recent.insert(Path::new(r"C:\Projects\Hero.sns"));
        recent.insert(Path::new("c:/projects/hero.sns"));
        assert_eq!(recent.paths().len(), 1);
    }

    /// Only the newest files are kept
    #[test]
    fn limit() {
        let mut recent = RecentFiles::default();
        for i in 0..LIMIT + 5 {
            recent.insert(&absolute(&format!("{i}.sns")));
        }
        assert_eq!(recent.paths().len(), LIMIT);
        assert_eq!(recent.paths()[0], absolute(&format!("{}.sns", LIMIT + 4)));
    }
}
