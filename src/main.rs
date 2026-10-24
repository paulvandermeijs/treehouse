//! Treehouse: a desktop app for running Pi agents in git worktrees.

mod actions;
mod approval;
mod assets;
mod config;
mod git;
mod pi;
mod session;
mod theme;
mod transcript;
mod ui;
mod workspace;

use gpui_kit::{AppContext as _, Bounds, Size, TitlebarOptions, WindowBounds, WindowOptions, px};

fn main() {
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
                    ..Default::default()
                }),
                window_min_size: Some(Size::new(px(720.), px(480.))),
                ..Default::default()
            };
            gpui_kit::open_window(options, cx, |window, cx| {
                cx.new(|cx| ui::TreehouseApp::new(window, cx))
            })
            .expect("failed to open the Treehouse window");
            cx.activate(true);
        });
}
