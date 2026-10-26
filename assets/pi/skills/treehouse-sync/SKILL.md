---
name: treehouse-sync
description: Rebase this worktree's branch onto the latest base branch and resolve any conflicts. Run from Treehouse's /sync command.
disable-model-invocation: true
---

# Sync with the base branch

The base branch is named in the system prompt ("This worktree's base branch is …"). Rebasing keeps the history linear, so the branch can later be merged into the base with a fast-forward.

1. If there are uncommitted changes, commit them first by following the treehouse-commit steps, or ask the user what to do with them. Don't stash.
2. If `git log HEAD..<base>` shows nothing, the branch is up to date: say so and stop.
3. If the branch has been pushed and the remote has its commits (`git status` names an upstream), ask the user before rewriting it.
4. Run `git rebase <base>`.
5. For each conflict, resolve it so both sides keep their intent, `git add` the files and `git rebase --continue`. When they truly clash and you can't tell which should win, stop and ask the user, describing both sides.
6. Once the rebase is done, check no conflict markers are left (`git diff <base> --check`) and run the project's tests as its AGENTS.md or README describes.
7. If you get stuck, leave the rebase in progress and explain what's left. Treehouse offers to abort it.

Never merge the base into the branch, reset or push unless the user asks.

If the user added instructions after the command, they come first.
