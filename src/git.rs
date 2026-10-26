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

/// What removing a worktree, and perhaps its branch, would throw away.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct UnsavedWork {
    /// Changed and untracked files, as `git status` lists them.
    pub files: Vec<String>,
    /// Commits on the branch that no other branch or remote has.
    pub commits: usize,
}

impl UnsavedWork {
    pub fn is_empty(&self) -> bool {
        self.files.is_empty() && self.commits == 0
    }
}

pub fn unsaved_work(worktree: &Path, branch: &str) -> UnsavedWork {
    let files = git(worktree, &["status", "--porcelain=v1", "-z"])
        .map(|out| {
            let mut entries = out.split('\0');
            let mut files = Vec::new();
            while let Some(entry) = entries.next() {
                if entry.len() > 3 {
                    files.push(entry[3..].to_string());
                    // A rename's source path follows as its own entry.
                    if entry.starts_with('R') || entry.starts_with('C') {
                        entries.next();
                    }
                }
            }
            files
        })
        .unwrap_or_default();
    let commits = only_on_branch(worktree, "HEAD", branch).unwrap_or(0);
    UnsavedWork { files, commits }
}

/// How many commits reachable from `rev` no branch but `branch`, and no
/// remote, has: the ones deleting `branch` would lose.
fn only_on_branch(repo: &Path, rev: &str, branch: &str) -> Option<usize> {
    // `--exclude` matches the names `--branches` lists, without `refs/heads/`.
    let exclude = format!("--exclude={branch}");
    git(
        repo,
        &[
            "rev-list",
            "--count",
            rev,
            "--not",
            &exclude,
            "--branches",
            "--remotes",
        ],
    )
    .ok()
    .and_then(|out| out.trim().parse().ok())
}

/// Removes a worktree, and its branch when asked. Without `force`, git keeps
/// a worktree with changed or untracked files, and `delete_branch` only
/// deletes a branch whose commits are all on another branch or a remote.
/// A branch that is kept is reported in the `Ok`, since the worktree is gone
/// by then.
pub fn remove_worktree(
    project: &Path,
    path: &Path,
    delete_branch: Option<&str>,
    force: bool,
    merged_into: Option<&str>,
) -> Result<Option<String>> {
    let path = path.display().to_string();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&path);
    git(project, &args)?;
    if let Some(branch) = delete_branch {
        // `branch -d` only checks the current branch and the upstream, which
        // misses work merged into some other branch; check that ourselves.
        // A squash-merged branch keeps commits of its own, but its changes
        // are all in the base.
        let merged = force
            || only_on_branch(project, branch, branch) == Some(0)
            || merged_into.is_some_and(|base| is_merged(project, branch, base));
        if git(
            project,
            &["branch", if merged { "-D" } else { "-d" }, branch],
        )
        .is_err()
        {
            return Ok(Some(format!(
                "Kept the branch {branch}: it has commits no other branch has."
            )));
        }
    }
    Ok(None)
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

/// A merge into the base branch that passed its checks.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MergePlan {
    /// Commits on the branch that the base lacks.
    pub commits: usize,
    /// The folder the base branch is checked out in, if any.
    pub base_dir: Option<PathBuf>,
}

/// Checks that `branch`, checked out in `worktree`, can be merged into
/// `base` without forcing anything. A refusal says why, in words Pi and the
/// user can act on.
pub fn check_merge(
    project: &Path,
    worktree: &Path,
    branch: &str,
    base: &str,
) -> std::result::Result<MergePlan, String> {
    let local = format!("refs/heads/{base}");
    if git(worktree, &["rev-parse", "--verify", "--quiet", &local]).is_err() {
        return Err(format!(
            "{base} isn't a local branch, so Treehouse can't merge into it. Push the branch and open a pull request instead."
        ));
    }
    match git(worktree, &["status", "--porcelain"]) {
        Ok(status) if status.trim().is_empty() => {}
        Ok(_) => return Err("The worktree has uncommitted changes. Commit them first.".into()),
        Err(err) => return Err(format!("{err:#}")),
    }
    if git(worktree, &["merge-base", "--is-ancestor", base, "HEAD"]).is_err() {
        return Err(format!(
            "The branch is behind {base}. Sync with {base} first, then try again."
        ));
    }
    let range = format!("{base}..HEAD");
    let commits: usize = git(worktree, &["rev-list", "--count", &range])
        .ok()
        .and_then(|out| out.trim().parse().ok())
        .unwrap_or(0);
    if commits == 0 {
        return Err(format!(
            "Nothing to merge: {base} already has every commit on {branch}."
        ));
    }
    let base_dir = list_worktrees(project)
        .map_err(|err| format!("{err:#}"))?
        .into_iter()
        .find(|w| w.branch == base)
        .map(|w| w.path);
    if let Some(dir) = &base_dir {
        // Untracked files don't block a fast-forward: git refuses to
        // overwrite one rather than losing it.
        let status = git(dir, &["status", "--porcelain", "--untracked-files=no"])
            .map_err(|err| format!("{err:#}"))?;
        if !status.trim().is_empty() {
            return Err(format!(
                "{base} has uncommitted changes in {}. Commit or stash them there first.",
                dir.display()
            ));
        }
    }
    Ok(MergePlan { commits, base_dir })
}

