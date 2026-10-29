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

## Versions and releases

Treehouse follows [semver](https://semver.org): a change to the app bumps `version` in `Cargo.toml` before it's pushed to `master`, the minor version for features and the patch version for fixes. That push publishes a GitHub release `vX.Y.Z`, built by `.github/workflows/release.yml` for macOS (Apple silicon and Intel), Linux (`.deb` and AppImage) and Windows. The macOS app is signed with a Developer ID and notarized (`.github/scripts/sign-macos.sh`) using the repository secrets `MACOS_CERTIFICATE`, `MACOS_CERTIFICATE_PASSWORD`, `APPLE_API_KEY`, `APPLE_API_KEY_ID` and `APPLE_API_ISSUER`; without them it's only ad-hoc signed. Signing runs in a job of its own (`sign-macos`) that only downloads the unsigned app, so the build jobs, which run every crate's build script, never see the secrets; only the release job can write to the repository, and actions are pinned to commits. Run the workflow by hand from the Actions tab to retry a release that failed; it skips versions that are already released.

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
| `src/mermaid.rs` | Draws mermaid blocks with mermaid-rs-renderer, in the theme's colours |
| `src/models.rs` | The model picker's list: favorites, recent, provider and maker groups, filtering; and the model list for Settings |
| `src/transcript.rs` | Turns Pi's events into chat items: messages, thinking, tool calls, notes, errors |
| `src/steps.rs` | Folds a turn's finished steps into one summary line |
| `src/history.rs` | Which Pi session file holds each worktree's chat, and reading one back |
| `assets/pi/skills/` | The built-in skills behind `/commit`, `/sync`, `/finish-branch` and `/setup-worktrees`, loaded with `--skill` |
| `src/slash.rs` | The slash menu's commands: matching, Pi's skills and templates, `/compact`, `/new` and the lifecycle commands |
| `src/approval.rs` | Decides which tool calls need the user's approval |
| `src/trust.rs` | Project trust, read from and saved to Pi's `trust.json` |
| `src/mcp.rs` | Reads Pi's MCP sign-in messages |
| `src/workspace.rs` | Projects, worktrees and sub-agents; reports sub-agent results to their parent |
| `src/git.rs` | Worktree list/add/remove, the diff since the base branch, branch state, and the merge checks and merge |
| `src/setup.rs` | Preparing a new worktree from `.agents/linked` and `.agents/prepare` |
| `src/theme.rs`, `assets/themes/treehouse.json` | Catppuccin Mocha and Latte, with IBM Plex Sans and Mono |
| `src/ui/` | The window: sidebar, chat, tool output, diff dock, command palette, settings |

Most of the chat's behaviour is unit-tested in `src/transcript.rs` by feeding it Pi's JSON events.

## The bridge extension

Pi has no built-in tool approval or sub-agents. Treehouse adds both with a small extension, `assets/pi/treehouse.ts`, that it writes to its data folder and passes to Pi with `-e`:

- **Approval.** Every tool call is sent to Treehouse as an RPC extension UI request titled `treehouse:approval`. `src/approval.rs` applies the [approval settings](/guide/approvals), and when it has to ask, the chat shows the call with Run, Always allow and Deny.
- **Sub-agents.** The extension registers a `subagent` tool. When Pi calls it, the extension asks Treehouse (title `treehouse:subagent`) to start a separate Pi session in the same worktree, and returns that session's final answer as the tool result.
- **Merging.** The extension registers a `treehouse_merge` tool for worktrees with a base branch. When Pi calls it, the extension asks Treehouse (title `treehouse:merge`), which checks the merge is safe, asks the user, merges and answers with JSON: the text for the model, and what was merged, which the chat shows as a card. A refusal or denial fails the tool with the reason.
- **Worktrees.** The extension registers a `treehouse_worktree` tool with a branch name and an optional task. Treehouse (title `treehouse:worktree`) creates the worktree as New worktree does and hands the task to its chat, saved as a `treehouse:handover` entry so the task card comes back. A refusal fails the tool with the reason. The approval hook blocks a bash `git worktree add` while the tool is active, since models follow a project's own worktree instructions over the tool's description.
- **Saving.** Some of what the chat shows isn't in Pi's messages, such as the commits a turn made. Treehouse sends `/treehouse-save {customType, data}`, an extension command, and the extension stores it with `pi.appendEntry` as a `custom` entry in the session file, out of the model's context. `Transcript::restore` turns them back into chat items: commit cards, the worktree's setup row, Treehouse's notices and errors, and MCP sign-ins. Everything the chat shows as history has to come back this way or from Pi's own entries; see `AGENTS.md`.

## Things that bite

- `--tools` replaces Pi's whole tool selection: name the bridge's `subagent`, `treehouse_merge` and `treehouse_worktree` tools, and `codemode` and `tool_search` (the only way MCP tools are reachable). Untrusted projects get `read` only, plus `--no-approve`, `--no-mcp` and a read-only note through `--append-system-prompt`.
- Some Pi messages are only human-readable text; `src/mcp.rs` parses MCP sign-in notices by their exact wording. Check it against Pi's source when Pi changes.
- Pi keeps the first skill it finds with a name, and `--skill` paths load first, so the built-in skills are prefixed `treehouse-` to leave a project's own `commit` skill in charge. `/commit` picks the project's skill when `get_commands` lists one.
- Pi doesn't replay events for a resumed session, so `Transcript::restore` builds the chat from the session file's stored messages.

## gpui-kit components used

The sidebar is laid out by hand to the design with gpui-kit buttons and context menus; `Diff` for the worktree diff in unified and split layouts; `Command` in a `Dialog` for the command palette; `Settings` for the settings dialog; `TextView` for streaming Markdown; `ShimmerText` and `Spinner` for running work; `Textarea`, `Button`, `ButtonGroup`, `PopupMenu`, `Kbd` and alert dialogs throughout.

## Status

Treehouse implements the screens of the Treehouse design: the worktree chat, sub-agents in the main screen and the right dock, the diff dock and its full-screen split view, the command palette, settings, the sidebar states, the trust prompt and read-only projects, and the agent output for messages, thinking, read, edit, write, bash, approvals, sub-agent cards, notes and errors.

Not built yet, from the design's output boards: data tables (long tables and CSV) and charts from tool output, Mermaid diagrams, maths, image thumbnails, plan cards, local server links, the "Open in terminal" dock tab, and a test button for provider keys.

## Design

The UI follows the Treehouse design canvas: Catppuccin Mocha and Latte, IBM Plex Sans and Mono. Colours come from `theme::Palette` in `src/theme.rs`. Layouts the kit doesn't provide, such as the sidebar, are built by hand from gpui-kit primitives.

## Licences

Treehouse is released under the [MIT License](https://github.com/paulvandermeijs/treehouse/blob/master/LICENSE). IBM Plex Sans and IBM Plex Mono are bundled under the SIL Open Font License (`assets/fonts/OFL.txt`). The colours are Catppuccin's (MIT).
