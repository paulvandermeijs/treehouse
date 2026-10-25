//! The About Treehouse window, direction A of the design's About board:
//! a centred stack with the credits and links in a footer.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::Sizable as _;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::button::Button;
use gpui_kit::component::{h_flex, v_flex};
use gpui_kit::*;
use std::sync::Arc;

use crate::theme::{MONO, palette};

const REPOSITORY: &str = "https://github.com/paulvandermeijs/treehouse";

/// "0.1.0 (a3f9c21)", or just the version when the build had no git.
fn version() -> String {
    let commit = env!("TREEHOUSE_COMMIT");
    if commit.is_empty() {
        env!("CARGO_PKG_VERSION").to_string()
    } else {
        format!("{} ({commit})", env!("CARGO_PKG_VERSION"))
    }
}

/// What a bug report needs.
fn details() -> String {
    format!(
        "Treehouse {}\n{} · {}",
        version(),
        std::env::consts::OS,
        std::env::consts::ARCH
    )
}

struct About {
    logo: Arc<Image>,
    copied: bool,
}

impl Render for About {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let link = |id: &'static str, label: &'static str| {
            div()
                .id(id)
                .text_color(p.blue)
                .cursor_pointer()
                .hover(|s| s.underline())
                // A click on a link isn't a window drag.
                .on_mouse_down(MouseButton::Left, |_, window, _| window.prevent_default())
                .child(label)
        };
        super::drags_window(v_flex().id("about"))
            .size_full()
            .items_center()
            .bg(p.base)
            .text_color(p.text)
            .child(
                v_flex()
                    .flex_1()
                    .items_center()
                    .justify_center()
                    .px(px(28.))
                    .pt(px(20.))
                    .child(
                        div()
                            .size(px(96.))
                            .rounded(px(22.))
                            .shadow_lg()
                            .child(img(self.logo.clone()).size_full()),
                    )
                    .child(
                        div()
                            .mt(px(18.))
                            .text_size(px(22.))
                            .font_semibold()
                            .child("Treehouse"),
                    )
                    .child(
                        div()
                            .mt_1()
                            .font_family(MONO)
                            .text_size(px(12.5))
                            .text_color(p.subtext0)
                            .child(format!("Version {}", version())),
                    )
                    .child(
                        div()
                            .mt_3()
                            .max_w(px(240.))
                            .text_center()
                            .text_size(px(13.))
                            .text_color(p.subtext1)
                            .child("A desktop app for the Pi coding agent."),
                    )
                    .child(
                        div().mt(px(20.)).child(
                            Button::new("copy-details")
                                .outline()
                                .small()
                                .icon(super::icon(Lucide::Copy).size(px(14.)))
                                .label(if self.copied {
                                    "Copied"
                                } else {
                                    "Copy details"
                                })
                                .tooltip("Version and platform, for a bug report")
                                .on_click(cx.listener(|about, _, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(details()));
                                    about.copied = true;
                                    cx.notify();
                                })),
                        ),
                    ),
            )
            .child(
                h_flex()
                    .w_full()
                    .justify_center()
                    .flex_wrap()
                    .gap(px(14.))
                    .px_4()
                    .py_3()
                    .border_t_1()
                    .border_color(p.surface0)
                    .bg(p.mantle)
                    .text_size(px(11.5))
                    .text_color(p.subtext0)
                    .child("Designed by Paul van der Meijs")
                    .child(link("github", "GitHub").on_click(|_, _, cx| cx.open_url(REPOSITORY)))
                    .child(
                        link("licences", "Licences")
                            .on_click(|_, _, cx| cx.open_url(&format!("{REPOSITORY}#licences"))),
                    ),
            )
    }
}

/// Opens the About window, or brings the open one to the front.
pub fn open(cx: &mut App) {
    struct Open(Option<AnyWindowHandle>);
    impl Global for Open {}

    if let Some(Open(Some(handle))) = cx.try_global::<Open>()
        && handle
            .update(cx, |_, window, _| window.activate_window())
            .is_ok()
    {
        return;
    }
    let logo = Arc::new(Image::from_bytes(
        ImageFormat::Png,
        include_bytes!("../../assets/icon/png/256x256.png").to_vec(),
    ));
    let bounds = Bounds::centered(None, size(px(340.), px(370.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("About Treehouse".into()),
            ..super::titlebar()
        }),
        app_owns_titlebar_drag: cfg!(target_os = "macos"),
        is_resizable: false,
        is_minimizable: false,
        app_id: Some("treehouse".into()),
        ..Default::default()
    };
    let handle = gpui_kit::open_window(options, cx, |_, cx| {
        cx.new(|_| About {
            logo,
            copied: false,
        })
    })
    .ok()
    .map(|(handle, _)| handle);
    cx.set_global(Open(handle));
}
