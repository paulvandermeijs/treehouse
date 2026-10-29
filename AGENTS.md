# AGENTS.md

This file provides guidance to coding agents (Claude Code, Pi and others) when working with code in this repository.

Treehouse is a Rust desktop app (GPUI via [gpui-kit](https://gpui-kit.com/docs/) 0.8) that runs [Pi](https://github.com/badlogic/pi-mono) coding agents in git worktrees. Every chat is a `pi --mode rpc` child process spoken to in JSON lines. The default branch is `master`.

## Commands

```sh
cargo build                      # debug build; the binary is target/debug/treehouse
cargo run --release
cargo test                       # all tests (unit tests live next to the code)
cargo test closest_decision_wins # one test, by name substring
cargo clippy --all-targets       # keep it warning-free
cargo fmt
cargo run --example render-icons # after changing assets/icon/treehouse.svg
```

`cargo test` and `cargo clippy` don't rebuild `target/debug/treehouse`; run `cargo build` before launching the app to check a change.

Linux needs `libxkbcommon-dev libxkbcommon-x11-dev libvulkan1 mesa-vulkan-drivers` to link and run.

### Running without a model

`tools/fake-pi` is a Python stand-in for `pi --mode rpc` that plays scripted turns. Set it as the Pi command in Settings › Agent defaults (the `pi_command` key in `settings.json`). The first word of a message picks the script: `Find` (sub-agent), `Worktree` (asks for a new worktree with a task), `Fail` (failed edit, provider error), `Retry` (a retried connection error), `Approve` (long approval pattern), `Long` (long-running command), `Mangle` (garbled tool name), `MCP` (MCP tool calls), `Code` (code blocks), `Table` (a table); anything else plays a worktree turn with tools, an approval and a sub-agent. The lifecycle commands (`/commit`, `/sync`, `/finish-branch`) run real git in the worktree and call `treehouse_merge`; `/setup-worktrees` writes sample `.agents/` files. It also handles `/mcp login`, `!` shell commands (Pi's `bash` RPC, run for real), and `FAKE_PI_MCP=1` (an MCP server needing sign-in). It saves the chat to a session file in Pi's format (`--session-dir`, `--session`), so restoring history can be tried. Extend it when a feature needs a new Pi behaviour to test against.

For screenshots without a display, run the app under Xvfb and drive it with `xdotool`. Point `XDG_CONFIG_HOME`/`XDG_DATA_HOME` at a scratch folder for Treehouse's settings, and `PI_CODING_AGENT_DIR` at one for Pi's (`trust.json`, `auth.json`), so real settings aren't touched.

## Architecture

Data flows one way: **Pi process → `PiClient` → `Session` → `Transcript` items → `ChatView` render**.

- `src/pi/rpc.rs`: spawns Pi (`LaunchOptions::args` builds every flag), writes commands with an `id`, and reads stdout into `PiMessage::{Response, Event, Exited}`. `pi::rpc::command` has a constructor per RPC command.
- `src/session.rs`: one entity per chat (a worktree's chat or a sub-agent, `SessionKind`). It starts Pi lazily on the first message (`ensure_started`) and turns responses and events into transcript changes. It also owns pending approvals, extension UI requests (approvals, sub-agents, MCP sign-in) and restarts (`restart`, used for `/reload` and trust changes; it resumes the same `sessionFile`).
- `src/transcript.rs`: the chat's state as a list of `Item`s, built from Pi's events by `Transcript::apply`. Most behaviour is unit-tested here by feeding JSON events. `src/steps.rs` folds a turn's finished steps into a summary line for `ui/items.rs`.
- `src/history.rs`: the worktree → Pi session file index (`sessions.json`, with each worktree's sub-agents by tool call id; a new parent session clears them) and reading a session file's current branch. `Transcript::restore` builds the chat from its stored messages, since Pi doesn't replay events; `Session::restore_history` runs it when the chat is first opened or Pi starts.
- `src/workspace.rs`: projects → worktrees → sub-agent sessions. It creates and removes worktrees through `src/git.rs`, which shells out to `git`, and applies trust decisions to every session.
- `src/setup.rs`: a new worktree's `.agents/linked` symlinks and `.agents/prepare` script (with `ROOT_WORKTREE_PATH`). `Session::run_setup` runs them as a shell row in the chat (`Transcript::start_setup`), holds messages until it ends, and waits for trust.
- `src/ui/`: `app.rs` is the root (sidebar, main chat, right dock, dialogs); `chat.rs` is a chat pane, `items.rs` renders each transcript item, `diff.rs` the dock's diff, `model_picker.rs` the model picker (its list comes from `src/models.rs`), and `trust.rs`, `settings.rs` and `palette.rs` the dialogs.

### The bridge extension

Pi has no tool approval or sub-agents. `assets/pi/treehouse.ts` is a Pi extension that Treehouse writes to its data folder and loads with `-e`. It sends every tool call to Treehouse as an `extension_ui_request` input titled `treehouse:approval`, registers a `subagent` tool that asks Treehouse (title `treehouse:subagent`) to run another Session, a `treehouse_merge` tool (title `treehouse:merge`) that the workspace checks, asks the user about every time, and carries out, and a `treehouse_worktree` tool (title `treehouse:worktree`) that creates a worktree the way New worktree does and hands its chat a task. Its `/treehouse-save` command stores what the chat shows but Pi's messages don't hold (commit cards, `treehouse:commits`) as a custom entry in the session file, which `Transcript::restore` reads back. `src/approval.rs` decides allow or ask from the user's settings.

The worktree lifecycle commands (`/commit`, `/sync`, `/finish-branch`, and `/setup-worktrees`, which writes `.agents/linked` and `.agents/prepare`) run skills from `assets/pi/skills/`, written to the data folder and loaded with `--skill` in trusted worktree chats. They're named `treehouse-*` because Pi keeps the first skill of a name; `/commit` runs a project's own `commit` skill when Pi lists one.

### Chat history lives in Pi's session file

Everything the chat shows as history must come back from Pi's session file after a restart. Either Pi stores it itself (messages, tool results, `bashExecution`, `compaction`, `model_change`, `thinking_level_change`), or Treehouse saves it with the bridge's `/treehouse-save` as a custom `treehouse:*` entry, and `Transcript::restore` reads it back. Don't add a transcript item that neither covers.

- Live state isn't history: pending approvals, the merge card, "Sync stopped halfway" and the setup waiting for trust are drawn from the current state each time.
- Pi must be running to save. Queue a save until it starts rather than skipping it; only when there's no Pi to save to at all (Pi failed to start) may the chat show something unsaved.
- Pi keeps entries in memory and writes the file at the first assistant reply, with everything saved before it.

### Things that bite

- `--tools` replaces Pi's whole tool selection: name the bridge's `subagent`, `treehouse_merge` and `treehouse_worktree` tools, and `codemode` and `tool_search` (the only way MCP tools are reachable). Untrusted projects get `read` only, plus `--no-approve`, `--no-mcp` and a read-only note through `--append-system-prompt`.
- Pi lists a custom tool in its system prompt only when it has a `promptSnippet`. The bridge's tools carry one, plus `promptGuidelines` that tell the model to use them instead of the `git` commands a project's own instructions may describe. Models still follow those instructions, so the bridge also blocks a bash `git worktree add`.
- Project trust is stored in Pi's own `~/.pi/agent/trust.json`, shared with `pi` in a terminal (`src/trust.rs`, which takes Pi's `.json.lock` directory lock).
- Some Pi messages are only human-readable text; `src/mcp.rs` parses MCP sign-in notices by their exact wording. Check it against Pi's source when Pi changes.
- `/compact`, `/new` and `/reload` are handled in Treehouse (`src/slash.rs`, `Session::send`). Messages starting with `!`/`!!` go to Pi's `bash` RPC command, not to the model.
- Actions are registered at app level (`route::<A>` in `ui/app.rs`) so they work whatever has focus. Keys that must beat an input's own bindings use a key context such as `"SlashMenu > Input"`.
- A dialog opened while `TreehouseApp` is still being constructed panics; defer it (`window.defer`).
- In `#[cfg(test)]` modules, import items by name rather than `use super::*` when the parent glob-imports `gpui_kit::*`; the glob causes a recursion error.

## Versions and releases

Treehouse follows [semver](https://semver.org). Before pushing to `master`, bump `version` in `Cargo.toml`: the minor version for new features, the patch version for fixes (while it's 0.x, breaking changes bump the minor version too). Run `cargo build` so `Cargo.lock` follows. Changes that don't touch the app itself, such as docs, tests or CI, don't need a bump.

A push to `master` that changes the version publishes a GitHub release `vX.Y.Z` (`.github/workflows/release.yml`) with the app for macOS, Linux and Windows. The docs link to `releases/latest/download/<asset>`, so keep the asset names free of the version.

## Design and conventions

- The UI follows a Treehouse design canvas (Catppuccin Mocha/Latte, IBM Plex Sans and Mono). Colours come from `theme::Palette` (`src/theme.rs`); add a token there rather than hard-coding a colour. Icons beyond gpui-kit's defaults are added to `ExtraIcons` in `src/assets.rs`.
- Use gpui-kit components where they fit. Layouts the kit doesn't provide, such as the sidebar, are built by hand from `div`/`h_flex`/`v_flex` to the design.
- Follow existing standards (Pi's own files, `.agents/` conventions such as `.agents/prepare`, skills, `AGENTS.md`) rather than inventing Treehouse-specific files or formats; only add our own when no standard exists.
- `TODO.md` is the list of what's left to build, with open questions. Mark items done by removing them, and add new ones as they come up.
- Comments explain why, not what. User-facing text is short and plain.
- Keep the manual in `docs/` up to date with user-visible behaviour: `docs/guide/` documents each feature and `docs/development/` is for contributors. Check the site still builds with `npm run build` in `docs/`. `README.md` stays minimal (intro, installing, development, licences); change it only when those change.
