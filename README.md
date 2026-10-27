# Treehouse

<picture>
  <source media="(prefers-color-scheme: dark)" srcset="docs/public/treehouse-announcement-mocha.gif">
  <img alt="Treehouse: Pi agents working in worktrees, with approvals, sub-agents and the diff" src="docs/public/treehouse-announcement-latte.gif">
</picture>

A desktop app for running [Pi](https://github.com/badlogic/pi-mono) coding agents in git worktrees. Each worktree of a project gets its own Pi chat; Pi can start sub-agents that you can open, steer and stop; and the right dock shows the worktree's diff or a second chat. Tool calls wait for your approval, and new projects stay read-only until you trust them.

Treehouse is written in Rust with [GPUI](https://www.gpui.rs) and [gpui-kit](https://gpui-kit.com/docs/). It drives Pi through Pi's [RPC mode](https://github.com/badlogic/pi-mono/blob/main/packages/coding-agent/docs/rpc.md): every chat is a `pi --mode rpc` process.

**[Read the manual](https://paulvandermeijs.github.io/treehouse/)** for how to use it.

## Installing

Download the app for macOS, Linux or Windows from the [latest release](https://github.com/paulvandermeijs/treehouse/releases/latest), and install Pi:

```sh
npm install -g @earendil-works/pi-coding-agent
```

To build it yourself, you need Rust 1.88 or later (edition 2024) and Pi:

```sh
npm install -g @earendil-works/pi-coding-agent
git clone https://github.com/paulvandermeijs/treehouse
cd treehouse
cargo run --release
```

On Linux, GPUI also needs the X11/Wayland and Vulkan libraries, for example on Debian or Ubuntu:

```sh
sudo apt install libxkbcommon-dev libxkbcommon-x11-dev libvulkan1 mesa-vulkan-drivers
```

To build an app bundle with Treehouse's icon (`cargo bundle --release`), see [Packaging](https://paulvandermeijs.github.io/treehouse/guide/packaging).

## Development

```sh
cargo test
cargo clippy --all-targets
cargo fmt
```

`tools/fake-pi` stands in for Pi with scripted turns, so you can work on the UI without a model or an API key. The [development docs](https://paulvandermeijs.github.io/treehouse/development/) cover the architecture and how to use it; `AGENTS.md` has notes for coding agents.

The manual is in [`docs/`](docs/), built with [VitePress](https://vitepress.dev) and deployed to GitHub Pages on every push to `master` that changes it. To preview it:

```sh
cd docs
npm install
npm run dev
```

## Licences

Treehouse is released under the [MIT License](LICENSE). IBM Plex Sans and IBM Plex Mono are bundled under the SIL Open Font License (`assets/fonts/OFL.txt`). The colours are Catppuccin's (MIT).
