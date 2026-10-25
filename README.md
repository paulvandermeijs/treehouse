# Treehouse

A desktop app for running [Pi](https://github.com/badlogic/pi-mono) coding agents in git worktrees. Each worktree of a project gets its own Pi chat; Pi can start sub-agents that you can open, steer and stop; and the right dock shows the worktree's diff or a second chat.

Treehouse is written in Rust with [GPUI](https://www.gpui.rs) and [gpui-kit](https://gpui-kit.com/docs/). It drives Pi through Pi's [RPC mode](https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/rpc.md): every session is a `pi --mode rpc` process.

## Running it

You need Rust 1.88 or later (edition 2024) and Pi:

```sh
npm install -g @earendil-works/pi-coding-agent
cargo run --release
```

On Linux, GPUI also needs the X11/Wayland and Vulkan development libraries, for example on Debian or Ubuntu:

```sh
sudo apt install libxkbcommon-dev libxkbcommon-x11-dev libvulkan1 mesa-vulkan-drivers
```

Add a project with the folder button next to **Projects**. Treehouse lists the project's worktrees (`git worktree list`). **New worktree** (⌘N, or right-click a project) creates a branch in `{project}/.worktrees/{branch}`, which you can change in Settings › Worktrees.

Pi starts the first time you send a message in a worktree. Treehouse looks for `pi` on the `PATH` your login shell sets up, so an app started from the Dock, Finder or a desktop launcher finds the `pi` and `node` that Homebrew, nvm or npm installed. It reads that `PATH` once at startup by running your shell (`$SHELL -l -i`); set `TREEHOUSE_NO_SHELL_ENV=1` to skip this. It uses Pi's own configuration and credentials, so a key added in Settings › Providers also works for Pi in a terminal, and the other way round.

Settings are saved in the platform config folder (`~/.config/treehouse/settings.json` on Linux). Pi sessions are saved under the platform data folder (`~/.local/share/treehouse/sessions`).

### App icon and packaging

The icon is `assets/icon/treehouse.svg`, from the design. `cargo run --example render-icons` renders it into `assets/icon/png/`, `treehouse.icns` (macOS, on Apple's icon grid) and `treehouse.ico` (Windows); run it after changing the SVG.

- **macOS:** the Dock shows the icon once Treehouse runs as an app bundle: `cargo install cargo-bundle`, then `cargo bundle --release` builds `target/release/bundle/osx/Treehouse.app`. Started with `cargo run`, it's a bare executable and shows the default icon.
- **Linux:** X11 windows get the icon directly. Wayland docks look it up through the desktop file: copy `assets/linux/treehouse.desktop` to `~/.local/share/applications/` and `assets/icon/png/256x256.png` to `~/.local/share/icons/hicolor/256x256/apps/treehouse.png`. `cargo bundle --release` also builds a `.deb` with both.
- **Windows:** `build.rs` embeds `treehouse.ico` in the executable.

### Collapsed sidebar

⌘B (Ctrl+B) collapses the sidebar to a 48px rail. Each row stays where it is in the full sidebar: a project shows its folder, a worktree or sub-agent its status dot, and hovering shows the name. Clicks and right-click menus work as in the full sidebar. The footer keeps only Settings; a red dot on it means the provider has no key. On macOS, outside full screen, the traffic lights don't fit in the rail, so the toggle moves to the top bar beside it.

### Chat history

Each worktree's chat comes back when you restart Treehouse. Pi saves every session as a file in Treehouse's data folder; Treehouse remembers which file belongs to which worktree (`sessions.json` there) and shows the chat again when you open the worktree. Your next message continues the same Pi session, so Pi has the whole conversation. `/new` starts a new session, which is the one that comes back from then on. Sub-agents from before the restart show as finished cards with their report.

### Slash commands

Type `/` in the message box for Pi's skills (`/skill:name`) and prompt templates, plus `/compact [instructions]`, `/new` and `/reload`. ↑↓ choose, Tab or Enter picks, Esc closes. Skills and templates come from Pi's `get_commands`, so they are the ones Pi finds for that worktree; Pi expands them, and the chat shows the command as you typed it. `/compact` and `/new` run Pi's `compact` and `new_session` RPC commands. RPC mode has no reload, so `/reload` restarts the worktree's Pi on the same session; use it after changing Pi's settings, skills, extensions or `mcp.json`.

Pi only loads a project's own skills, templates and extensions (`.pi/` and `.agents/skills`) once the project is trusted.

### Project trust

The first time you open a project's worktree, Treehouse asks whether you trust the folder. Until you do, Pi runs read-only: it gets only the `read` tool, skips the project's own settings and skills, connects no MCP servers, and Treehouse doesn't run the worktree setup command. An untrusted project shows a warning in the sidebar and a **Read only** button in the chat's top bar, which opens the dialog again. Trusting can cover every folder in the project's parent folder.

Decisions are saved to Pi's own `~/.pi/agent/trust.json`, so `pi` in a terminal sees them too, and Treehouse passes them to Pi as `--approve` or `--no-approve`. Changing a decision restarts the worktree's Pi with the new tools, resuming the same Pi session.

### MCP sign-in

Pi reads MCP servers from `~/.pi/agent/mcp.json` and, in trusted projects, `.pi/mcp.json`. When a server needs a sign-in, the chat shows it with a **Sign in** button. That runs Pi's `/mcp login <server>`: Pi opens the browser, and the card waits for it. If the browser can't reach this machine (a remote session, for example), paste the address it ended up on into the card.

Pi reaches MCP tools through its `codemode` and `tool_search` tools, so Treehouse adds those to the tools it gives Pi. The tools a script calls still go through approval.

Some servers only accept sign-ins from apps they have approved and refuse Pi with "dynamic client registration failed with status 403". Figma's remote server is one. For Figma, its desktop app's local MCP server (`http://127.0.0.1:3845/mcp`) needs no sign-in.

### Syntax highlighting

Code blocks in the chat and the diff dock are highlighted with tree-sitter for Astro, Bash, CSS, HTML, JavaScript, JSON, Markdown, Python, Rust, TOML, TSX, TypeScript and YAML, in the theme's Catppuccin colours. Other languages show as plain text. Each grammar is a gpui-kit feature in `Cargo.toml`; the first build with them takes a few minutes, after which cargo reuses them.

### Tables

Markdown tables in Pi's replies follow the design: a mantle header, rows divided by hairlines, a rounded border, and a copy button (Markdown or CSV) under the table. Columns whose values are all numbers, dates, sizes or durations are right-aligned once the reply has finished; an alignment marker in the Markdown (`:--:`) wins.

### Shell commands

A message that starts with `!` runs as a shell command in the worktree instead of going to the model. The message box turns maroon while you type one. Treehouse runs it through Pi's `bash` RPC command, so Pi adds the command and its output to the conversation for the model to see. Start with `!!` to keep the output out of the model's context. The chat shows the output as it arrives, with the exit code and time. Stop ends a command that's still running.

### Without a model

`tools/fake-pi` speaks the same RPC protocol and plays back a scripted turn with tool calls, an approval and a sub-agent. Set it as the Pi command in Settings › Agent defaults to work on the UI without a model or an API key. `FAKE_PI_MCP=1` makes it report an MCP server that needs a sign-in.

## How it fits together

| Path | What it does |
| --- | --- |
| `src/pi/rpc.rs` | Starts `pi --mode rpc`, writes commands and reads events as JSON lines |
| `src/pi/auth.rs` | Reads and writes provider keys in Pi's `auth.json` |
| `assets/pi/treehouse.ts` | The Pi extension Treehouse loads into every session (see below) |
| `src/transcript.rs` | Turns Pi's events into chat items: messages, thinking, tool calls, notes, errors |
| `src/session.rs` | One Pi session: its process, transcript, status, model and pending approvals |
| `src/history.rs` | Which Pi session file holds each worktree's chat, and reading one back |
| `src/slash.rs` | The slash menu's commands: matching, Pi's skills and templates, `/compact` and `/new` |
| `src/approval.rs` | Decides which tool calls need the user's approval |
| `src/trust.rs` | Project trust, read from and saved to Pi's `trust.json` |
| `src/mcp.rs` | Reads Pi's MCP sign-in messages |
| `src/workspace.rs` | Projects, worktrees and sub-agents; reports sub-agent results to their parent |
| `src/git.rs` | Worktree list/add/remove and the diff since the base branch |
| `src/theme.rs`, `assets/themes/treehouse.json` | Catppuccin Mocha and Latte, with IBM Plex Sans and Mono |
| `src/ui/` | The window: sidebar, chat, tool output, diff dock, command palette, settings |

### The bridge extension

Pi has no built-in tool approval or sub-agents. Treehouse adds both with a small extension (`assets/pi/treehouse.ts`) that it writes to its data folder and passes to Pi with `-e`:

- **Approval.** Every tool call is sent to Treehouse as an RPC extension UI request. Treehouse applies the approval setting (Ask before bash, Ask before every tool call, Don't ask), the "Always allow" patterns and the "always ask" rules for deleting, pushing and files outside the worktree. When it has to ask, the chat shows the command with Run, Always allow and Deny.
- **Sub-agents.** The extension registers a `subagent` tool. When Pi calls it, Treehouse starts a separate Pi session in the same worktree, lists it under the worktree in the sidebar, and returns that session's final answer as the tool result. You can open a sub-agent, steer it or stop it like any other chat.

### gpui-kit components used

The sidebar is laid out by hand to the design with gpui-kit buttons and context menus; `Diff` for the worktree diff in unified and split layouts; `Command` in a `Dialog` for the command palette; `Settings` for the settings dialog; `TextView` for streaming Markdown; `ShimmerText` and `Spinner` for running work; `Textarea`, `Button`, `ButtonGroup`, `PopupMenu`, `Kbd` and alert dialogs throughout.

## Status

This implements the screens of the Treehouse design: the worktree chat, sub-agents in the main screen and the right dock, the diff dock and its full-screen split view, the command palette, settings, the sidebar states, the trust prompt and read-only projects, and the agent output for messages, thinking, read, edit, write, bash, approvals, sub-agent cards, notes and errors.

Not built yet, from the design's output boards: data tables (long tables and CSV) and charts from tool output, Mermaid diagrams, maths, image thumbnails, plan cards, local server links, the "Open in terminal" dock tab, and a test button for provider keys.

## Licences

IBM Plex Sans and IBM Plex Mono are bundled under the SIL Open Font License (`assets/fonts/OFL.txt`). The colours are Catppuccin's (MIT).

## Development

```sh
cargo test
cargo clippy --all-targets
```
