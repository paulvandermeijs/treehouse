# Install and run

Treehouse is a desktop app for running [Pi](https://github.com/badlogic/pi-mono) coding agents in git worktrees. Each worktree of a project gets its own Pi chat; Pi can start sub-agents that you can open, steer and stop; and the right dock shows the worktree's diff or a second chat.

## Pi

Treehouse runs Pi, so install it first, with npm:

```sh
npm install -g @earendil-works/pi-coding-agent
```

## Download

Each release on GitHub has the app ready to run:

| Platform | Download |
| --- | --- |
| macOS, Apple silicon | [Treehouse-macos-arm64.zip](https://github.com/paulvandermeijs/treehouse/releases/latest/download/Treehouse-macos-arm64.zip) |
| macOS, Intel | [Treehouse-macos-x86_64.zip](https://github.com/paulvandermeijs/treehouse/releases/latest/download/Treehouse-macos-x86_64.zip) |
| Linux (Debian, Ubuntu) | [treehouse-linux-x86_64.deb](https://github.com/paulvandermeijs/treehouse/releases/latest/download/treehouse-linux-x86_64.deb) |
| Linux (other) | [Treehouse-linux-x86_64.AppImage](https://github.com/paulvandermeijs/treehouse/releases/latest/download/Treehouse-linux-x86_64.AppImage) |
| Windows | [treehouse-windows-x86_64.zip](https://github.com/paulvandermeijs/treehouse/releases/latest/download/treehouse-windows-x86_64.zip) |

The [releases page](https://github.com/paulvandermeijs/treehouse/releases/latest) lists what changed in each version.

The macOS app is signed and notarized by Apple. Windows may warn that the app is from an unknown publisher; choose **More info › Run anyway**.

On Linux, the `.deb` installs the libraries GPUI needs. The AppImage runs on any distribution once it's executable (`chmod +x`), with the Vulkan and xkbcommon libraries installed (see below).

## Build from source

You need Rust 1.88 or later (edition 2024). On Linux, GPUI also needs the X11/Wayland and Vulkan libraries. On Debian or Ubuntu:

```sh
sudo apt install libxkbcommon-dev libxkbcommon-x11-dev libvulkan1 mesa-vulkan-drivers
```

Then build and start it:

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
| The open chat, what's collapsed in the sidebar, and whether the diff is open | `~/.local/share/treehouse/ui.json` |
| Project trust | `~/.pi/agent/trust.json` (shared with Pi) |
| Provider keys | `~/.pi/agent/auth.json` (shared with Pi) |

On macOS and Windows the first two use the platform's config and data folders.
