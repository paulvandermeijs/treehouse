//! The git operations Treehouse needs, run through the `git` command line.

use anyhow::{Context as _, Result, bail};
use std::path::{Path, PathBuf};
use std::process::Command;

fn git(cwd: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(cwd)
        .output()
        .context("failed to run git")?;
    if !output.status.success() {
        bail!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WorktreeInfo {
    pub path: PathBuf,
    /// The branch name, or a short commit for a detached worktree.
    pub branch: String,
}

pub fn is_repository(path: &Path) -> bool {
    git(path, &["rev-parse", "--git-dir"]).is_ok()
}

pub fn init_repository(path: &Path) -> Result<()> {
    git(path, &["init"]).map(|_| ())
}

/// The project's worktrees, the main one first.
pub fn list_worktrees(project: &Path) -> Result<Vec<WorktreeInfo>> {
    Ok(parse_worktree_list(&git(
        project,
        &["worktree", "list", "--porcelain"],
    )?))
}

pub fn parse_worktree_list(porcelain: &str) -> Vec<WorktreeInfo> {
    let mut worktrees = Vec::new();
    for block in porcelain.split("\n\n") {
        let mut path = None;
        let mut branch = None;
        let mut head = None;
        let mut bare = false;
        for line in block.lines() {
            if let Some(p) = line.strip_prefix("worktree ") {
                path = Some(PathBuf::from(p));
            } else if let Some(b) = line.strip_prefix("branch ") {
                branch = Some(b.trim_start_matches("refs/heads/").to_string());
            } else if let Some(h) = line.strip_prefix("HEAD ") {
                head = Some(h.chars().take(7).collect::<String>());
            } else if line == "bare" {
                bare = true;
            }
        }
        if let (Some(path), false) = (path, bare) {
            let branch = branch.or(head).unwrap_or_else(|| "(unknown)".to_string());
            worktrees.push(WorktreeInfo { path, branch });
        }
    }
    worktrees
}

/// Fills in `{project}` and `{branch}` in the worktree location setting.
pub fn worktree_location(template: &str, project: &Path, branch: &str) -> PathBuf {
    let branch = branch.replace('/', "-");
    PathBuf::from(
        template
            .replace("{project}", &project.display().to_string())
            .replace("{branch}", &branch),
    )
}

/// Creates a worktree with a new branch off the current HEAD.
pub fn add_worktree(project: &Path, branch: &str, location: &Path) -> Result<()> {
    exclude_from_project(project, location);
    git(
        project,
        &[
            "worktree",
            "add",
            "-b",
            branch,
            &location.display().to_string(),
        ],
    )
    .map(|_| ())
}

pub fn remove_worktree(project: &Path, path: &Path, delete_branch: Option<&str>) -> Result<()> {
    git(
        project,
        &["worktree", "remove", "--force", &path.display().to_string()],
    )?;
    if let Some(branch) = delete_branch {
        git(project, &["branch", "-D", branch])?;
    }
    Ok(())
}

pub fn rename_branch(worktree: &Path, new_name: &str) -> Result<()> {
    git(worktree, &["branch", "-m", new_name]).map(|_| ())
}

/// The branch new worktrees are compared against: the main worktree's branch.
pub fn default_base(project: &Path) -> String {
    git(project, &["symbolic-ref", "--short", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "main".to_string())
}

/// The unified diff of the worktree, including uncommitted changes and new
/// files, against its merge base with `base`. The index is left alone.
pub fn diff_since(worktree: &Path, base: &str) -> Result<String> {
    let merge_base = git(worktree, &["merge-base", base, "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "HEAD".to_string());
    let mut patch = git(
        worktree,
        &["diff", "--no-color", "--no-ext-diff", &merge_base],
    )?;
    let untracked = git(
        worktree,
        &["ls-files", "--others", "--exclude-standard", "-z"],
    )?;
    // A trailing slash marks a nested repository, such as another worktree.
    for path in untracked
        .split('\0')
        .filter(|p| !p.is_empty() && !p.ends_with('/'))
    {
        let Ok(content) = std::fs::read(worktree.join(path)) else {
            continue;
        };
        if content.contains(&0) {
            continue;
        }
        let content = String::from_utf8_lossy(&content);
        patch.push_str(&unified_patch(path, None, &content));
    }
    Ok(patch)
}

/// Keeps worktrees created inside the project out of its `git status`.
fn exclude_from_project(project: &Path, location: &Path) {
    let Ok(relative) = location.strip_prefix(project) else {
        return;
    };
    let Some(first) = relative.components().next() else {
        return;
    };
    let entry = format!("/{}/", first.as_os_str().to_string_lossy());
    let Ok(git_dir) = git(project, &["rev-parse", "--git-common-dir"]) else {
        return;
    };
    let git_dir = project.join(git_dir.trim());
    let exclude = git_dir.join("info").join("exclude");
    let current = std::fs::read_to_string(&exclude).unwrap_or_default();
    if !current.lines().any(|line| line.trim() == entry) {
        let _ = std::fs::create_dir_all(git_dir.join("info"));
        let separator = if current.is_empty() || current.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        let _ = std::fs::write(&exclude, format!("{current}{separator}{entry}\n"));
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct DiffStat {
    pub files: usize,
    pub additions: usize,
    pub deletions: usize,
}

/// Counts changed files and lines in a unified diff.
pub fn diff_stat(patch: &str) -> DiffStat {
    let mut stat = DiffStat::default();
    for line in patch.lines() {
        if line.starts_with("diff --git ") {
            stat.files += 1;
        } else if line.starts_with('+') && !line.starts_with("+++") {
            stat.additions += 1;
        } else if line.starts_with('-') && !line.starts_with("---") {
            stat.deletions += 1;
        }
    }
    stat
}

pub fn user_name(cwd: &Path) -> Option<String> {
    git(cwd, &["config", "user.name"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// A unified patch between two versions of one file, for showing a `write`
/// in the chat. `old` is `None` for a new file.
pub fn unified_patch(path: &str, old: Option<&str>, new: &str) -> String {
    let diff = similar::TextDiff::from_lines(old.unwrap_or(""), new);
    let (from, to) = match old {
        Some(_) => (format!("a/{path}"), format!("b/{path}")),
        None => ("/dev/null".to_string(), format!("b/{path}")),
    };
    let body = diff
        .unified_diff()
        .context_radius(3)
        .header(&from, &to)
        .to_string();
    format!("diff --git a/{path} b/{path}\n{body}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_porcelain_worktree_list() {
        let porcelain = "worktree /code/app\nHEAD 1234567890abcdef\nbranch refs/heads/main\n\n\
            worktree /code/app/.worktrees/feat-diff\nHEAD abcdef1234567890\nbranch refs/heads/feat/diff-view\n\n\
            worktree /code/app/.worktrees/detached\nHEAD 0011223344556677\ndetached\n";
        let list = parse_worktree_list(porcelain);
        assert_eq!(list.len(), 3);
        assert_eq!(list[0].branch, "main");
        assert_eq!(list[1].branch, "feat/diff-view");
        assert_eq!(
            list[1].path,
            PathBuf::from("/code/app/.worktrees/feat-diff")
        );
        assert_eq!(list[2].branch, "0011223");
    }

    #[test]
    fn fills_the_location_template() {
        let path = worktree_location(
            "{project}/.worktrees/{branch}",
            Path::new("/code/app"),
            "feat/diff",
        );
        assert_eq!(path, PathBuf::from("/code/app/.worktrees/feat-diff"));
    }

    #[test]
    fn counts_a_patch() {
        let patch = unified_patch("src/a.rs", Some("one\ntwo\n"), "one\nthree\nfour\n");
        assert_eq!(
            diff_stat(&patch),
            DiffStat {
                files: 1,
                additions: 2,
                deletions: 1
            }
        );
        let created = unified_patch("src/b.rs", None, "x\ny\n");
        assert!(created.contains("--- /dev/null"));
        assert_eq!(diff_stat(&created).additions, 2);
    }

    #[test]
    fn works_with_a_real_repository() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repository(repo).unwrap();
        let run = |args: &[&str]| git(repo, args).unwrap();
        run(&["config", "user.email", "t@example.com"]);
        run(&["config", "user.name", "Test"]);
        run(&["checkout", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        run(&["add", "."]);
        run(&["commit", "-m", "init"]);

        let location = worktree_location("{project}/.worktrees/{branch}", repo, "feat/x");
        add_worktree(repo, "feat/x", &location).unwrap();
        let worktrees = list_worktrees(repo).unwrap();
        assert_eq!(worktrees.len(), 2);
        assert_eq!(worktrees[1].branch, "feat/x");

        std::fs::write(location.join("a.txt"), "one\ntwo\n").unwrap();
        std::fs::write(location.join("new.txt"), "fresh\n").unwrap();
        let stat = diff_stat(&diff_since(&location, "main").unwrap());
        assert_eq!(
            stat,
            DiffStat {
                files: 2,
                additions: 2,
                deletions: 0
            }
        );
        // The main worktree is clean: the new worktree inside it is excluded.
        assert_eq!(
            diff_stat(&diff_since(repo, "main").unwrap()),
            DiffStat::default()
        );
        let status = git(repo, &["status", "--porcelain"]).unwrap();
        assert!(status.is_empty(), "unexpected status: {status}");
        assert_eq!(default_base(repo), "main");
    }
}
