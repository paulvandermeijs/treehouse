# Settings

Open Settings with <kbd>⌘,</kbd> (<kbd>Ctrl+,</kbd>), the gear at the bottom of the sidebar, or the command palette. Changes apply right away and are saved to `settings.json` in the platform config folder (`~/.config/treehouse/settings.json` on Linux).

## Providers

API keys for Anthropic, OpenAI, Google and OpenRouter. Paste a key and press <kbd>Enter</kbd>; it replaces the stored key.

Keys are saved in Pi's own `auth.json`, so a key added here also works for `pi` in a terminal. Each provider shows where its key comes from: saved in Pi's `auth.json`, signed in with Pi (OAuth), read from the environment (such as `$ANTHROPIC_API_KEY`), or not set up.

## Agent defaults

Used for new worktrees and sub-agents. You can still change the model and thinking level per chat in the toolbar under the message box.

| Setting | Key | Default | |
| --- | --- | --- | --- |
| Model | `model` | Pi's default | `provider/model`, for example `anthropic/claude-opus-4-5` |
| Sub-agent model | `subagent_model` | Same as the chat | Cheaper models are often enough |
| Thinking level | `thinking` | `high` | `off`, `minimal`, `low`, `medium` or `high` |
| Approval | `approval` | Ask before bash | See [Approvals](./approvals) |
| Pi command | `pi_command` | `pi` | The `pi` executable |
| Tools | `tools` | read, bash, edit, write | What Pi may use |

## Approvals

| Setting | Key | Default |
| --- | --- | --- |
| Always allowed patterns | `allowed` | none |
| Always ask before deleting files | `always_ask.deleting` | on |
| Always ask before pushing | `always_ask.pushing` | on |
| Always ask before touching files outside the worktree | `always_ask.outside_worktree` | on |

See [Approvals](./approvals) for what each one covers.

## Worktrees

| Setting | Key | Default | |
| --- | --- | --- | --- |
| Location | `worktree_location` | `{project}/.worktrees/{branch}` | Where new worktrees go |
| Compare against | `compare_base` | The branch it was created from | Or `origin/main`; the base for the diff and `+/−` counts |
| Setup command | `setup_command` | none | Runs once after a worktree is created, for example `npm install` |
| Also delete its folder and branch | `delete_on_remove` | off | When removing a worktree |

The page also shows the commit author, read from `git config user.name`.

## Appearance

| Setting | Key | Default |
| --- | --- | --- |
| Theme | `theme` | Mocha (`mocha`, `latte` or `system`) |
| Text size | `text_size` | 14 (13 to 16) |
| Keep diffs expanded after a turn | `keep_diffs_expanded` | off |
| Always expand thinking | `always_expand_thinking` | off |

**Toggle Theme** in the View menu or command palette switches between Mocha and Latte. **Match system** follows the system's light or dark setting.

## Environment variables

| Variable | Effect |
| --- | --- |
| `TREEHOUSE_NO_SHELL_ENV=1` | Don't read `PATH` from your login shell at startup |
| `XDG_CONFIG_HOME`, `XDG_DATA_HOME` | Where Treehouse keeps its settings and sessions on Linux |
| `PI_CODING_AGENT_DIR` | Pi's own folder (`trust.json`, `auth.json`, `mcp.json`) instead of `~/.pi/agent` |
