# Approvals

Pi has no tool approval of its own. Treehouse's [bridge extension](/development/#the-bridge-extension) sends every tool call to Treehouse first, and Treehouse decides from your settings whether it runs straight away or waits for you.

When it has to ask, the chat shows the call with three buttons:

- **Run** runs it this once.
- **Always allow** runs it and saves a pattern so similar commands run without asking.
- **Deny** (<kbd>Esc</kbd>) refuses it; Pi is told the user denied it.

A worktree waiting for approval shows it in the sidebar, and its status card has a **Review** button.

## When Pi has to ask

**Settings › Agent defaults › Approval** sets the general rule:

| Setting | Bash commands | Other tools (read, edit, write…) |
| --- | --- | --- |
| **Ask before bash** (default) | Ask | Run |
| **Ask before every tool call** | Ask | Ask |
| **Don't ask** | Run | Run |

Bash commands that match an **Always allowed** pattern run without asking, whatever the setting.

## Always ask

Some calls ask first whatever the approval setting, even with **Don't ask** or a matching pattern. Each can be turned off in **Settings › Approvals**:

- **Deleting files**: `rm`, `rmdir`, `unlink`, `shred`, `git clean`, `git rm` and `find … -delete`.
- **Pushing**: `git push`, `deploy`, and `npm`, `cargo`, `pnpm` or `yarn publish`.
- **Touching files outside the worktree**: any tool whose `path` points outside the worktree's folder.

## Always allowed patterns

**Always allow** proposes a pattern from the command's first two words: `cargo add gpui-kit@0.4` becomes `cargo add *`. The second word only counts when it names a subcommand, so `ls /some/folder` becomes `ls *`. `*` matches any text, and `cargo test *` also matches a bare `cargo test`.

A pattern applies to the project it was added in, or to all projects. **Settings › Approvals** lists them; remove one there.

## Sub-agents

Starting a [sub-agent](./sub-agents) never asks. Its own tool calls go through the same rules.
