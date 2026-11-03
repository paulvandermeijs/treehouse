//! What an approval card shows: the change a file tool would make, or every
//! argument a tool is given. Worked out once, when the call arrives, so the
//! card doesn't read files on every frame.

use serde_json::Value;
use std::path::{Path, PathBuf};

use crate::approval::{Reason, normalize, resolve};

#[derive(Clone, Debug)]
pub enum Preview {
    /// A bash command, shown as it is.
    Command(String),
    File(FileChange),
    Args(Vec<Arg>),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FileAction {
    Edit,
    /// A write over a file that's there.
    Write,
    /// A write to a new file.
    Create,
}

#[derive(Clone, Debug)]
pub struct FileChange {
    pub action: FileAction,
    /// The path as Pi gave it.
    pub path: String,
    /// Where the path really leads, when a symlink takes it out of the
    /// worktree.
    pub link_target: Option<PathBuf>,
    /// The path is outside the worktree as written, not through a symlink.
    pub outside: bool,
    /// `.agents/linked` shares it with every worktree.
    pub shared: bool,
    pub patch: String,
    pub additions: usize,
    pub deletions: usize,
    /// The lines a write replaces, or a new file's lines.
    pub lines: usize,
    /// False when the edit's text wasn't found in the file as it is now:
    /// the diff then only shows the text it replaces, so line numbers would
    /// be made up.
    pub numbered: bool,
}

/// A value longer than this is cut, with the rest one click away.
pub const MAX_ARG_LINES: usize = 6;
pub const MAX_ARG_CHARS: usize = 400;

#[derive(Clone, Debug)]
pub struct Arg {
    pub name: String,
    pub value: String,
    /// The start of a long value and what's left out, such as
    /// "… 31 more lines".
    pub cut: Option<(String, String)>,
}

/// `mcp__<server>__<tool>` as `(server, tool)`.
pub fn mcp_parts(name: &str) -> Option<(&str, &str)> {
    let rest = name.strip_prefix("mcp__")?;
    Some(rest.split_once("__").unwrap_or(("", rest)))
}

pub fn preview(tool: &str, input: &Value, worktree: &Path, reason: Reason) -> Preview {
    let path = input.get("path").and_then(Value::as_str);
    match (tool, path) {
        ("bash", _) => Preview::Command(input["command"].as_str().unwrap_or("").to_string()),
        ("edit" | "write", Some(path)) => {
            Preview::File(file_change(tool, input, path, worktree, reason))
        }
        _ => Preview::Args(args(input)),
    }
}

fn file_change(
    tool: &str,
    input: &Value,
    path: &str,
    worktree: &Path,
    reason: Reason,
) -> FileChange {
    let absolute = normalize(&worktree.join(path));
    let current = std::fs::read(&absolute)
        .ok()
        .map(|bytes| String::from_utf8_lossy(&bytes).into_owned());

    let (action, patch, numbered) = if tool == "edit" {
        let edits = edits(input);
        match current.as_deref().and_then(|text| apply(text, &edits)) {
            Some(new) => (
                FileAction::Edit,
                crate::git::unified_patch(path, current.as_deref(), &new),
                true,
            ),
            // Pi may still place it, matching more loosely than we do.
            None => {
                let patch = edits
                    .iter()
                    .map(|(old, new)| crate::git::unified_patch(path, Some(old), new))
                    .collect::<Vec<_>>()
                    .join("\n");
                (FileAction::Edit, patch, false)
            }
        }
    } else {
        let content = input["content"].as_str().unwrap_or("");
        let action = if current.is_some() {
            FileAction::Write
        } else {
            FileAction::Create
        };
        (
            action,
            crate::git::unified_patch(path, current.as_deref(), content),
            true,
        )
    };
    let stat = crate::git::diff_stat(&patch);
    let lines = match action {
        FileAction::Create => input["content"].as_str().unwrap_or("").lines().count(),
        _ => current.as_deref().map(|c| c.lines().count()).unwrap_or(0),
    };

    let (link_target, outside) = if reason == Reason::OutsideWorktree {
        // The worktree's own folder may be reached through a symlink, such
        // as /tmp on macOS; only a link below it counts.
        match absolute.strip_prefix(worktree) {
            Ok(rest) => {
                let real = resolve(&absolute);
                (
                    Some(real).filter(|real| *real != resolve(worktree).join(rest)),
                    false,
                )
            }
            Err(_) => (None, true),
        }
    } else {
        (None, false)
    };
    let shared = absolute
        .strip_prefix(worktree)
        .is_ok_and(|rest| crate::setup::is_linked(worktree, rest));

    FileChange {
        action,
        path: path.to_string(),
        link_target,
        outside,
        shared,
        patch,
        additions: stat.additions,
        deletions: stat.deletions,
        lines,
        numbered,
    }
}

/// Pi's edit takes a list of replacements; older versions took one.
fn edits(input: &Value) -> Vec<(String, String)> {
    let pair = |edit: &Value| {
        Some((
            edit.get("oldText")?.as_str()?.to_string(),
            edit.get("newText")?.as_str()?.to_string(),
        ))
    };
    match input.get("edits").and_then(Value::as_array) {
        Some(list) => list.iter().filter_map(pair).collect(),
        None => pair(input).into_iter().collect(),
    }
}

/// The file after the edits, or `None` when one of them isn't found.
fn apply(text: &str, edits: &[(String, String)]) -> Option<String> {
    let mut text = text.to_string();
    for (old, new) in edits {
        if old.is_empty() || !text.contains(old.as_str()) {
            return None;
        }
        text = text.replacen(old.as_str(), new, 1);
    }
    Some(text)
}

fn args(input: &Value) -> Vec<Arg> {
    let Some(object) = input.as_object() else {
        return match input {
            Value::Null => Vec::new(),
            other => vec![arg("input", other)],
        };
    };
    object
        .iter()
        .map(|(name, value)| arg(name, value))
        .collect()
}

fn arg(name: &str, value: &Value) -> Arg {
    let value = match value {
        Value::String(text) => text.clone(),
        Value::Object(_) | Value::Array(_) => {
            serde_json::to_string_pretty(value).unwrap_or_default()
        }
        other => other.to_string(),
    };
    Arg {
        name: name.to_string(),
        cut: cut(&value),
        value,
    }
}

fn cut(value: &str) -> Option<(String, String)> {
    let total_lines = value.lines().count();
    if total_lines <= MAX_ARG_LINES && value.chars().count() <= MAX_ARG_CHARS {
        return None;
    }
    let mut shown: String = value
        .lines()
        .take(MAX_ARG_LINES)
        .collect::<Vec<_>>()
        .join("\n");
    if let Some((at, _)) = shown.char_indices().nth(MAX_ARG_CHARS) {
        shown.truncate(at);
    }
    let shown_lines = shown.lines().count();
    let more = match total_lines.saturating_sub(shown_lines) {
        0 => format!(
            "… {} more characters",
            value.chars().count() - shown.chars().count()
        ),
        1 => "… 1 more line".to_string(),
        n => format!("… {n} more lines"),
    };
    Some((shown, more))
}

#[cfg(test)]
mod tests {
    use super::{FileAction, Preview, cut, mcp_parts, preview};
    use crate::approval::Reason;
    use serde_json::json;

