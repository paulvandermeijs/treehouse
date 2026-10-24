//! Tool approval policy.
//!
//! Pi has no built-in approval, so the Treehouse bridge extension sends every
//! tool call here first. This module decides, from the user's settings, whether
//! a call runs straight away or waits for the user.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::path::Path;

/// When Pi has to ask before using a tool. Matches Settings › Agent defaults.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalMode {
    #[default]
    AskBeforeBash,
    AskEveryTool,
    DontAsk,
}

impl ApprovalMode {
    pub const ALL: [ApprovalMode; 3] = [Self::AskBeforeBash, Self::AskEveryTool, Self::DontAsk];

    pub fn label(self) -> &'static str {
        match self {
            Self::AskBeforeBash => "Ask before bash",
            Self::AskEveryTool => "Ask before every tool call",
            Self::DontAsk => "Don't ask",
        }
    }
}

/// A command pattern added with "Always allow" on a bash approval.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllowedPattern {
    pub pattern: String,
    /// The project folder the pattern applies to; `None` means all projects.
    pub project: Option<std::path::PathBuf>,
}

/// The "always ask, whatever the approval setting" toggles.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AlwaysAsk {
    pub deleting: bool,
    pub pushing: bool,
    pub outside_worktree: bool,
}

impl Default for AlwaysAsk {
    fn default() -> Self {
        Self {
            deleting: true,
            pushing: true,
            outside_worktree: true,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Ask,
}

/// Everything the policy needs to know about one tool call.
pub struct ToolCallRequest<'a> {
    pub tool_name: &'a str,
    pub input: &'a Value,
    pub worktree: &'a Path,
    pub project: &'a Path,
}

pub struct Policy<'a> {
    pub mode: ApprovalMode,
    pub allowed: &'a [AllowedPattern],
    pub always_ask: &'a AlwaysAsk,
}

impl Policy<'_> {
    pub fn decide(&self, call: &ToolCallRequest) -> Decision {
        // The sub-agent tool only starts another session in the same worktree;
        // that session's own tool calls go through this policy.
        if call.tool_name == "subagent" {
            return Decision::Allow;
        }

        if self.always_ask.outside_worktree && touches_outside(call) {
            return Decision::Ask;
        }

        if call.tool_name == "bash" {
            let command = call.input.get("command").and_then(Value::as_str).unwrap_or("");
            if self.always_ask.deleting && is_deleting(command) {
                return Decision::Ask;
            }
            if self.always_ask.pushing && is_pushing(command) {
                return Decision::Ask;
            }
            if self.is_allowed(command, call.project) {
                return Decision::Allow;
            }
            return match self.mode {
                ApprovalMode::DontAsk => Decision::Allow,
                _ => Decision::Ask,
            };
        }

        match self.mode {
            ApprovalMode::AskEveryTool => Decision::Ask,
            _ => Decision::Allow,
        }
    }

    fn is_allowed(&self, command: &str, project: &Path) -> bool {
        self.allowed.iter().any(|allowed| {
            allowed.project.as_deref().is_none_or(|p| p == project)
                && pattern_matches(&allowed.pattern, command)
        })
    }
}

/// Proposes an "Always allow" pattern from the command's first two words,
/// for example `cargo add gpui-kit@0.4` becomes `cargo add *`.
pub fn propose_pattern(command: &str) -> String {
    let words: Vec<&str> = command.split_whitespace().take(2).collect();
    match words.as_slice() {
        [] => "*".to_string(),
        [one] => format!("{one} *"),
        [one, two] => format!("{one} {two} *"),
        _ => unreachable!(),
    }
}

/// Matches a command against a pattern where `*` stands for any text.
pub fn pattern_matches(pattern: &str, command: &str) -> bool {
    let command = command.trim();
    let pattern = pattern.trim();
    // "cargo test *" should also match a bare "cargo test".
    if let Some(prefix) = pattern.strip_suffix(" *") {
        if command == prefix {
            return true;
        }
    }
    glob(pattern.as_bytes(), command.as_bytes())
}

fn glob(pattern: &[u8], text: &[u8]) -> bool {
    match pattern.split_first() {
        None => text.is_empty(),
        Some((b'*', rest)) => (0..=text.len()).any(|i| glob(rest, &text[i..])),
        Some((c, rest)) => text.first() == Some(c) && glob(rest, &text[1..]),
    }
}

fn shell_words(command: &str) -> impl Iterator<Item = &str> {
    command
        .split(|c: char| c.is_whitespace() || matches!(c, ';' | '&' | '|' | '(' | ')'))
        .filter(|w| !w.is_empty())
}

