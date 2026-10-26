# To do

What's left to build, most valuable first. Update it as items are finished or added.

The design lives on the [Treehouse design canvas](https://claude.ai/artifact/FNq7DKcbtZat4xzPXmefkk). New UI starts there: draw the boards, get them reviewed, then build.

## Sub-agent worktrees

Now that the worktree lifecycle is in, this can be picked up.

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

- Clicking a commit in a commit card should open its diff in the right dock.
- After a squash merge the worktree's diff stat still counts the branch's changes, since it compares with the merge base.
- The sidebar's branch markers refresh after each turn and merge, not yet when the window gets focus.
- With **Compare against** set to `origin/main` the merge is refused; offer push and a pull request instead.
- "Jump to latest" flashes when scrolling past the end of the chat, probably the scroll bounce on macOS.
- In the collapsed rail, project tooltips open below the pointer rather than to the right.
- Restored sub-agents can come back in a different order in the sidebar than in the chat.
- A worktree setup script still running when Treehouse quits is left running; stop its process group on quit (`setup::stop`).
- The setup row doesn't come back after a restart, since Pi's session file doesn't hold it. Save it through the bridge's `/treehouse-save`, as commit cards are.
- "Ask Pi to fix it" sends the setup output in a code fence, which shows as literal backticks because the user's own messages aren't rendered as Markdown.
