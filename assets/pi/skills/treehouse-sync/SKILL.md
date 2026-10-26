---
name: treehouse-sync
description: Merge the latest base branch into this worktree's branch and resolve any conflicts. Run from Treehouse's /sync command.
disable-model-invocation: true
---

# Sync with the base branch

The base branch is named in the system prompt ("This worktree's base branch is …").

1. If there are uncommitted changes, commit them first by following the treehouse-commit steps, or ask the user what to do with them. Don't stash.
2. Run `git merge <base>`. If it reports "Already up to date", say so and stop.
3. Resolve each conflict so both sides keep their intent. When they truly clash and you can't tell which should win, stop and ask the user, describing both sides.
4. Check no conflict markers are left (`git diff --check`), run the project's tests as its AGENTS.md or README describes, then `git commit --no-edit`.
5. If you get stuck, leave the merge in progress and explain what's left. Treehouse offers to abort it.

Never rebase, reset or push unless the user asks.

If the user added instructions after the command, they come first.