fn is_deleting(command: &str) -> bool {
    let words: Vec<&str> = shell_words(command).collect();
    words.iter().enumerate().any(|(i, w)| {
        matches!(*w, "rm" | "rmdir" | "unlink" | "shred")
            || (*w == "git" && matches!(words.get(i + 1), Some(&"clean") | Some(&"rm")))
            || (*w == "find" && words[i..].contains(&"-delete"))
    })
}

fn is_pushing(command: &str) -> bool {
    let words: Vec<&str> = shell_words(command).collect();
    words.iter().enumerate().any(|(i, w)| {
        (*w == "git" && words.get(i + 1) == Some(&"push"))
            || matches!(*w, "deploy")
            || (matches!(*w, "npm" | "cargo" | "pnpm" | "yarn")
                && words.get(i + 1) == Some(&"publish"))
    })
}

fn touches_outside(call: &ToolCallRequest) -> bool {
    let Some(path) = call.input.get("path").and_then(Value::as_str) else {
        return false;
    };
    let path = Path::new(path);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        call.worktree.join(path)
    };
    !normalize(&absolute).starts_with(normalize(call.worktree))
}

/// Resolves `.` and `..` without touching the file system.
fn normalize(path: &Path) -> std::path::PathBuf {
    use std::path::Component;
    let mut out = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn decide(mode: ApprovalMode, allowed: &[AllowedPattern], tool: &str, input: Value) -> Decision {
        let always_ask = AlwaysAsk::default();
        let policy = Policy {
            mode,
            allowed,
            always_ask: &always_ask,
        };
        policy.decide(&ToolCallRequest {
            tool_name: tool,
            input: &input,
            worktree: Path::new("/code/app/.worktrees/feat"),
            project: Path::new("/code/app"),
        })
    }

    #[test]
    fn asks_before_bash_only() {
        let mode = ApprovalMode::AskBeforeBash;
        assert_eq!(decide(mode, &[], "bash", json!({"command": "ls"})), Decision::Ask);
        assert_eq!(decide(mode, &[], "read", json!({"path": "src/main.rs"})), Decision::Allow);
        assert_eq!(decide(mode, &[], "edit", json!({"path": "src/main.rs"})), Decision::Allow);
    }

    #[test]
    fn every_tool_and_never() {
        let read = json!({"path": "src/main.rs"});
        assert_eq!(decide(ApprovalMode::AskEveryTool, &[], "read", read.clone()), Decision::Ask);
        assert_eq!(decide(ApprovalMode::DontAsk, &[], "read", read), Decision::Allow);
        assert_eq!(decide(ApprovalMode::DontAsk, &[], "bash", json!({"command": "ls"})), Decision::Allow);
    }

    #[test]
    fn allowed_patterns_respect_project() {
        let allowed = [
            AllowedPattern { pattern: "cargo test *".into(), project: None },
            AllowedPattern { pattern: "npm run *".into(), project: Some("/code/site".into()) },
        ];
        let mode = ApprovalMode::AskBeforeBash;
        assert_eq!(decide(mode, &allowed, "bash", json!({"command": "cargo test -p diff"})), Decision::Allow);
        assert_eq!(decide(mode, &allowed, "bash", json!({"command": "cargo test"})), Decision::Allow);
        assert_eq!(decide(mode, &allowed, "bash", json!({"command": "npm run dev"})), Decision::Ask);
    }

    #[test]
    fn dangerous_commands_always_ask() {
        let allowed = [AllowedPattern { pattern: "*".into(), project: None }];
        let mode = ApprovalMode::DontAsk;
        assert_eq!(decide(mode, &allowed, "bash", json!({"command": "rm -rf target"})), Decision::Ask);
        assert_eq!(decide(mode, &allowed, "bash", json!({"command": "cd x && git clean -fd"})), Decision::Ask);
        assert_eq!(decide(mode, &allowed, "bash", json!({"command": "git push origin main"})), Decision::Ask);
        assert_eq!(decide(mode, &allowed, "bash", json!({"command": "git status"})), Decision::Allow);
    }

    #[test]
    fn files_outside_the_worktree_always_ask() {
        let mode = ApprovalMode::DontAsk;
        assert_eq!(decide(mode, &[], "write", json!({"path": "../../other/file"})), Decision::Ask);
        assert_eq!(decide(mode, &[], "read", json!({"path": "/etc/hosts"})), Decision::Ask);
        assert_eq!(decide(mode, &[], "read", json!({"path": "./src/../Cargo.toml"})), Decision::Allow);
    }

    #[test]
    fn proposes_patterns_from_two_words() {
        assert_eq!(propose_pattern("cargo add gpui-kit@0.4 --features editor"), "cargo add *");
        assert_eq!(propose_pattern("ls"), "ls *");
        assert!(pattern_matches("git status", "git status"));
        assert!(!pattern_matches("git status", "git status --short"));
    }
}
