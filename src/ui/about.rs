//! The About Treehouse window.

use gpui_kit::component::{StyledExt as _, h_flex, v_flex};
use gpui_kit::*;
use std::sync::Arc;

use crate::theme::palette;

struct About {
    logo: Arc<Image>,
}

impl Render for About {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        v_flex()
            .size_full()
            .items_center()
            .justify_center()
            .gap_2()
            .bg(p.base)
            .text_color(p.text)
            .child(img(self.logo.clone()).size(px(96.)))
            .child(
                h_flex()
                    .mt_2()
                    .gap_2()
                    .items_baseline()
                    .child(div().text_size(px(20.)).font_semibold().child("Treehouse"))
                    .child(
                        div()
                            .text_size(px(12.5))
                            .text_color(p.subtext0)
                            .child(concat!("Version ", env!("CARGO_PKG_VERSION"))),
                    ),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .text_color(p.subtext0)
                    .child("Designed by Paul van der Meijs"),
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
    let bounds = Bounds::centered(None, size(px(340.), px(260.)), cx);
    let options = WindowOptions {
        window_bounds: Some(WindowBounds::Windowed(bounds)),
        titlebar: Some(TitlebarOptions {
            title: Some("About Treehouse".into()),
            ..Default::default()
        }),
        is_resizable: false,
        is_minimizable: false,
        app_id: Some("treehouse".into()),
        ..Default::default()
    };
    let handle = gpui_kit::open_window(options, cx, |_, cx| cx.new(|_| About { logo }))
        .ok()
        .map(|(handle, _)| handle);
    cx.set_global(Open(handle));
}
