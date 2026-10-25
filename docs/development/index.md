# Architecture

Treehouse is written in Rust with [GPUI](https://www.gpui.rs) and [gpui-kit](https://gpui-kit.com/docs/). It drives Pi through Pi's [RPC mode](https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/rpc.md): every chat is a `pi --mode rpc` child process, spoken to in JSON lines.

## Building and testing

```sh
cargo build                      # debug build; the binary is target/debug/treehouse
cargo run --release
cargo test                       # unit tests live next to the code
cargo clippy --all-targets       # keep it warning-free
cargo fmt
```

`cargo test` and `cargo clippy` don't rebuild `target/debug/treehouse`; run `cargo build` before launching the app to check a change.

## How data flows

Data flows one way:

```text
Pi process → PiClient → Session → Transcript items → ChatView
```

| Path | What it does |
| --- | --- |
| `src/pi/rpc.rs` | Starts `pi --mode rpc`, writes commands and reads events as JSON lines |
| `src/pi/auth.rs` | Reads and writes provider keys in Pi's `auth.json` |
| `assets/pi/treehouse.ts` | The Pi extension Treehouse loads into every session (see below) |
| `src/session.rs` | One Pi session: its process, transcript, status, model and pending approvals |
| `src/transcript.rs` | Turns Pi's events into chat items: messages, thinking, tool calls, notes, errors |
| `src/history.rs` | Which Pi session file holds each worktree's chat, and reading one back |
| `src/slash.rs` | The slash menu's commands: matching, Pi's skills and templates, `/compact` and `/new` |
| `src/approval.rs` | Decides which tool calls need the user's approval |
| `src/trust.rs` | Project trust, read from and saved to Pi's `trust.json` |
| `src/mcp.rs` | Reads Pi's MCP sign-in messages |
| `src/workspace.rs` | Projects, worktrees and sub-agents; reports sub-agent results to their parent |
| `src/git.rs` | Worktree list/add/remove and the diff since the base branch |
| `src/theme.rs`, `assets/themes/treehouse.json` | Catppuccin Mocha and Latte, with IBM Plex Sans and Mono |
| `src/ui/` | The window: sidebar, chat, tool output, diff dock, command palette, settings |

Most of the chat's behaviour is unit-tested in `src/transcript.rs` by feeding it Pi's JSON events.

## The bridge extension

Pi has no built-in tool approval or sub-agents. Treehouse adds both with a small extension, `assets/pi/treehouse.ts`, that it writes to its data folder and passes to Pi with `-e`:

- **Approval.** Every tool call is sent to Treehouse as an RPC extension UI request titled `treehouse:approval`. `src/approval.rs` applies the [approval settings](/guide/approvals), and when it has to ask, the chat shows the call with Run, Always allow and Deny.
- **Sub-agents.** The extension registers a `subagent` tool. When Pi calls it, the extension asks Treehouse (title `treehouse:subagent`) to start a separate Pi session in the same worktree, and returns that session's final answer as the tool result.

## Things that bite

- `--tools` replaces Pi's whole tool selection: name the bridge's `subagent` tool, and `codemode` and `tool_search` (the only way MCP tools are reachable). Untrusted projects get `read` only, plus `--no-approve`, `--no-mcp` and a read-only note through `--append-system-prompt`.
- Some Pi messages are only human-readable text; `src/mcp.rs` parses MCP sign-in notices by their exact wording. Check it against Pi's source when Pi changes.
- Pi doesn't replay events for a resumed session, so `Transcript::restore` builds the chat from the session file's stored messages.

## Design

The UI follows the Treehouse design canvas: Catppuccin Mocha and Latte, IBM Plex Sans and Mono. Colours come from `theme::Palette` in `src/theme.rs`. Layouts the kit doesn't provide, such as the sidebar, are built by hand from gpui-kit primitives.

## Licences

IBM Plex Sans and IBM Plex Mono are bundled under the SIL Open Font License (`assets/fonts/OFL.txt`). The colours are Catppuccin's (MIT).
