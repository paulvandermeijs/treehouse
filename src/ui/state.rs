//! Window state that comes back after a restart: what's folded in the
//! sidebar, whether the sidebar and the diff are open. Kept apart from the
//! settings, in `ui.json` in the data folder, since nobody edits it by hand.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

use crate::config::Config;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiState {
    pub collapsed_projects: Vec<PathBuf>,
    pub collapsed_worktrees: Vec<PathBuf>,
    pub sidebar_open: bool,
    pub diff_open: bool,
}

impl Default for UiState {
    fn default() -> Self {
        Self {
            collapsed_projects: Vec::new(),
            collapsed_worktrees: Vec::new(),
            sidebar_open: true,
            diff_open: false,
        }
    }
}

fn path() -> PathBuf {
    Config::data_dir().join("ui.json")
}

impl UiState {
    pub fn load() -> Self {
        Self::read(&path())
    }

    pub fn save(&self) {
        self.write(&path());
    }

    fn read(path: &Path) -> Self {
        std::fs::read_to_string(path)
            .ok()
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    fn write(&self, path: &Path) {
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::UiState;
    use std::path::PathBuf;

    #[test]
    fn missing_or_old_files_fill_in_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ui.json");
        assert_eq!(UiState::read(&file), UiState::default());

        std::fs::write(&file, r#"{"collapsed_projects":["/code/app"]}"#).unwrap();
        let state = UiState::read(&file);
        assert_eq!(state.collapsed_projects, [PathBuf::from("/code/app")]);
        assert!(state.sidebar_open);
        assert!(!state.diff_open);
    }

    #[test]
    fn round_trips() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("ui.json");
        let state = UiState {
            collapsed_projects: vec!["/code/app".into()],
            collapsed_worktrees: vec!["/code/app/.worktrees/fix".into()],
            sidebar_open: false,
            diff_open: true,
        };
        state.write(&file);
        assert_eq!(UiState::read(&file), state);
    }
}
