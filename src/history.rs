//! Which Pi session file holds each worktree's chat, so the chat comes back
//! after a restart.
//!
//! Pi saves every session as a JSONL file but has no notion of worktrees,
//! and sub-agents share their worktree's folder, so Treehouse keeps its own
//! index: `sessions.json` in its data folder, worktree path → session file.

use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::Config;

fn index_path() -> PathBuf {
    Config::data_dir().join("sessions.json")
}

fn read_index(path: &Path) -> BTreeMap<PathBuf, PathBuf> {
    std::fs::read_to_string(path)
        .ok()
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

fn write_index(path: &Path, index: &BTreeMap<PathBuf, PathBuf>) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(index) {
        let _ = std::fs::write(path, json);
    }
}

/// The worktree's saved session, if its file still exists.
pub fn session_file(worktree: &Path) -> Option<PathBuf> {
    read_index(&index_path())
        .remove(worktree)
        .filter(|file| file.exists())
}

pub fn remember(worktree: &Path, file: &Path) {
    let path = index_path();
    let mut index = read_index(&path);
    if index.get(worktree).map(PathBuf::as_path) != Some(file) {
        index.insert(worktree.to_path_buf(), file.to_path_buf());
        write_index(&path, &index);
    }
}

/// Drops a deleted worktree. Pi's session file stays, like Pi's own.
pub fn forget(worktree: &Path) {
    let path = index_path();
    let mut index = read_index(&path);
    if index.remove(worktree).is_some() {
        write_index(&path, &index);
    }
}

/// The entries of a session file's current branch, oldest first.
pub fn load(file: &Path) -> Vec<Value> {
    std::fs::read_to_string(file)
        .map(|text| active_branch(&text))
        .unwrap_or_default()
}

/// Entries form a tree through `parentId`; the last one written is the
/// current leaf. Walking back from it skips branches left with `/tree`.
fn active_branch(text: &str) -> Vec<Value> {
    let entries: Vec<Value> = text
        .lines()
        .filter_map(|line| serde_json::from_str::<Value>(line).ok())
        .filter(|entry| entry.get("id").is_some() && entry["type"] != "session")
        .collect();
    let by_id: std::collections::HashMap<&str, usize> = entries
        .iter()
        .enumerate()
        .filter_map(|(ix, entry)| Some((entry["id"].as_str()?, ix)))
        .collect();
    let mut branch = Vec::new();
    let mut next = entries.len().checked_sub(1);
    while let Some(ix) = next {
        // A cycle would be a broken file; stop rather than loop.
        if branch.len() > entries.len() {
            break;
        }
        branch.push(ix);
        next = entries[ix]["parentId"]
            .as_str()
            .and_then(|parent| by_id.get(parent).copied());
    }
    branch.reverse();
    branch.into_iter().map(|ix| entries[ix].clone()).collect()
}

#[cfg(test)]
mod tests {
    use super::{active_branch, read_index, write_index};
    use std::collections::BTreeMap;
    use std::path::PathBuf;

    #[test]
    fn follows_the_current_branch() {
        let text = [
            r#"{"type":"session","version":3,"id":"s","cwd":"/w"}"#,
            r#"{"type":"message","id":"a","parentId":null,"message":{"role":"user","content":"one"}}"#,
            r#"{"type":"message","id":"b","parentId":"a","message":{"role":"user","content":"abandoned"}}"#,
            r#"{"type":"message","id":"c","parentId":"a","message":{"role":"user","content":"two"}}"#,
            "not json",
        ]
        .join("\n");
        let ids: Vec<String> = active_branch(&text)
            .iter()
            .map(|e| e["id"].as_str().unwrap().to_string())
            .collect();
        assert_eq!(ids, ["a", "c"]);
    }

    #[test]
    fn index_round_trips() {
        let dir = std::env::temp_dir().join(format!("treehouse-history-{}", std::process::id()));
        let path = dir.join("sessions.json");
        let mut index = BTreeMap::new();
        index.insert(PathBuf::from("/w/main"), PathBuf::from("/s/1.jsonl"));
        write_index(&path, &index);
        assert_eq!(read_index(&path), index);
        let _ = std::fs::remove_dir_all(dir);
    }
}
