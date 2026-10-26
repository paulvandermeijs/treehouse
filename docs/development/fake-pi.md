# Running without a model

`tools/fake-pi` is a Python stand-in for `pi --mode rpc` that plays scripted turns. Use it to work on the UI without a model or an API key.

Set it as the **Pi command** in **Settings › Agent defaults** (the `pi_command` key in `settings.json`), using the full path to `tools/fake-pi` in your clone.

## Scripts

The first word of a message picks the script:

| First word | Plays |
| --- | --- |
| `Find` | A sub-agent |
| `Fail` | A provider error |
| `Approve` | A long approval pattern |
| `Long` | A long-running command |
| `Mangle` | A garbled tool name |
| `MCP` | MCP tool calls |
| `Code` | Code blocks |
| `Table` | A table |
| anything else | A worktree turn with tools, an approval and a sub-agent |

`/setup-worktrees` writes a sample `.agents/linked` and `.agents/prepare`. `/commit`, `/sync` and `/finish-branch` run real git in the worktree: they commit everything, rebase onto the base branch named in `--append-system-prompt`, and call `treehouse_merge`, so the whole lifecycle can be tried on a scratch repository.

It also handles `/mcp login`, and `!` shell commands, which it runs for real through Pi's `bash` RPC. Set `FAKE_PI_MCP=1` to make it report an MCP server that needs a sign-in.

It saves the chat to a session file in Pi's format (`--session-dir`, `--session`), so restoring history can be tried too. Extend it when a feature needs a new Pi behaviour to test against.

## Screenshots without a display

Run the app under Xvfb and drive it with `xdotool`. Point `XDG_CONFIG_HOME` and `XDG_DATA_HOME` at a scratch folder for Treehouse's settings, and `PI_CODING_AGENT_DIR` at one for Pi's (`trust.json`, `auth.json`), so your real settings aren't touched.
