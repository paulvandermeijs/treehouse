# Committing and merging

Pi takes a worktree from first change to merged branch: it commits, brings in the latest base branch, and asks Treehouse to merge. Treehouse does the steps that reach outside the worktree, checks them first, and always asks you before it changes the base branch. For anything else there's still `!git …`.

## The commands

Three commands at the top of the slash menu. Anything you type after one goes to Pi as extra instructions, such as `/commit only the parser`.

| Command | What Pi does |
| --- | --- |
| `/commit` | Commits the work so far as one or more logical [Conventional Commits](https://www.conventionalcommits.org/), leaving out scratch files and saying so |
| `/sync` | Rebases the worktree's branch onto the latest base branch, resolves conflicts and runs the tests |
| `/finish-branch` | Commits anything left, syncs, runs the tests, then asks Treehouse to merge the branch into the base |

They're ordinary turns: the chat shows Pi's git commands, your approval settings apply, and you can steer or stop them. Pi never pushes unless you ask it to.

The base branch is the one the worktree was created from (or `origin/main`, depending on **Compare against** in **Settings › Worktrees**). The main worktree is on the base branch, so it only has `/commit`.

You can start the same commands without typing:

- Right-click a worktree for **Commit with Pi**, **Sync with main** and **Finish branch**. Each shows what it'll do, or why it can't: nothing to commit, up to date, merged, or Pi is busy.
- The diff dock has **Commit with Pi** and **Finish branch** icon buttons, next to Refresh.
- The command palette has all three for the chat on screen.

### Your own skills

Each command runs a skill that ships with Treehouse: `treehouse-commit`, `treehouse-sync` and `treehouse-finish-branch`. If Pi finds a skill called `commit`, `sync` or `finish-branch` in the project or your own skills, the command runs that one instead, and the slash menu marks it **Skill** rather than **Command**. Use this to follow a team's own commit style.

## The commit card

Right after the command that made commits, the chat shows a card listing them, read from git rather than from Pi's reply, and saved with the chat so it's still there after a restart: each commit's type, subject, body, short hash and number of files, and how far the branch is now ahead of the base, with anything left uncommitted.

## Merging

`/finish-branch` ends with Pi calling Treehouse's merge tool. The base branch is usually checked out in the main worktree, often with your own work in it, so Treehouse does the merge, not Pi. First it checks that:

- the worktree has no uncommitted changes,
- the branch contains the latest base (it's synced, so the merge is a fast-forward),
- the base is a local branch and its folder has no uncommitted changes,
- no agent is working in the base's worktree.

If any check fails, the merge is refused with the reason, and Pi can fix what it can (sync again, say) and try once more.

When the checks pass, the chat asks you, whatever your [approval setting](./approvals):

- **Merge** (<kbd>Enter</kbd>) fast-forwards the base to the branch.
- **Squash into one** adds a single commit to the base with all of the branch's changes, using a message Pi wrote. Hover the button to read it.
- **Deny** leaves everything as it was.

Treehouse never forces anything, and doesn't push. The card that follows says where the base now is, and offers to clean up.

## Cleaning up

**Clean up worktree** on the merged card, or **Clean up…** in the context menu of a merged worktree, removes the worktree's folder and its branch, and its chat goes with it. Squash-merged branches count as merged too: Treehouse checks that every change is in the base. **Keep it** leaves the worktree where it is.

A merged worktree that gets new changes or commits counts as unmerged again: it loses its check mark, and **Delete worktree…**, in red, takes the place of Clean up. For abandoned work, **Delete worktree…** still warns about anything that would be lost.

## When a sync stops halfway

`/sync` rebases rather than merging, so the branch's history stays linear and the final merge is a fast-forward. If the branch has already been pushed, Pi asks before rewriting it.

When Pi can't resolve a conflict on its own, it stops and asks you how. Until the rebase is finished or undone, the chat shows **Sync stopped halfway** with the number of conflicts left. Answer Pi to finish it, or click **Abort sync** to put the branch back as it was before the sync (`git rebase --abort`).

## Where each branch stands

Worktree rows in the sidebar show small markers before the status dot:

| Marker | Means |
| --- | --- |
| Orange dot | Uncommitted changes |
| `↑3` | 3 commits the base doesn't have yet |
| `↓5` | 5 commits on the base that this branch lacks |
| ✓ | Merged into the base; the name dims |

Hover them for the full story, such as "1 ahead, 3 behind main · 1 uncommitted file". A branch level with its base shows nothing, and the main worktree only shows changes. A worktree's markers are refreshed as Pi works in it and when its turn ends; the others follow when their base branch moves, and every worktree after a merge.

## Read-only projects

In a project you haven't [trusted](./trust), Pi can only read, so the commands and menu items show disabled with **Read only**.
