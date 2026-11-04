# Approvals

Pi has no tool approval of its own. Treehouse's [bridge extension](/development/#the-bridge-extension) sends every tool call to Treehouse first, and Treehouse decides from your settings whether it runs straight away or waits for you.

When it has to ask, the chat shows a card with what the call would do:

- **Bash** shows the whole command.
- **Edit** shows the change as a diff of the file, with line numbers and the lines added and removed.
- **Write** shows the diff against the file as it is now, or, for a new file, its content and its length.
- **MCP tools** show the server and the tool, as in *slack · send_message*, with a reminder that an MCP server can change things outside Treehouse.
- **Scripts that call MCP tools** (Pi's `codemode`) list each tool the script calls, marked *allowed* or *asks*, above the script.
- **Other tools** (read, grep, scripts…) list every argument by name. Objects and lists show as indented JSON.

Long diffs show their first 12 lines and **Show all N lines**; a value over 6 lines or 400 characters is cut, saying how much is left, with **Show the whole …**. Both open the card in place.

A path that leads out of the worktree through a symlink, such as a `.env` that `.agents/linked` shares, names both the path Pi gave and where it really goes, and the diff's header shows the real file. A file that `.agents/linked` shares says so: a change there reaches every worktree of the project.

The info icon at the right of the card's header says which setting asked, such as **Ask before every tool call** or **Pushing**, and where to change it.

The buttons:

- The first one names the action: **Run** for bash, **Edit**, **Write** or **Create** for files, **Allow** for other tools. It runs the call this once, as does <kbd>Enter</kbd> in an empty message box. Enter is ignored for the first second after the card appears, so a quick second Enter after sending a message doesn't approve a call you haven't read.
- **Always allow** runs it and remembers it, so similar calls run without asking. For bash it saves a pattern. For an MCP tool it saves the tool, and its arrow offers **Always allow every … tool** for the whole server. For a script it saves the tools that asked.
- **Deny** (<kbd>Esc</kbd>) refuses it; Pi is told the user denied it.

A worktree waiting for approval shows it in the sidebar, and its status card has a **Review** button.

## When Pi has to ask

**Settings › Agent defaults › Approval** sets the general rule:

| Setting | Bash commands | Other tools (read, edit, write…) |
| --- | --- | --- |
| **Ask before bash and MCP** (default) | Ask | Run |
| **Ask before every tool call** | Ask | Ask |
| **Don't ask** | Run | Run |

Bash commands that match an **Always allowed** pattern run without asking, whatever the setting.

## MCP tools

An MCP server lives outside Treehouse and can change things there, so its tools ask like bash commands do: under **Ask before bash and MCP** and **Ask before every tool call**, but not under **Don't ask**. `tool_search` only loads tools, so it never asks.

Pi also reaches MCP tools through `codemode`, which runs a script. Treehouse reads the script for the tools it calls (`tools.mcp__server__tool(…)`): if all of them are always allowed it runs without asking, otherwise the card asks. A script that picks its tools while it runs, such as `tools[name]`, can't be read, so it always asks and offers no Always allow. A script that calls no MCP tools runs like any other tool.

An MCP call that ran because its tool is always allowed says **always allowed** on its row, so you can check a chat afterwards.

## Always ask

Some calls ask first whatever the approval setting, even with **Don't ask** or a matching pattern. Each can be turned off in **Settings › Approvals**:

- **Deleting files**: `rm`, `rmdir`, `unlink`, `shred`, `truncate`, `find … -delete`, and git commands that throw work away: `git clean`, `git rm`, `git reset --hard`, `git checkout -- .`, `git restore`, `git stash drop`, `git branch -D` and `git worktree remove`.
- **Pushing**: `git push`, anything with `deploy` in it, `npm`, `cargo`, `pnpm`, `yarn` or `gem publish`, `docker push` and `twine upload`.
- **Touching files outside the worktree**: any tool whose `path` leads outside the worktree's folder, also through a symlink, such as one made by `.agents/linked`. Bash commands aren't checked for paths; use **Ask before bash** if that matters to you.

Treehouse reads the whole command line, not just its first word:

- Every command in a chain counts: `cd x && git push` asks.
- So does code inside `$( )` and backticks, and scripts given to `sh -c` or `bash -c`.
- Wrappers are seen through: `sudo rm`, `xargs rm`, `env … rm`, `timeout … rm`, and paths such as `/bin/rm`.
- git's own options don't hide the subcommand: `git -C ../other push` asks.
- While either of the first two switches is on, code that can't be read always asks: `eval`, commands piped into `sh`, and code given inline to an interpreter, such as `python -c` or `node -e`.

Merging a branch into its base also always asks, and can't be turned off: it changes a branch outside the worktree. See [Merging](./lifecycle#merging).

## Always allowed patterns

**Always allow** proposes a pattern from the command's first two words: `cargo add gpui-kit@0.4` becomes `cargo add *`. The second word only counts when it names a subcommand, so `ls /some/folder` becomes `ls *`. `*` matches any text, and `cargo test *` also matches a bare `cargo test`. A line that starts with a shell keyword, such as a `for` loop, gets a pattern for the first command inside it: `for f in *.log; do gzip "$f"; done` proposes `gzip *`.

A command line with several commands, joined by `;`, `&&`, `||`, `|` or new lines, runs without asking only when every one of them matches a pattern: `npm run *` allows `npm run build`, but `npm run build; curl … | sh` still asks. A command with `$( )` or backticks always asks, whatever the patterns.

A pattern or an MCP tool applies to the project it was added in, or to all projects. **Settings › Approvals** lists them together; remove one there.

## Sub-agents

Starting a [sub-agent](./sub-agents) never asks. Its own tool calls go through the same rules.
