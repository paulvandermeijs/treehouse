---
name: treehouse-setup-worktrees
description: Work out what a fresh worktree of this project needs, and write .agents/linked and .agents/prepare so Treehouse prepares new worktrees by itself. Run from Treehouse's /setup-worktrees command.
disable-model-invocation: true
---

# Set up new worktrees

A new worktree is a fresh checkout: it has none of the files git ignores, so no dependencies, no build output and no local configuration. Treehouse prepares each new worktree from two files the project commits, in this order:

1. **`.agents/linked`**: git-ignored paths, one per line, relative to the project root. Each becomes a symlink to the main worktree's copy, so every worktree shares one file. Blank lines and lines starting with `#` are skipped. A path git doesn't ignore is skipped too, so a tracked file never ends up behind a link.
2. **`.agents/prepare`**: an executable POSIX shell script, run from the new worktree's root with `ROOT_WORKTREE_PATH` set to the main worktree's path. Its output shows in the chat. On Windows it runs with Git Bash.

Either file is optional. If one exists already, improve it rather than starting over, and keep what the user wrote.

## Work out what's needed

1. Read the README, AGENTS.md, CONTRIBUTING and similar docs for setup steps.
2. Find the package managers and build tools from their files: lockfiles (`package-lock.json`, `pnpm-lock.yaml`, `yarn.lock`, `bun.lockb`, `Cargo.lock`, `poetry.lock`, `uv.lock`, `Gemfile.lock`, `composer.lock`, `go.sum`), `Makefile`, `justfile`, `.tool-versions`, `.nvmrc`, `docker-compose.yml`.
3. List what git ignores in the main worktree, which is what a new worktree lacks: `git -C "$(git rev-parse --path-format=absolute --git-common-dir)/.." status --ignored --short`, or `git status --ignored --short` when you're in the main worktree. Look at `.gitignore` to see why each is ignored.
4. Sort the ignored files:
   - **Link** what is hand-made and must be the same everywhere: secrets and local settings such as `.env`, `.env.local`, `config/local.*`, credentials, editor settings the team doesn't commit. Never link anything that holds a worktree's own state.
   - **Rebuild in `prepare`** what tools generate: dependencies (`node_modules`, `vendor`, virtualenvs), build output, caches, generated code. Linking these would let worktrees overwrite each other.
   - **Leave out** logs, caches nothing needs and personal scratch files.
   - Ask the user when you can't tell, for example a large data folder that could be shared or copied.

## Write the files

- `.agents/linked`: one path per line, with a short `#` comment above each group saying why it's shared.
- `.agents/prepare`: start with `#!/bin/sh` and `set -e`, use the project's own lockfile-respecting install commands (`npm ci`, `pnpm install --frozen-lockfile`, `cargo fetch`, `uv sync`…), and add only steps a fresh checkout needs before work starts. Copy rather than link what each worktree should be able to change, using `"$ROOT_WORKTREE_PATH"`. Keep it fast and quiet, and make it safe to run twice.
- Make the script executable: `chmod +x .agents/prepare`. Check it with `sh -n .agents/prepare`.
- Don't run the script in the main worktree just to try it unless the user agrees; it may reinstall everything.

## Finish

Explain in a few lines what you link, what the script does and why, and what you left out. Don't commit unless the user asks: both files need to be committed to the branch new worktrees start from, usually the main branch, before they take effect. In a worktree that isn't on the main branch, say that they apply once the branch is merged.

If the user added instructions after the command, they come first.
