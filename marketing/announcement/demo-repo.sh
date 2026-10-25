#!/usr/bin/env bash
# Builds the "lumen" project the announcement video shows: a main branch and
# two worktrees, one with uncommitted changes that match fake-pi's script so
# the diff dock has something real to show.
set -euo pipefail
root=${1:?usage: demo-repo.sh <folder>}
rm -rf "$root"
mkdir -p "$root/lumen/src"
cd "$root/lumen"
git init -q -b main
git config user.name "Ada Lovelace"
git config user.email "ada@example.com"

cat > Cargo.toml <<'T'
[package]
name = "lumen"
version = "0.3.0"
edition = "2024"

[dependencies]
gpui-kit = "0.8"
T
cat > src/main.rs <<'T'
mod app;
mod worktree;

fn main() {
    app::run();
}
T
cat > src/app.rs <<'T'
use crate::worktree::Worktree;

pub fn run() {
    let worktrees = Worktree::list(".").unwrap_or_default();
    for worktree in &worktrees {
        println!("{} — {}", worktree.branch, worktree.path.display());
    }
}
T
cat > src/worktree.rs <<'T'
use std::path::PathBuf;

/// A git worktree and the branch checked out in it.
pub struct Worktree {
    pub path: PathBuf,
    pub branch: String,
    pub status: Option<String>,
}

impl Worktree {
    pub fn list(repo: &str) -> std::io::Result<Vec<Worktree>> {
        let output = std::process::Command::new("git")
            .args(["-C", repo, "worktree", "list", "--porcelain"])
            .output()?;
        Ok(parse(&String::from_utf8_lossy(&output.stdout)))
    }
}

fn parse(porcelain: &str) -> Vec<Worktree> {
    porcelain
        .split("\n\n")
        .filter_map(|block| {
            let path = block.lines().next()?.strip_prefix("worktree ")?;
            let branch = block
                .lines()
                .find_map(|l| l.strip_prefix("branch refs/heads/"))
                .unwrap_or("detached");
            Some(Worktree { path: path.into(), branch: branch.into(), status: None })
        })
        .collect()
}
T
printf '# Lumen\n\nA small viewer for git worktrees.\n' > README.md
printf '/target\n/.worktrees\n' > .gitignore
git add -A && git commit -qm "Start Lumen"

mkdir -p .worktrees
git worktree add -q -b diff-view .worktrees/diff-view
git worktree add -q -b settings-sync .worktrees/settings-sync

# The diff-view worktree has work in progress.
cd .worktrees/diff-view
python3 - <<'P'
import re, pathlib
p = pathlib.Path("src/worktree.rs")
s = p.read_text()
s = s.replace("use std::path::PathBuf;\n", "use std::path::PathBuf;\n\nuse crate::diff::DiffState;\n")
s = s.replace("    pub status: Option<String>,\n",
              "    /// The branch this worktree compares against.\n    pub base: String,\n    pub diff: DiffState,\n    pub sub_agents: Vec<u32>,\n")
s = s.replace("Some(Worktree { path: path.into(), branch: branch.into(), status: None })",
              "Some(Worktree {\n                path: path.into(),\n                branch: branch.into(),\n                base: \"main\".into(),\n                diff: DiffState::default(),\n                sub_agents: Vec::new(),\n            })")
p.write_text(s)
m = pathlib.Path("src/main.rs")
m.write_text(m.read_text().replace("mod app;\n", "mod app;\nmod diff;\n"))
P
mkdir -p src/diff
cat > src/diff/mod.rs <<'T'
mod pane;

pub use pane::DiffPane;

/// The changes in a worktree since it branched off its base.
#[derive(Default)]
pub struct DiffState {
    pub files: Vec<FileDiff>,
    pub additions: usize,
    pub deletions: usize,
}

pub struct FileDiff {
    pub path: String,
    pub patch: String,
}
T
cat > src/diff/pane.rs <<'T'
use gpui_kit::*;

use super::DiffState;

/// Shows a worktree's diff beside its chat.
pub struct DiffPane {
    state: Entity<DiffState>,
    split: bool,
}

impl DiffPane {
    pub fn new(state: Entity<DiffState>) -> Self {
        Self { state, split: false }
    }

    pub fn toggle_split(&mut self, cx: &mut Context<Self>) {
        self.split = !self.split;
        cx.notify();
    }
}
T
echo "$root/lumen"
