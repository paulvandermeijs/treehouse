//! Which Pi session file holds each worktree's chat, so the chat comes back
//! after a restart.
//!
//! Pi saves every session as a JSONL file but has no notion of worktrees,
//! and sub-agents share their worktree's folder, so Treehouse keeps its own
//! index: `sessions.json` in its data folder, worktree path → session file.

use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::config::Config;

/// A worktree's chat and the sub-agents its current session started.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Saved {
    pub file: PathBuf,
    #[serde(default)]
    pub subagents: Vec<SavedSubAgent>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SavedSubAgent {
    /// The parent's `subagent` tool call, which its card in the chat names.
    pub tool_call_id: String,
    pub file: PathBuf,
    pub title: String,
    pub task: String,
}

type Index = BTreeMap<PathBuf, Saved>;

fn index_path() -> PathBuf {
    Config::data_dir().join("sessions.json")
}

fn read_index(path: &Path) -> Index {
    let Ok(json) = std::fs::read_to_string(path) else {
        return Index::default();
    };
    serde_json::from_str(&json)
        .or_else(|_| {
            // The first version mapped each worktree straight to its file.
            serde_json::from_str::<BTreeMap<PathBuf, PathBuf>>(&json).map(|old| {
                old.into_iter()
                    .map(|(worktree, file)| {
                        let saved = Saved {
                            file,
                            subagents: Vec::new(),
                        };
                        (worktree, saved)
                    })
                    .collect()
            })
        })
        .unwrap_or_default()
}

fn write_index(path: &Path, index: &Index) {
    if let Some(dir) = path.parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    if let Ok(json) = serde_json::to_string_pretty(index) {
        let _ = std::fs::write(path, json);
    }
}

fn update(change: impl FnOnce(&mut Index)) {
    let path = index_path();
    let mut index = read_index(&path);
    let before = index.clone();
    change(&mut index);
    if index != before {
        write_index(&path, &index);
    }
}

/// The worktree's saved chat, keeping only files that still exist. Pi
/// writes a session file once it has a message, so a sub-agent stopped
/// before then has none.
pub fn saved(worktree: &Path) -> Option<Saved> {
    let mut saved = read_index(&index_path()).remove(worktree)?;
    saved.subagents.retain(|s| s.file.exists());
    saved.file.exists().then_some(saved)
}

/// Records the worktree's session file. A different file means a new
/// session, whose sub-agents start over.
pub fn remember(worktree: &Path, file: &Path) {
    update(|index| {
        let saved = index.entry(worktree.to_path_buf()).or_default();
        if saved.file != file {
            *saved = Saved {
                file: file.to_path_buf(),
                subagents: Vec::new(),
            };
        }
    });
}

pub fn remember_subagent(worktree: &Path, subagent: SavedSubAgent) {
    update(|index| {
        let Some(saved) = index.get_mut(worktree) else {
            return;
        };
        saved
            .subagents
            .retain(|s| s.tool_call_id != subagent.tool_call_id);
        saved.subagents.push(subagent);
    });
}

/// Drops a deleted worktree. Pi's session files stay, like Pi's own.
pub fn forget(worktree: &Path) {
    update(|index| {
        index.remove(worktree);
    });
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
    use super::{Saved, SavedSubAgent, active_branch, read_index, write_index};
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
        index.insert(
            PathBuf::from("/w/main"),
            Saved {
                file: PathBuf::from("/s/1.jsonl"),
                subagents: vec![SavedSubAgent {
                    tool_call_id: "c1".into(),
                    file: PathBuf::from("/s/2.jsonl"),
                    title: "Explore".into(),
                    task: "Map the components".into(),
                }],
            },
        );
        write_index(&path, &index);
        assert_eq!(read_index(&path), index);
        std::fs::write(&path, r#"{"/w/main": "/s/1.jsonl"}"#).unwrap();
        assert_eq!(
            read_index(&path)[&PathBuf::from("/w/main")].file,
            PathBuf::from("/s/1.jsonl")
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
