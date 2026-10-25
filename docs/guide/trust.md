# Project trust

A project's own files can make Pi run code: its `.pi/` settings and extensions, its skills, its MCP servers and its worktree setup (`.agents/prepare`). So the first time you open a worktree of a project, Treehouse asks whether you trust the folder.

- **Trust folder** (<kbd>Enter</kbd>) gives Pi its normal tools. Tick **Trust every folder in …** first to trust the project's parent folder, so every project in it is trusted too.
- **Open read-only** on the first prompt, or **Keep read-only** (<kbd>Esc</kbd>) when you reopen it, keeps the project untrusted.

![The trust prompt on first opening a project](/screens/trust-latte.png){.screen .light-only loading=lazy}
![The trust prompt on first opening a project](/screens/trust-mocha.png){.screen .dark-only loading=lazy}

## Read-only projects

Until you trust a project:

- Pi gets only the `read` tool, and is told it is read-only.
- Pi skips the project's own settings, skills, templates and extensions.
- No MCP servers are connected.
- Treehouse doesn't [prepare new worktrees](./worktrees#preparing-new-worktrees); the setup waits until you trust the project.

An untrusted project shows a warning in the sidebar and a **Read only** button in the chat's top bar. Click it to open the trust dialog again.

## Where decisions are stored

Decisions are saved in Pi's own `~/.pi/agent/trust.json`, so `pi` in a terminal sees them too, and the other way round. Treehouse passes them to Pi as `--approve` or `--no-approve`.

Changing a decision restarts the worktree's Pi with the new tools, resuming the same Pi session, so the conversation carries on.
