---
name: treehouse-finish-branch
description: Commit, sync and test this worktree's branch, then ask Treehouse to merge it into the base branch. Run from Treehouse's /finish-branch command.
disable-model-invocation: true
---

# Finish the branch

The base branch is named in the system prompt ("This worktree's base branch is …").

1. If anything is uncommitted, commit it by following the treehouse-commit steps.
2. If the base branch has commits this branch lacks (`git log HEAD..<base>`), rebase onto it by following the treehouse-sync steps, so the merge can be a fast-forward.
3. Run the project's tests as its AGENTS.md or README describes. If they fail, stop and report; don't merge.
4. Call the `treehouse_merge` tool. Give it a Conventional Commit message summing up the whole branch, used if the user chooses to squash it into one commit.
5. Report what the tool returns in a sentence. If it refuses, say why, and fix what you can (for example, sync again when the branch is behind) before calling it again.

Never merge into the base branch with git yourself, and never push: the base branch is checked out in another folder, often with the user's own work in it.

If the user added instructions after the command, they come first.
