# Projects and worktrees

## Projects

Add a project with the folder button next to **Projects**, <kbd>⌘O</kbd>, or **Open project…** in the command palette. Treehouse lists the project's worktrees as `git worktree list` reports them.

Projects are listed in the order you added them. To move one to the top, hover it and click the pin at the right of its row, or choose **Pin to top** from its context menu. Pinned projects keep their pin icon and stay in the order you pinned them; to reorder, unpin and pin again.

Right-click a project for **New worktree**, **Pin to top**, **Reveal in file manager**, **Copy path** and **Remove project**. Removing a project only takes it out of the sidebar; nothing on disk changes.

## Creating a worktree

**New worktree** (<kbd>⌘N</kbd>, or right-click a project) asks for a branch name, such as `feat/my-change`. Treehouse creates the branch from the project's current `HEAD` and checks it out in `{project}/.worktrees/{branch}`. You can change that location in **Settings › Worktrees**; `{project}` and `{branch}` are filled in.

If you set a **Setup command** in **Settings › Worktrees** (for example `npm install`), Treehouse runs it once in every new worktree. It doesn't run in projects you haven't trusted.

::: tip
Add `.worktrees/` to your project's `.gitignore`, or pick a location outside the project, so the worktrees don't show up as untracked files.
:::

## Renaming and deleting

Right-click a worktree for **Rename branch**, **Show diff**, **Open in right dock** and **Delete worktree…**.

Deleting removes the worktree's folder and stops its chat and sub-agents. The branch is kept unless **Also delete its folder and branch** is on in **Settings › Worktrees**. If the worktree has uncommitted changes, or commits that would be lost with the branch, the dialog lists them and asks you to **Delete anyway**.

## The sidebar

Each worktree and sub-agent has a status dot: working, needs input, failed, finished or idle. A worktree shows its most urgent status, so a collapsed worktree still tells you a sub-agent is waiting.

### Status cards

Hover a worktree or sub-agent to see what it's doing: the running tool and the latest reply, what it's waiting for (with a **Review** button), the error, or when it was last active, plus the worktree's diff stat or the sub-agent's tool calls.

### Collapsed sidebar

<kbd>⌘B</kbd> collapses the sidebar to a 48px rail. Each row stays where it is in the full sidebar: a project shows its folder, a worktree or sub-agent its status dot. Hovering shows the name, or for a worktree or sub-agent its status card. Clicks and right-click menus work as in the full sidebar.

The rail's footer keeps only Settings; a red dot on it means the provider has no key. On macOS, outside full screen, the window's traffic lights don't fit in the rail, so the toggle moves to the top bar.

### Opening in the right dock

Hold <kbd>⌘</kbd> (<kbd>Ctrl</kbd> off macOS) and click a worktree or sub-agent to open it in the right dock instead of the main chat, so you can follow two chats side by side.

## Switching quickly

<kbd>⌘P</kbd> opens a switcher with every worktree and sub-agent. Type a few letters of a branch, sub-agent or project ("fdv" finds `feat/diff-view`) and press <kbd>Enter</kbd> to open it, or <kbd>⌘Enter</kbd> to open it in the right dock.
