//! Preparing a new worktree with the project's own `.agents/linked` and
//! `.agents/prepare`, the files Delta and other tools use.
//!
//! `.agents/linked` lists git-ignored paths, such as `.env`, that every
//! worktree shares with the main one through a symlink. `.agents/prepare` is
//! an executable script run from the new worktree's root. Both are the
//! project's own code, so they only run once the project is trusted.

use std::path::{Path, PathBuf};
use std::process::Command;

const LINKED: &str = ".agents/linked";
const PREPARE: &str = ".agents/prepare";
/// The main worktree's path, as Cursor names it for its worktree setup;
/// Delta's docs define no variable of their own.
pub const ROOT_ENV: &str = "ROOT_WORKTREE_PATH";

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub enum LinkOutcome {
    /// Now a symlink to the main worktree's copy.
    Linked,
    /// Only this worktree had it: moved to the main worktree, then linked.
    Moved,
    /// Neither worktree has it, so there's nothing to link yet.
    Missing,
    /// Linking would put a tracked file behind a symlink.
    NotIgnored,
    Failed(String),
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Link {
    pub path: String,
    pub outcome: LinkOutcome,
}

/// Whether the worktree has anything to set up.
pub fn has_setup(worktree: &Path) -> bool {
    worktree.join(LINKED).is_file() || worktree.join(PREPARE).is_file()
}

/// What the row in the chat names: the script, or the list when there's
/// only a list.
pub fn title(worktree: &Path) -> &'static str {
    if worktree.join(PREPARE).is_file() {
        PREPARE
    } else {
        LINKED
    }
}

/// The paths in `.agents/linked`: one per line, `#` comments, `/` separators.
pub fn linked_paths(text: &str) -> Vec<String> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(|line| line.trim_matches('/').to_string())
        .filter(|line| !line.is_empty() && !line.split('/').any(|part| part == ".."))
        .collect()
}

/// Links every path `.agents/linked` lists to the main worktree's copy.
pub fn link_all(root: &Path, worktree: &Path) -> Vec<Link> {
    let Ok(text) = std::fs::read_to_string(worktree.join(LINKED)) else {
        return Vec::new();
    };
    linked_paths(&text)
        .into_iter()
        .map(|path| Link {
            outcome: link(root, worktree, &path),
            path,
        })
        .collect()
}

fn link(root: &Path, worktree: &Path, path: &str) -> LinkOutcome {
    let ignored = Command::new("git")
        .args(["check-ignore", "-q", "--no-index", "--", path])
        .current_dir(worktree)
        .status()
        .is_ok_and(|status| status.success());
    if !ignored {
        return LinkOutcome::NotIgnored;
    }
    let shared = root.join(path);
    let here = worktree.join(path);
    let already = std::fs::read_link(&here).is_ok_and(|target| target == shared);
    if already {
        return LinkOutcome::Linked;
    }
    let result = if shared.exists() {
        remove(&here).and_then(|_| symlink(&shared, &here).map(|_| LinkOutcome::Linked))
    } else if here.exists() {
        // Nothing is merged: the new worktree's copy becomes the shared one.
        shared
            .parent()
            .map(std::fs::create_dir_all)
            .transpose()
            .and_then(|_| std::fs::rename(&here, &shared))
            .and_then(|_| symlink(&shared, &here).map(|_| LinkOutcome::Moved))
    } else {
        Ok(LinkOutcome::Missing)
    };
    result.unwrap_or_else(|err| LinkOutcome::Failed(err.to_string()))
}

fn remove(path: &Path) -> std::io::Result<()> {
    match std::fs::symlink_metadata(path) {
        Err(_) => Ok(()),
        Ok(meta) if meta.is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
    }
}

