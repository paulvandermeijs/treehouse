# Slash and shell commands

## Slash commands

Type `/` in the message box to open the slash menu. <kbd>↑</kbd> <kbd>↓</kbd> (or <kbd>Ctrl+P</kbd> <kbd>Ctrl+N</kbd>) choose, <kbd>Tab</kbd> or <kbd>Enter</kbd> picks, <kbd>Esc</kbd> closes.

The menu lists:

| Command | What it does |
| --- | --- |
| `/skill:name` | Pi's skills for this worktree |
| `/template` | Pi's prompt templates |
| `/compact [instructions]` | Summarises the conversation so far to free up context (Pi's `compact`) |
| `/new` | Starts a new Pi session in this worktree (Pi's `new_session`) |
| `/reload` | Restarts the worktree's Pi on the same session |

Skills and templates come from Pi's `get_commands`, so they're the ones Pi finds for that worktree. Pi expands them; the chat shows the command as you typed it.

Pi's RPC mode has no reload, so `/reload` restarts Pi and resumes the same session. Use it after changing Pi's settings, skills, extensions or `mcp.json`.

::: info
Pi only loads a project's own skills, templates and extensions (`.pi/` and `.agents/skills`) once the project is [trusted](./trust).
:::

## Shell commands

A message that starts with `!` runs as a shell command in the worktree instead of going to the model. The message box turns yellow while you type one.

```text
!cargo test
!!git log --oneline -20
```

![A shell command in the message box, and the output of an earlier one in the chat](/screens/shell-latte.png){.screen .light-only loading=lazy}
![A shell command in the message box, and the output of an earlier one in the chat](/screens/shell-mocha.png){.screen .dark-only loading=lazy}

- `!` adds the command and its output to the conversation, so the model sees it.
- `!!` keeps the output out of the model's context.

The chat shows the output as it arrives, with the exit code and how long it took; a command that fails gets a red header. **Stop** ends a command that's still running.

Treehouse runs these through Pi's `bash` RPC command, so they work the same as `!` in Pi's own terminal UI.
