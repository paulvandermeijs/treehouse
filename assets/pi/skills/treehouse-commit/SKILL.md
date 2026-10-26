---
name: treehouse-commit
description: Commit the work in this worktree as one or more logical conventional commits. Run from Treehouse's /commit command.
disable-model-invocation: true
---

# Commit the work so far

1. Read `git status` and the full diff, untracked files included (`git diff`, `git diff --cached`, and the contents of new files).
2. Group the changes into logical commits: one per self-contained change, such as a feature and its tests, or an unrelated fix on its own. Stage each commit with paths, or `git add -p` for single hunks.
3. Leave out scratch files, debug output and generated files, and say which ones you left out and why.
4. Write each message as a Conventional Commit: `type(scope): subject`, imperative, under 72 characters, with a body saying why when that isn't obvious from the subject. If `git log` shows the repository uses another style, follow that instead.
5. Never amend, rebase or push unless the user asks. Never skip hooks (`--no-verify`). If a hook fails, fix the cause and commit again.
6. Finish with a one-line summary. Treehouse shows the commits itself, so don't list them again.

If the user added instructions after the command, they come first.