fn symlink(target: &Path, link: &Path) -> std::io::Result<()> {
    if let Some(parent) = link.parent() {
        std::fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    return std::os::unix::fs::symlink(target, link);
    #[cfg(windows)]
    return if target.is_dir() {
        std::os::windows::fs::symlink_dir(target, link)
    } else {
        std::os::windows::fs::symlink_file(target, link)
    };
}

/// The command that runs `.agents/prepare`, or why it can't run.
pub fn prepare_command(root: &Path, worktree: &Path) -> Option<Result<Command, String>> {
    let script = worktree.join(PREPARE);
    if !script.is_file() {
        return None;
    }
    if !is_executable(&script) {
        return Some(Err(format!(
            "Not executable: run chmod +x {PREPARE} and commit it"
        )));
    }
    // Windows has no shebangs; the script is POSIX shell, run by Git Bash.
    let mut command = if cfg!(windows) {
        let mut bash = Command::new("bash");
        bash.arg(PREPARE);
        bash
    } else {
        Command::new(PathBuf::from(".").join(PREPARE))
    };
    command
        .current_dir(worktree)
        .env(ROOT_ENV, root)
        .stdin(std::process::Stdio::null());
    // Its own process group, so stopping it also stops what it started;
    // a leftover child would hold the output open and keep the run going.
    #[cfg(unix)]
    std::os::unix::process::CommandExt::process_group(&mut command, 0);
    Some(Ok(command))
}

/// Stops the script and everything it started.
pub fn stop(child: &mut std::process::Child) {
    #[cfg(unix)]
    if let Ok(group) = libc::pid_t::try_from(child.id()) {
        // SAFETY: killpg only sends a signal; the group is the script's own.
        if unsafe { libc::killpg(group, libc::SIGTERM) } == 0 {
            return;
        }
    }
    #[cfg(windows)]
    {
        let tree = Command::new("taskkill")
            .args(["/T", "/F", "/PID", &child.id().to_string()])
            .status();
        if tree.is_ok_and(|status| status.success()) {
            return;
        }
    }
    let _ = child.kill();
}

#[cfg(unix)]
fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn is_executable(_: &Path) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::{LinkOutcome, link_all, linked_paths};
    use std::process::Command;

    #[test]
    fn reads_the_linked_list() {
        let text = "# Shared local configuration.\n.env\n\n /config/local.json/ \n../outside\n";
        assert_eq!(linked_paths(text), [".env", "config/local.json"]);
    }

    #[test]
    fn links_ignored_paths_to_the_main_worktree() {
        let base = std::env::temp_dir().join(format!("treehouse-setup-{}", std::process::id()));
        let (root, wt) = (base.join("main"), base.join("wt"));
        for dir in [&root, &wt] {
            std::fs::create_dir_all(dir.join(".agents")).unwrap();
            Command::new("git")
                .args(["init", "-q"])
                .current_dir(dir)
                .status()
                .unwrap();
            std::fs::write(dir.join(".gitignore"), ".env\nonly-here\n").unwrap();
        }
        std::fs::write(
            wt.join(".agents/linked"),
            ".env\nonly-here\ntracked.txt\nnowhere\n",
        )
        .unwrap();
        std::fs::write(root.join(".env"), "SECRET=1").unwrap();
        std::fs::write(wt.join(".env"), "stale").unwrap();
        std::fs::write(wt.join("only-here"), "mine").unwrap();
        std::fs::write(wt.join("tracked.txt"), "x").unwrap();

        let links = link_all(&root, &wt);
        let outcomes: Vec<_> = links.iter().map(|l| l.outcome.clone()).collect();
        assert_eq!(
            outcomes,
            [
                LinkOutcome::Linked,
                LinkOutcome::Moved,
                LinkOutcome::NotIgnored,
                LinkOutcome::NotIgnored
            ]
        );
        assert_eq!(
            std::fs::read_to_string(wt.join(".env")).unwrap(),
            "SECRET=1"
        );
        assert_eq!(
            std::fs::read_to_string(root.join("only-here")).unwrap(),
            "mine"
        );
        assert!(std::fs::read_link(wt.join("only-here")).is_ok());
        let _ = std::fs::remove_dir_all(base);
    }
}
