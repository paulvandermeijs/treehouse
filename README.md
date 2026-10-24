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

Pi starts the first time you send a message in a worktree. It uses Pi's own configuration and credentials, so a key added in Settings › Providers also works for Pi in a terminal, and the other way round.

Settings are saved in the platform config folder (`~/.config/treehouse/settings.json` on Linux). Pi sessions are saved under the platform data folder (`~/.local/share/treehouse/sessions`).

### Slash commands

Type `/` in the message box for Pi's skills (`/skill:name`) and prompt templates, plus `/compact [instructions]` and `/new`. ↑↓ choose, Tab or Enter picks, Esc closes. Skills and templates come from Pi's `get_commands`, so they are the ones Pi finds for that worktree; Pi expands them, and the chat shows the command as you typed it. `/compact` and `/new` run Pi's `compact` and `new_session` RPC commands.

Pi only loads a project's own skills, templates and extensions (`.pi/` and `.agents/skills`) once the project is trusted, and in RPC mode it can't ask. Until Treehouse handles project trust, record the decision by running `pi` once in the project and answering its trust prompt, or use `/trust` there.

### Without a model

`tools/fake-pi` speaks the same RPC protocol and plays back a scripted turn with tool calls, an approval and a sub-agent. Set it as the Pi command in Settings › Agent defaults to work on the UI without a model or an API key.

## How it fits together

| Path | What it does |
| --- | --- |
| `src/pi/rpc.rs` | Starts `pi --mode rpc`, writes commands and reads events as JSON lines |
| `src/pi/auth.rs` | Reads and writes provider keys in Pi's `auth.json` |
| `assets/pi/treehouse.ts` | The Pi extension Treehouse loads into every session (see below) |
| `src/transcript.rs` | Turns Pi's events into chat items: messages, thinking, tool calls, notes, errors |
| `src/session.rs` | One Pi session: its process, transcript, status, model and pending approvals |
| `src/slash.rs` | The slash menu's commands: matching, Pi's skills and templates, `/compact` and `/new` |
| `src/approval.rs` | Decides which tool calls need the user's approval |
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

This implements the screens of the Treehouse design: the worktree chat, sub-agents in the main screen and the right dock, the diff dock and its full-screen split view, the command palette, settings, the sidebar states, and the agent output for messages, thinking, read, edit, write, bash, approvals, sub-agent cards, notes and errors.

Not built yet, from the design's output boards: data tables and charts from tool output, Mermaid diagrams, maths, image thumbnails, plan cards, local server links, the "Open in terminal" dock tab, and a test button for provider keys.

## Licences

IBM Plex Sans and IBM Plex Mono are bundled under the SIL Open Font License (`assets/fonts/OFL.txt`). The colours are Catppuccin's (MIT).

## Development

```sh
cargo test
cargo clippy --all-targets
```
