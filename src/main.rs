//! Treehouse: a desktop app for running Pi agents in git worktrees.

mod actions;
mod approval;
mod assets;
mod config;
mod git;
mod markdown;
mod mcp;
mod pi;
mod session;
mod shell_env;
mod slash;
mod theme;
mod transcript;
mod trust;
mod ui;
mod workspace;

use gpui_kit::{AppContext as _, Bounds, Size, TitlebarOptions, WindowBounds, WindowOptions, px};

/// The window icon. Only X11 reads it; macOS takes the icon from the app
/// bundle, Windows from the executable, Wayland from the desktop file.
fn window_icon() -> Option<std::sync::Arc<image::RgbaImage>> {
    let png = include_bytes!("../assets/icon/png/256x256.png");
    let image = image::load_from_memory_with_format(png, image::ImageFormat::Png).ok()?;
    Some(std::sync::Arc::new(image.to_rgba8()))
}

fn main() {
    shell_env::load_login_path();
    gpui_kit::application()
        .with_assets(assets::Assets)
        .run(|cx| {
            gpui_kit::init(cx);
            cx.set_global(config::Config::load());
            theme::init(cx);
            actions::init(cx);

            let bounds = Bounds::centered(None, Size::new(px(1440.), px(900.)), cx);
            let options = WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                titlebar: Some(TitlebarOptions {
                    title: Some("Treehouse".into()),
                    ..ui::titlebar()
                }),
                app_owns_titlebar_drag: cfg!(target_os = "macos"),
                window_min_size: Some(Size::new(px(720.), px(480.))),
                // Matches `assets/linux/treehouse.desktop`, so Wayland and X11
                // desktops show the installed icon and group the windows.
                app_id: Some("treehouse".into()),
                icon: window_icon(),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| ui::TreehouseApp::new(window, cx))
            })
            .expect("failed to open the Treehouse window");
            cx.activate(true);
        });
}
