# Projects and worktrees

## Projects

Add a project with the folder button next to **Projects**, <kbd>⌘O</kbd>, or **Open project…** in the command palette. Treehouse lists the project's worktrees as `git worktree list` reports them.

Projects are listed in the order you added them. To move one to the top, hover it and click the pin at the right of its row, or choose **Pin to top** from its context menu. Pinned projects keep their pin icon and stay in the order you pinned them; to reorder, unpin and pin again.

Right-click a project for **New worktree**, **Pin to top**, **Reveal in file manager**, **Copy path** and **Remove project**. Removing a project only takes it out of the sidebar; nothing on disk changes.

## Creating a worktree

**New worktree** (<kbd>⌘N</kbd>, or right-click a project) asks for a branch name, such as `feat/my-change`. Treehouse creates the branch from the project's current `HEAD` and checks it out in `{project}/.worktrees/{branch}`. You can change that location in **Settings › Worktrees**; `{project}` and `{branch}` are filled in.

## Preparing new worktrees

A new worktree is a fresh checkout: it has no `node_modules`, no `.env` and nothing else git ignores. A project can prepare its worktrees with two files of its own, as in [Delta](https://delta.dev/docs/concepts/worktrees#configuring-your-checkouts). Both are committed to the project.

To have Pi write them, run `/setup-worktrees` in the project's main worktree. Pi reads the project's docs and tooling, sorts what git ignores into what to share and what to rebuild, writes both files and explains its choices; it asks when it can't tell. Commit them to the main branch for new worktrees to pick them up.

### `.agents/linked`

Lists git-ignored paths, one per line, that every worktree shares with the main worktree:

```text
.env
config/local.json
```

Each becomes a symlink to the main worktree's copy. A path git doesn't ignore is skipped, so no tracked file ends up behind a link. If only the new worktree has the file, it is moved to the main worktree first.

### `.agents/prepare`

An executable script that Treehouse runs from the new worktree's root, for installing dependencies and the like. `ROOT_WORKTREE_PATH` holds the main worktree's path. On Windows it runs with Git Bash.

```sh
#!/bin/sh
set -e
npm ci
```

### While it runs

The setup shows as the first row of the new worktree's chat, with the linked paths and the script's output. A message you send meanwhile waits, and goes to Pi when the setup finishes. Stop ends the script and everything it started.

If the setup fails, the worktree is still usable. **Run again** retries it, and **Ask Pi to fix it** sends Pi the output. Right-click a worktree for **Run setup again** at any time.

Both files are the project's own code, so in a project you haven't [trusted](./trust) the setup waits until you trust it.

::: tip
Add `.worktrees/` to your project's `.gitignore`, or pick a location outside the project, so the worktrees don't show up as untracked files.
:::

## Renaming and deleting

Right-click a worktree for **Rename branch**, **Show diff**, **Open in right dock** and **Delete worktree…**, and for **Commit with Pi**, **Sync with main** and **Finish branch** ([Committing and merging](./lifecycle)). Once a worktree is merged, **Clean up…** takes the place of **Delete worktree…**.

Deleting removes the worktree's folder and stops its chat and sub-agents. The branch is kept unless **Also delete its folder and branch** is on in **Settings › Worktrees**. If the worktree has uncommitted changes, or commits that would be lost with the branch, the dialog lists them and asks you to **Delete anyway**.

## The sidebar

Each worktree and sub-agent has a status dot: working, needs input, failed, finished or idle. A worktree shows its most urgent status, so a collapsed worktree still tells you a sub-agent is waiting.

Before the dot, markers show where the worktree's branch stands: uncommitted changes, commits ahead of and behind the base, or merged. See [Where each branch stands](./lifecycle#where-each-branch-stands).

### Status cards

Hover a worktree or sub-agent to see what it's doing: the running tool and the latest reply, what it's waiting for (with a **Review** button), the error, or when it was last active, plus the worktree's diff stat or the sub-agent's tool calls.

### Collapsed sidebar

<kbd>⌘B</kbd> collapses the sidebar to a 48px rail. Each row stays where it is in the full sidebar: a project shows its folder, a worktree or sub-agent its status dot. Hovering shows the name, or for a worktree or sub-agent its status card. Clicks and right-click menus work as in the full sidebar.

The rail's footer keeps only Settings; a red dot on it means the provider has no key. On macOS, outside full screen, the window's traffic lights don't fit in the rail, so the toggle moves to the top bar.

### Opening in the right dock

Hold <kbd>⌘</kbd> (<kbd>Ctrl</kbd> off macOS) and click a worktree or sub-agent to open it in the right dock instead of the main chat, so you can follow two chats side by side.

## Switching quickly

<kbd>⌘P</kbd> opens a switcher with every worktree and sub-agent. Type a few letters of a branch, sub-agent or project ("fdv" finds `feat/diff-view`) and press <kbd>Enter</kbd> to open it, or <kbd>⌘Enter</kbd> to open it in the right dock.
