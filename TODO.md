# To do

What's left to build, most valuable first. Update it as items are finished or added.

The design lives on the [Treehouse design canvas](https://claude.ai/artifact/FNq7DKcbtZat4xzPXmefkk). New UI starts there: draw the boards, get them reviewed, then build.

## Next

- **Auto mode, next:** the quick check's threshold for a classifier (0.2) is a guess; tune it against real sessions, as `pi-automode-classifier` did. A chat's mode and tools aren't kept across restarts. The checks run one after another in the chat's Pi; a slow model holds up the turn.
- **Other extensions' dialogs:** Treehouse answers any `confirm`, `select`, `input` or `editor` request it doesn't know with cancel (`Session::handle_ui_request`), so Pi extensions that ask the user, such as the auto-mode guardrails in Pi's package catalogue, refuse instead of asking. Show them as cards in the chat.
- **Signed Windows releases:** SmartScreen warns about the unsigned `treehouse.exe`. Azure Trusted Signing is the cheapest way to sign it from the release workflow.
- **Images:** paste or drop screenshots into the message box, and show thumbnails when Pi reads an image.
- **Plan cards:** a built-in skill whose plan shows as a checklist card that updates as Pi works.
- **MCP:** a `/mcp` command.
- **Always allow edits:** with Ask before every tool call, edit and write cards offer "Always allow edits in this worktree" in the approval cards design, only when that setting is why it asked. It needs a per-worktree allow rule in the settings, the policy and Settings › Approvals.
- **Context popover:** hovering the token count in the status bar shows what's using the context.
- **Text-size stepper** in Settings › Appearance, in place of the Text size dropdown.
- **Mermaid diagrams:** save as SVG, as the design has it.

## Security

Open findings from the [security evaluation](https://claude.ai/artifact/Fir8dZeBaEgBz63tBP9ELo), by its numbers. 1, 3–8 and 11–14 are fixed, and 9 with auto mode.

- **2 · Links (high):** done but for the hover: every link should show its full target in a tooltip (a globe for links that open, a warning sign for ones that ask, "· not github.com" when the text names another site). gpui-kit's `TextView` has no link hover hook or tooltip, so it needs one upstream; drawing links through an inline Markdown plugin instead would lose their wrapping and selection.
- **10 · Trust prompt (low):** say what trusting runs: the project's `.pi` extensions, its MCP servers and `.agents/prepare`.

## Small fixes

- Two sub-agents running at once may still edit the same file, though the `subagent` tool asks Pi not to. If that shows up in practice, flag the overlap on their cards in the parent's chat.
- A worktree removed outside Treehouse (`git worktree remove` in a terminal) stays in the sidebar until a restart; drop it once its chat isn't running.
- Clicking a commit in a commit card should open its diff in the right dock.
- After a squash merge the worktree's diff stat still counts the branch's changes, since it compares with the merge base.
- The sidebar's branch markers refresh after each turn and merge, not yet when the window gets focus.
- With **Compare against** set to `origin/main` the merge is refused; offer push and a pull request instead.
- In the collapsed rail, project tooltips open below the pointer rather than to the right.
- Restored sub-agents can come back in a different order in the sidebar than in the chat.
- A worktree setup script still running when Treehouse quits is left running; stop its process group on quit (`setup::stop`).
