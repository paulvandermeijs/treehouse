# To do

What's left to build, most valuable first. Update it as items are finished or added.

The design lives on the [Treehouse design canvas](https://claude.ai/artifact/FNq7DKcbtZat4xzPXmefkk). New UI starts there: draw the boards, get them reviewed, then build.

## Worktree lifecycle

Commit, sync, merge and clean up from Treehouse, done by Pi rather than by hand. The boards are on the canvas (Lifecycle page) and wait for a review.

- `/commit`, `/sync` and `/finish-branch`, backed by three built-in skills: `treehouse-commit`, `treehouse-sync` and `treehouse-finish-branch`. They're shipped in Treehouse's data folder and loaded with `--skill`. The prefix is there because Pi keeps the first skill it finds with a name, so a built-in `commit` would hide a project's own. `/commit` runs the project's or the user's `commit` skill when Pi lists one, and the built-in otherwise.
- Pi's git commands show as normal rows, followed by a card per conventional commit (type, subject, body, hash), built from `git log` rather than from Pi's reply.
- "Commit with Pi" and "Finish branch" in the diff dock and the worktree's context menu send the same commands. There's no manual commit UI.
- Finish branch: Pi commits, syncs with the base branch and runs the tests, then calls a new merge tool in the bridge extension (`assets/pi/treehouse.ts`). Treehouse checks that the base is clean, never forces, and shows a merge approval card. After the merge, a card offers Clean up (safe worktree delete already exists).
- `/sync` has Pi resolve conflicts. Refusals such as "main isn't clean" get their own states.

Open questions:

1. Should the merge approval always ask, even with approvals set to "Don't ask"? The boards assume yes.
2. Fast-forward by default, falling back to a merge commit, with squash as an option?
3. Should `/finish-branch` run the tests itself, and if so, how does it find the test command? Or leave it to Pi and the project's AGENTS.md?
4. Hide these commands in untrusted projects, or show them disabled with the read-only note?

## Sub-agent worktrees

Only start this once the worktree lifecycle is fully in.

- Sub-agents run in their parent's worktree, so two agents making changes can get in each other's way. Give each sub-agent its own worktree ("Every agent deserves its own space"), set up the same way as a new worktree, and merge or clean it up through the worktree lifecycle. Read-only sub-agents, such as research, should keep working.

## Next

- **Remember UI state:** the project tree opens fully expanded every time; store which items are collapsed and restore them on start. Restoring whether the right dock is open is lower priority.
- **Images:** paste or drop screenshots into the message box, and show thumbnails when Pi reads an image.
- **Plan cards:** a built-in skill whose plan shows as a checklist card that updates as Pi works.
- **MCP:** an approval option for MCP tool calls, and a `/mcp` command.
- **Context popover:** hovering the token count in the status bar shows what's using the context.
- **Custom fonts** for the interface and code in Settings, plus the optional text-size stepper.
- **Mermaid diagrams** in replies.

## Small fixes

- "Jump to latest" flashes when scrolling past the end of the chat, probably the scroll bounce on macOS.
- In the collapsed rail, project tooltips open below the pointer rather than to the right.
- Restored sub-agents can come back in a different order in the sidebar than in the chat.
- A worktree setup script still running when Treehouse quits is left running; stop its process group on quit (`setup::stop`).
- The setup row doesn't come back after a restart, since Pi's session file doesn't hold it.
- "Ask Pi to fix it" sends the setup output in a code fence, which shows as literal backticks because the user's own messages aren't rendered as Markdown.
