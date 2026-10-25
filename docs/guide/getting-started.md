# Install and run

Treehouse is a desktop app for running [Pi](https://github.com/badlogic/pi-mono) coding agents in git worktrees. Each worktree of a project gets its own Pi chat; Pi can start sub-agents that you can open, steer and stop; and the right dock shows the worktree's diff or a second chat.

## Requirements

- Rust 1.88 or later (edition 2024)
- Pi, installed with npm:

  ```sh
  npm install -g @earendil-works/pi-coding-agent
  ```

- On Linux, the X11/Wayland and Vulkan libraries GPUI needs. On Debian or Ubuntu:

  ```sh
  sudo apt install libxkbcommon-dev libxkbcommon-x11-dev libvulkan1 mesa-vulkan-drivers
  ```

## Build and start

```sh
git clone https://github.com/paulvandermeijs/treehouse
cd treehouse
cargo run --release
```

The first build takes a few minutes, mostly for the syntax highlighting grammars. To get an app with an icon in the Dock or your launcher, see [Packaging](./packaging).

## Your first chat

1. Click the folder button next to **Projects** in the sidebar (or press <kbd>⌘O</kbd>, <kbd>Ctrl+O</kbd> off macOS) and pick a git repository.
2. Treehouse lists the project's worktrees. Press <kbd>⌘N</kbd> to create a new one, or open the main worktree.
3. The first time you open a project, Treehouse asks whether you [trust the folder](./trust). Until you do, Pi can only read.
4. Type a message and press <kbd>Enter</kbd>. Pi starts with your first message.

## Pi and your shell

Treehouse looks for `pi` on the `PATH` your login shell sets up, so an app started from the Dock, Finder or a desktop launcher finds the `pi` and `node` that Homebrew, nvm or npm installed. It reads that `PATH` once at startup by running your shell (`$SHELL -l -i`). Set `TREEHOUSE_NO_SHELL_ENV=1` to skip this.

Treehouse uses Pi's own configuration and credentials. A key added in **Settings › Providers** also works for `pi` in a terminal, and the other way round.

## Where things are stored

| What | Where (Linux) |
| --- | --- |
| Treehouse settings | `~/.config/treehouse/settings.json` |
| Pi sessions and the bridge extension | `~/.local/share/treehouse/` |
| Which session belongs to which worktree | `~/.local/share/treehouse/sessions.json` |
| Project trust | `~/.pi/agent/trust.json` (shared with Pi) |
| Provider keys | `~/.pi/agent/auth.json` (shared with Pi) |

On macOS and Windows the first two use the platform's config and data folders.
