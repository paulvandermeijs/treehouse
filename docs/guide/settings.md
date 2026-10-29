# Settings

Open Settings with <kbd>⌘,</kbd> (<kbd>Ctrl+,</kbd>), the gear at the bottom of the sidebar, or the command palette. Changes apply right away and are saved to `settings.json` in the platform config folder (`~/.config/treehouse/settings.json` on Linux).

## Providers

API keys for Anthropic, OpenAI, Google and OpenRouter, one row per provider showing where its key comes from: saved in Pi's own `auth.json`, signed in with Pi (OAuth), read from the environment (such as `$ANTHROPIC_API_KEY`), or not set up.

A provider without a key has a field in its row: paste the key and press <kbd>Enter</kbd> or **Save**. Nothing is saved when you leave the field. The key shows as dots; the eye button shows it while you check it. A saved key shows masked, with **Replace** and **Remove**; **Add key** on a provider read from the environment saves a key that's used instead of the variable. **Sign out** asks first, since signing in again needs `pi` in a terminal.

Keys are saved in Pi's own `auth.json`, readable only by you, so a key added here also works for `pi` in a terminal.

## Agent defaults

Used for new worktrees and sub-agents. You can still change the model and thinking level per chat in the toolbar under the message box.

The model fields are the chat's model picker as a dropdown, with **Pi's default** or **Same as the chat** as the first option. Opening one asks Pi for its models. When a saved model's provider has no key any more, the field says so, since Pi then ignores the setting.

| Setting | Key | Default | |
| --- | --- | --- | --- |
| Model | `model` | Pi's default | `provider/model`, for example `anthropic/claude-opus-4-5` |
| Sub-agent model | `subagent_model` | Same as the chat | Cheaper models are often enough |
| | `favorite_models` | none | Starred in the model picker |
| | `recent_models` | none | Picked in a chat, most recent first |
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
| Also delete its folder and branch | `delete_on_remove` | off | When removing a worktree |

The page also shows the commit author, read from `git config user.name`. Preparing new worktrees is up to the project itself; see [Preparing new worktrees](./worktrees#preparing-new-worktrees).

## Appearance

| Setting | Key | Default |
| --- | --- | --- |
| Theme | `theme` | Mocha (`mocha`, `latte` or `system`) |
| Text size | `text_size` | 14 (13 to 16) |
| Interface font | `ui_font` | IBM Plex Sans |
| Code font | `code_font` | IBM Plex Mono |
| Keep diffs expanded after a turn | `keep_diffs_expanded` | off |
| Always expand thinking | `always_expand_thinking` | off |
| Collapse finished steps | `collapse_steps` | on |

The font fields take the name of a font family you have installed, as Font Book, `fc-list` or Windows' Fonts settings list it. A font applies as soon as its name matches; while it doesn't, Treehouse keeps the font it was using. A name that only differs in case or spaces offers the installed spelling. Leave a field empty for IBM Plex, which comes with Treehouse. The code font is used for code, paths, commands, diffs and model ids.

**Toggle Theme** in the View menu or command palette switches between Mocha and Latte. **Match system** follows the system's light or dark setting.

## Environment variables

| Variable | Effect |
| --- | --- |
| `TREEHOUSE_NO_SHELL_ENV=1` | Don't read `PATH` from your login shell at startup |
| `XDG_CONFIG_HOME`, `XDG_DATA_HOME` | Where Treehouse keeps its settings and sessions on Linux |
| `PI_CODING_AGENT_DIR` | Pi's own folder (`trust.json`, `auth.json`, `mcp.json`) instead of `~/.pi/agent` |
