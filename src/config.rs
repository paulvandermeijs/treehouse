//! User settings, saved as JSON in the platform config folder.

use gpui_kit::{App, Global};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

use crate::approval::{AllowedPattern, AlwaysAsk, ApprovalMode};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThemeChoice {
    #[default]
    Mocha,
    Latte,
    System,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompareBase {
    /// The branch the worktree was created from.
    #[default]
    CreatedFrom,
    OriginMain,
}

pub const THINKING_LEVELS: [&str; 5] = ["off", "minimal", "low", "medium", "high"];
pub const TOOLS: [&str; 4] = ["read", "bash", "edit", "write"];

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub projects: Vec<PathBuf>,
    /// Projects shown first in the sidebar, in the order they were pinned.
    pub pinned: Vec<PathBuf>,

    pub theme: ThemeChoice,
    pub text_size: f32,
    pub keep_diffs_expanded: bool,
    pub always_expand_thinking: bool,
    pub animate_working: bool,

    /// The `pi` executable.
    pub pi_command: String,
    /// `provider/model`; `None` lets Pi pick its default.
    pub model: Option<String>,
    /// `None` means the same as the chat.
    pub subagent_model: Option<String>,
    pub thinking: String,
    pub tools: Vec<String>,

    pub approval: ApprovalMode,
    pub allowed: Vec<AllowedPattern>,
    pub always_ask: AlwaysAsk,

    pub worktree_location: String,
    pub compare_base: CompareBase,
    pub setup_command: String,
    pub delete_on_remove: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            projects: Vec::new(),
            pinned: Vec::new(),
            theme: ThemeChoice::Mocha,
            text_size: 14.0,
            keep_diffs_expanded: false,
            always_expand_thinking: false,
            animate_working: true,
            pi_command: "pi".into(),
            model: None,
            subagent_model: None,
            thinking: "high".into(),
            tools: TOOLS.iter().map(|t| t.to_string()).collect(),
            approval: ApprovalMode::AskBeforeBash,
            allowed: Vec::new(),
            always_ask: AlwaysAsk::default(),
            worktree_location: "{project}/.worktrees/{branch}".into(),
            compare_base: CompareBase::CreatedFrom,
            setup_command: String::new(),
            delete_on_remove: false,
        }
    }
}

impl Global for Config {}

impl Config {
    pub fn path() -> Option<PathBuf> {
        dirs::config_dir().map(|dir| dir.join("treehouse").join("settings.json"))
    }

    /// Where Treehouse keeps Pi sessions and its extension.
    pub fn data_dir() -> PathBuf {
        dirs::data_dir()
            .unwrap_or_else(std::env::temp_dir)
            .join("treehouse")
    }

    pub fn load() -> Self {
        Self::path()
            .and_then(|path| std::fs::read_to_string(path).ok())
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    }

    pub fn save(&self) {
        let Some(path) = Self::path() else { return };
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(json) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(path, json);
        }
    }

    pub fn global(cx: &App) -> &Self {
        cx.global::<Self>()
    }

    /// Edits the settings and saves them.
    pub fn update(cx: &mut App, edit: impl FnOnce(&mut Self)) {
        let config = cx.global_mut::<Self>();
        edit(config);
        config.save();
        cx.refresh_windows();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_files_fill_in_defaults() {
        let config: Config =
            serde_json::from_str(r#"{"theme":"latte","projects":["/code/app"]}"#).unwrap();
        assert_eq!(config.theme, ThemeChoice::Latte);
        assert_eq!(config.projects, [PathBuf::from("/code/app")]);
        assert_eq!(config.approval, ApprovalMode::AskBeforeBash);
        assert_eq!(config.worktree_location, "{project}/.worktrees/{branch}");
    }
}