/// Merges the worktree's branch into `base` after `check_merge`: a
/// fast-forward, or one new commit on `base` with the branch's changes when
/// `squash` gives its message. Returns the base's new short commit.
pub fn merge_into(
    worktree: &Path,
    base: &str,
    plan: &MergePlan,
    squash: Option<&str>,
) -> Result<String> {
    let head = git(worktree, &["rev-parse", "HEAD"])?.trim().to_string();
    let target = match squash {
        None => head,
        // The branch contains the base, so its tree on top of the base is
        // exactly the branch's changes.
        Some(message) => {
            let tree = format!("{head}^{{tree}}");
            git(worktree, &["commit-tree", &tree, "-p", base, "-m", message])?
                .trim()
                .to_string()
        }
    };
    match &plan.base_dir {
        Some(dir) => git(dir, &["merge", "--ff-only", "--quiet", &target]).map(|_| ())?,
        None => {
            let local = format!("refs/heads/{base}");
            let old = git(worktree, &["rev-parse", &local])?.trim().to_string();
            git(worktree, &["update-ref", &local, &target, &old]).map(|_| ())?
        }
    }
    Ok(target.chars().take(7).collect())
}

/// Whether every change on `branch` is already in `base`: after a
/// fast-forward, a merge, or a squash merge, which git alone doesn't see.
pub fn is_merged(repo: &Path, branch: &str, base: &str) -> bool {
    if git(repo, &["merge-base", "--is-ancestor", branch, base]).is_ok() {
        return true;
    }
    // Merging a squash-merged branch again changes nothing.
    let Ok(merged) = git(repo, &["merge-tree", "--write-tree", base, branch]) else {
        return false;
    };
    let base_tree = format!("{base}^{{tree}}");
    git(repo, &["rev-parse", &base_tree])
        .is_ok_and(|tree| merged.lines().next() == Some(tree.trim()))
}

/// Whether `branch` has work that is now in `base`: merged, rather than
/// level with it because nothing was ever committed on it.
pub fn landed(repo: &Path, branch: &str, base: &str) -> bool {
    if !is_merged(repo, branch, base) {
        return false;
    }
    // The oldest reflog entry is where the branch started.
    let local = format!("refs/heads/{branch}");
    let created = git(repo, &["reflog", "show", "--format=%H", &local])
        .ok()
        .and_then(|log| log.lines().last().map(str::to_string));
    let tip = git(repo, &["rev-parse", &local]).ok();
    matches!((created, tip), (Some(created), Some(tip)) if created != tip.trim())
}

/// One commit, as the chat's commit card shows it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub subject: String,
    pub body: String,
    pub files: usize,
}