    fn file(preview: Preview) -> super::FileChange {
        match preview {
            Preview::File(change) => change,
            other => panic!("not a file change: {other:?}"),
        }
    }

    #[test]
    fn an_edit_is_diffed_against_the_file() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a.rs"), "one\ntwo\nthree\n").unwrap();
        let input = json!({"path": "a.rs", "edits": [{"oldText": "two", "newText": "2\n2b"}]});
        let change = file(preview("edit", &input, dir.path(), Reason::AskEveryTool));
        assert_eq!(change.action, FileAction::Edit);
        assert!(change.numbered);
        assert_eq!((change.additions, change.deletions), (2, 1));
        assert!(change.patch.contains("@@ -1,3 +1,4 @@"), "{}", change.patch);

        // Text that isn't in the file still shows what it replaces.
        let input = json!({"path": "a.rs", "oldText": "four", "newText": "4"});
        let change = file(preview("edit", &input, dir.path(), Reason::AskEveryTool));
        assert!(!change.numbered);
        assert_eq!((change.additions, change.deletions), (1, 1));
    }

    #[test]
    fn a_write_is_a_new_file_or_a_rewrite() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("old.txt"), "a\nb\nc\n").unwrap();
        let rewrite = json!({"path": "old.txt", "content": "a\nB\nc\n"});
        let change = file(preview("write", &rewrite, dir.path(), Reason::AskEveryTool));
        assert_eq!(change.action, FileAction::Write);
        assert_eq!(
            (change.lines, change.additions, change.deletions),
            (3, 1, 1)
        );

        let new = json!({"path": "new.txt", "content": "x\ny\n"});
        let change = file(preview("write", &new, dir.path(), Reason::AskEveryTool));
        assert_eq!(change.action, FileAction::Create);
        assert_eq!((change.lines, change.additions), (2, 2));
    }

    #[cfg(unix)]
    #[test]
    fn a_link_out_of_the_worktree_shows_where_it_goes() {
        let dir = tempfile::tempdir().unwrap();
        let worktree = dir.path().join("wt");
        let shared = dir.path().join("main");
        std::fs::create_dir_all(worktree.join(".agents")).unwrap();
        std::fs::create_dir_all(&shared).unwrap();
        std::fs::write(shared.join(".env"), "KEY=1\n").unwrap();
        std::os::unix::fs::symlink(shared.join(".env"), worktree.join(".env")).unwrap();
        std::fs::write(worktree.join(".agents/linked"), ".env\n").unwrap();

        let input = json!({"path": ".env", "content": "KEY=2\n"});
        let change = file(preview("write", &input, &worktree, Reason::OutsideWorktree));
        assert_eq!(
            change.link_target,
            Some(shared.join(".env").canonicalize().unwrap())
        );
        assert!(change.shared && !change.outside);
        assert_eq!(change.action, FileAction::Write);

        let input = json!({"path": "../elsewhere.txt", "content": "x\n"});
        let change = file(preview("write", &input, &worktree, Reason::OutsideWorktree));
        assert!(change.link_target.is_none() && change.outside && !change.shared);
    }

    #[test]
    fn arguments_are_listed_as_text() {
        let input =
            json!({"channel": "#releases", "unfurl": true, "count": 3, "blocks": [{"a": 1}]});
        let Preview::Args(args) = preview(
            "mcp__slack__send_message",
            &input,
            "/".as_ref(),
            Reason::AskEveryTool,
        ) else {
            panic!("not arguments");
        };
        let value = |name: &str| args.iter().find(|a| a.name == name).unwrap().value.clone();
        assert_eq!(value("channel"), "#releases");
        assert_eq!(value("unfurl"), "true");
        assert_eq!(value("count"), "3");
        assert_eq!(value("blocks"), "[\n  {\n    \"a\": 1\n  }\n]");
        assert_eq!(
            mcp_parts("mcp__slack__send_message"),
            Some(("slack", "send_message"))
        );
        assert_eq!(mcp_parts("read"), None);
    }

    #[test]
    fn long_values_are_cut_saying_how_much() {
        assert_eq!(cut("short"), None);
        let script = (1..=40)
            .map(|n| format!("line {n}"))
            .collect::<Vec<_>>()
            .join("\n");
        let (shown, more) = cut(&script).unwrap();
        assert_eq!(shown.lines().count(), 6);
        assert_eq!(more, "… 34 more lines");
        let (shown, more) = cut(&"x".repeat(450)).unwrap();
        assert_eq!(shown.len(), 400);
        assert_eq!(more, "… 50 more characters");
    }
}
