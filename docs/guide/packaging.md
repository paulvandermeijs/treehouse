# Packaging

`cargo run` starts a bare executable, which shows the default icon. To get Treehouse's icon in the Dock, taskbar or launcher, build it as an app.

## macOS

```sh
cargo install cargo-bundle
cargo bundle --release
```

This builds `target/release/bundle/osx/Treehouse.app`. Move it to `/Applications` to start it from Finder or Spotlight.

## Linux

X11 windows get the icon directly. Wayland docks look it up through the desktop file:

```sh
cp assets/linux/treehouse.desktop ~/.local/share/applications/
mkdir -p ~/.local/share/icons/hicolor/256x256/apps
cp assets/icon/png/256x256.png ~/.local/share/icons/hicolor/256x256/apps/treehouse.png
```

`cargo bundle --release` also builds a `.deb` with both.

## Windows

`build.rs` embeds `treehouse.ico` in the executable, so `cargo build --release` is enough.

## The icon

The icon is `assets/icon/treehouse.svg`. After changing it, run:

```sh
cargo run --example render-icons
```

This renders it into `assets/icon/png/`, `treehouse.icns` (macOS, on Apple's icon grid) and `treehouse.ico` (Windows).