pub fn head(repo: &Path) -> Option<String> {
    git(repo, &["rev-parse", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string())
}

/// The commits on the current branch since `since`, oldest first. A merge
/// of the base counts as one commit, without the base's own.
pub fn commits_since(repo: &Path, since: &str) -> Vec<Commit> {
    let range = format!("{since}..HEAD");
    let Ok(log) = git(
        repo,
        &[
            "log",
            "--first-parent",
            "--reverse",
            "--format=%h%x1f%s%x1f%b%x1e",
            &range,
        ],
    ) else {
        return Vec::new();
    };
    log.split('\x1e')
        .filter_map(|entry| {
            let mut fields = entry.trim_start_matches('\n').splitn(3, '\x1f');
            let hash = fields.next()?.to_string();
            if hash.is_empty() {
                return None;
            }
            let parent = format!("{hash}^1");
            let files = git(repo, &["diff", "--name-only", &parent, &hash])
                .map(|out| out.lines().count())
                .unwrap_or(0);
            Some(Commit {
                hash,
                subject: fields.next().unwrap_or_default().to_string(),
                body: fields.next().unwrap_or_default().trim().to_string(),
                files,
            })
        })
        .collect()
}

/// How many commits `HEAD` has that `base` lacks, and the other way round.
pub fn ahead_behind(repo: &Path, base: &str) -> Option<(usize, usize)> {
    let range = format!("{base}...HEAD");
    let out = git(repo, &["rev-list", "--left-right", "--count", &range]).ok()?;
    let mut counts = out.split_whitespace().map(|n| n.parse().ok());
    let behind = counts.next()??;
    let ahead = counts.next()??;
    Some((ahead, behind))
}

/// Changed and untracked files.
pub fn uncommitted(repo: &Path) -> usize {
    git(repo, &["status", "--porcelain"])
        .map(|out| out.lines().count())
        .unwrap_or(0)
}

/// Splits a Conventional Commit subject into its `type(scope): ` prefix and
/// the rest.
pub fn conventional(subject: &str) -> (Option<&str>, &str) {
    let Some(end) = subject.find(": ") else {
        return (None, subject);
    };
    let kind = &subject[..end];
    let name = kind.split(['(', '!']).next().unwrap_or("");
    let ok =
        !name.is_empty() && name.chars().all(|c| c.is_ascii_lowercase()) && !kind.contains(' ');
    if ok {
        (Some(&subject[..end + 2]), &subject[end + 2..])
    } else {
        (None, subject)
    }
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

    #[test]
    fn keeps_unsaved_work_unless_forced() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repository(repo).unwrap();
        let run = |cwd: &Path, args: &[&str]| git(cwd, args).unwrap();
        run(repo, &["config", "user.email", "t@example.com"]);
        run(repo, &["config", "user.name", "Test"]);
        run(repo, &["checkout", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        run(repo, &["add", "."]);
        run(repo, &["commit", "-m", "init"]);

        let location = repo.join(".worktrees/feat");
        add_worktree(repo, "feat", &location).unwrap();
        assert!(unsaved_work(&location, "feat").is_empty());

        // An untracked file and a commit only this branch has.
        std::fs::write(location.join("b.txt"), "new\n").unwrap();
        let unsaved = unsaved_work(&location, "feat");
        assert_eq!(unsaved.files, ["b.txt"]);
        run(&location, &["add", "."]);
        run(&location, &["commit", "-m", "work"]);
        std::fs::write(location.join("a.txt"), "changed\n").unwrap();
        let unsaved = unsaved_work(&location, "feat");
        assert_eq!((unsaved.files.len(), unsaved.commits), (1, 1));

        // Not forced: git keeps the worktree and its changes.
        assert!(remove_worktree(repo, &location, Some("feat"), false, None).is_err());
        assert!(location.join("a.txt").exists());

        // Committed but unmerged: the worktree goes, the branch stays.
        run(&location, &["checkout", "a.txt"]);
        let kept = remove_worktree(repo, &location, Some("feat"), false, None).unwrap();
        assert!(kept.is_some_and(|note| note.contains("feat")));
        assert!(!location.exists());
        assert!(git(repo, &["rev-parse", "--verify", "feat"]).is_ok());

        // Merged: the branch is deleted too.
        run(repo, &["merge", "--ff-only", "feat"]);
        let again = repo.join(".worktrees/feat2");
        add_worktree(repo, "feat2", &again).unwrap();
        assert_eq!(
            remove_worktree(repo, &again, Some("feat2"), false, None).unwrap(),
            None
        );
        assert!(git(repo, &["rev-parse", "--verify", "feat2"]).is_err());
    }

    #[test]
    fn merges_into_the_base_only_when_safe() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repository(repo).unwrap();
        let run = |cwd: &Path, args: &[&str]| git(cwd, args).unwrap();
        run(repo, &["config", "user.email", "t@example.com"]);
        run(repo, &["config", "user.name", "Test"]);
        run(repo, &["checkout", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        run(repo, &["add", "."]);
        run(repo, &["commit", "-m", "init"]);

        let feat = repo.join(".worktrees/feat");
        add_worktree(repo, "feat", &feat).unwrap();
        let check = || check_merge(repo, &feat, "feat", "main");
        assert!(check().unwrap_err().starts_with("Nothing to merge"));
        // Level with main, but nothing landed.
        assert!(is_merged(repo, "feat", "main") && !landed(repo, "feat", "main"));

        std::fs::write(feat.join("b.txt"), "new\n").unwrap();
        assert!(check().unwrap_err().contains("uncommitted"));
        run(&feat, &["add", "."]);
        run(&feat, &["commit", "-m", "feat: add b"]);

        // main moves on: the branch has to sync first.
        std::fs::write(repo.join("c.txt"), "main\n").unwrap();
        run(repo, &["add", "c.txt"]);
        run(repo, &["commit", "-m", "main work"]);
        assert!(check().unwrap_err().contains("behind main"));
        run(&feat, &["merge", "main", "--no-edit"]);

        // Changes in main's own folder block it; untracked files don't.
        std::fs::write(repo.join("a.txt"), "dirty\n").unwrap();
        assert!(
            check()
                .unwrap_err()
                .contains("main has uncommitted changes")
        );
        run(repo, &["checkout", "a.txt"]);
        std::fs::write(repo.join("scratch.txt"), "mine\n").unwrap();
        let plan = check().unwrap();
        assert_eq!(plan.commits, 2);
        assert_eq!(plan.base_dir.as_deref(), Some(repo));

        let sha = merge_into(&feat, "main", &plan, None).unwrap();
        assert_eq!(run(repo, &["rev-parse", "--short=7", "HEAD"]).trim(), sha);
        assert!(repo.join("b.txt").exists());
        assert!(is_merged(repo, "feat", "main"));
    }

    #[test]
    fn squashes_into_one_commit() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repository(repo).unwrap();
        let run = |cwd: &Path, args: &[&str]| git(cwd, args).unwrap();
        run(repo, &["config", "user.email", "t@example.com"]);
        run(repo, &["config", "user.name", "Test"]);
        run(repo, &["checkout", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        run(repo, &["add", "."]);
        run(repo, &["commit", "-m", "init"]);
        // main isn't checked out anywhere: the merge moves the ref.
        run(repo, &["checkout", "--detach"]);

        let feat = repo.join(".worktrees/feat");
        add_worktree(repo, "feat", &feat).unwrap();
        for name in ["b", "c"] {
            std::fs::write(feat.join(name), "x\n").unwrap();
            run(&feat, &["add", "."]);
            run(&feat, &["commit", "-m", name]);
        }
        assert!(!is_merged(repo, "feat", "main"));
        assert!(!landed(repo, "feat", "main"));
        let plan = check_merge(repo, &feat, "feat", "main").unwrap();
        assert_eq!(plan.base_dir, None);
        merge_into(&feat, "main", &plan, Some("feat: add b and c")).unwrap();
        assert_eq!(run(repo, &["rev-list", "--count", "main"]).trim(), "2");
        assert_eq!(
            run(repo, &["log", "-1", "--format=%s", "main"]).trim(),
            "feat: add b and c"
        );
        assert!(landed(repo, "feat", "main"));
        // Clean up deletes the squash-merged branch, though git sees it unmerged.
        assert_eq!(
            remove_worktree(repo, &feat, Some("feat"), false, Some("main")).unwrap(),
            None
        );
        assert!(git(repo, &["rev-parse", "--verify", "feat"]).is_err());
    }

    #[test]
    fn splits_conventional_subjects() {
        assert_eq!(
            conventional("feat(diff): show the dock"),
            (Some("feat(diff): "), "show the dock")
        );
        assert_eq!(conventional("fix!: drop it"), (Some("fix!: "), "drop it"));
        assert_eq!(
            conventional("Update README: typo"),
            (None, "Update README: typo")
        );
        assert_eq!(conventional("wip"), (None, "wip"));
    }

    #[test]
    fn lists_the_commits_a_turn_made() {
        let dir = tempfile::tempdir().unwrap();
        let repo = dir.path();
        init_repository(repo).unwrap();
        let run = |cwd: &Path, args: &[&str]| git(cwd, args).unwrap();
        run(repo, &["config", "user.email", "t@example.com"]);
        run(repo, &["config", "user.name", "Test"]);
        run(repo, &["checkout", "-b", "main"]);
        std::fs::write(repo.join("a.txt"), "one\n").unwrap();
        run(repo, &["add", "."]);
        run(repo, &["commit", "-m", "init"]);
        let feat = repo.join(".worktrees/feat");
        add_worktree(repo, "feat", &feat).unwrap();
        let before = head(&feat).unwrap();

        std::fs::write(feat.join("b.txt"), "b\n").unwrap();
        std::fs::write(feat.join("c.txt"), "c\n").unwrap();
        run(&feat, &["add", "."]);
        run(
            &feat,
            &["commit", "-m", "feat: add b and c", "-m", "Both at once."],
        );
        std::fs::write(repo.join("m.txt"), "m\n").unwrap();
        run(repo, &["add", "."]);
        run(repo, &["commit", "-m", "main: more"]);
        run(&feat, &["merge", "main", "--no-edit"]);
        std::fs::write(feat.join("d.txt"), "d\n").unwrap();

        let commits = commits_since(&feat, &before);
        assert_eq!(commits.len(), 2, "{commits:?}");
        assert_eq!(commits[0].subject, "feat: add b and c");
        assert_eq!(commits[0].body, "Both at once.");
        assert_eq!(commits[0].files, 2);
        assert!(commits[1].subject.starts_with("Merge branch"));
        assert_eq!(ahead_behind(&feat, "main"), Some((2, 0)));
        assert_eq!(uncommitted(&feat), 1);
    }
}
