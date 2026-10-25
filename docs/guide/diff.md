# Reviewing the diff

<kbd>⌘D</kbd>, **Show diff** in a worktree's context menu, or the diff button in the chat's top bar opens the worktree's changes in the right dock.

![The worktree's diff in the right dock, in the unified layout](/screens/diff-latte.png){.screen .light-only loading=lazy}
![The worktree's diff in the right dock, in the unified layout](/screens/diff-mocha.png){.screen .dark-only loading=lazy}

The diff shows everything since the branch split off from its base: commits, uncommitted changes and new untracked files. The header says what it compares, for example `feat/diff-view since main`.

- **Unified** and **Split** switch between one column and side by side.
- **Previous change** and **Next change** step through the hunks.
- **Refresh** reloads it. It also updates by itself when the worktree changes.
- <kbd>⌘⇧F</kbd> or **Full screen** gives the dock the whole window.

## What it compares against

**Settings › Worktrees › Compare against** picks the base:

- **The branch it was created from** (the default)
- **origin/main**

The same base is used for the `+/−` counts in the sidebar and status cards.

**Keep diffs expanded after a turn** in **Settings › Appearance** keeps the edit rows in the chat open once Pi's turn has finished.

## The right dock

The dock holds the diff or a second chat. <kbd>⌘&#92;</kbd> opens and closes it. <kbd>⌘</kbd>-click a worktree or sub-agent in the sidebar, or press <kbd>⌘Enter</kbd> in the switcher, to open its chat there.
