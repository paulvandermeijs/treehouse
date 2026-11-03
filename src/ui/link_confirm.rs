//! The dialog a link in the chat asks with before it opens: a file on this
//! computer, another app's link, or a web link whose text names another site.
//!
//! It always shows the full target, since the link's text can hide it, and
//! Cancel is the default: Enter and Esc both close it, so opening takes a
//! click on Open.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{ActiveTheme as _, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::ops::Range;
use std::path::PathBuf;

use super::icon;
use super::trust::display_path;
use crate::links::{Ask, is_executable, program_extension, split_host};
use crate::theme::palette;

/// Text with one part picked out, such as the file name in a path.
struct Marked {
    text: String,
    marked: Option<Range<usize>>,
}

impl Marked {
    fn plain(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            marked: None,
        }
    }

    /// `before`, then `marked`, then `after`.
    fn parts(before: &str, marked: &str, after: &str) -> Self {
        Self {
            text: format!("{before}{marked}{after}"),
            marked: Some(before.len()..before.len() + marked.len()),
        }
    }

    fn render(&self, mark: HighlightStyle) -> StyledText {
        let text = StyledText::new(SharedString::from(self.text.clone()));
        match &self.marked {
            Some(range) => text.with_highlights([(range.clone(), mark)]),
            None => text,
        }
    }
}

/// What the dialog says for one kind of link.
struct Content {
    title: &'static str,
    intro: Marked,
    target: Marked,
    warning: Option<String>,
    open_label: &'static str,
    url: String,
    reveal: Option<PathBuf>,
}

fn content(ask: Ask) -> Content {
    match ask {
        Ask::File { url, path } => {
            let shown = display_path(&path);
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().to_string())
                .unwrap_or_default();
            let target = match shown.strip_suffix(&name) {
                Some(dir) if !name.is_empty() => Marked::parts(dir, &name, ""),
                _ => Marked::plain(shown.clone()),
            };
            let warning = match program_extension(&path) {
                Some(ext) => Some(format!("Opening a .{ext} file runs it.")),
                None if is_executable(&path) => Some("Opening this file runs it.".into()),
                None => None,
            };
            Content {
                title: "Open this file?",
                intro: Marked::plain("Pi's reply links to a file on your computer:"),
                target,
                warning,
                open_label: "Open",
                url,
                reveal: Some(path),
            }
        }
        Ask::App { url, scheme } => Content {
            title: "Open this link in another app?",
            intro: Marked::parts(
                "Pi's reply links to ",
                &format!("{scheme}:"),
                ", which opens whatever app handles it:",
            ),
            target: Marked::plain(url.clone()),
            warning: None,
            open_label: "Open",
            url,
            reveal: None,
        },
        Ask::Elsewhere { url, named } => Content {
            title: "This link goes somewhere else",
            intro: Marked::parts("The link says ", &named, ", but it opens:"),
            target: match split_host(&url) {
                Some((before, host, after)) => Marked::parts(&before, &host, &after),
                None => Marked::plain(url.clone()),
            },
            warning: None,
            open_label: "Open anyway",
            url,
            reveal: None,
        },
    }
}

/// Asks before opening a link, showing where it goes.
pub fn open(ask: Ask, window: &mut Window, cx: &mut App) {
    let content = std::rc::Rc::new(content(ask));
    window.open_dialog(cx, move |dialog, _, cx| {
        let p = palette(cx);
        let radius = cx.theme().radius_lg;
        let content = content.clone();
        dialog
            .w(px(460.))
            .margin_top(px(150.))
            .p_0()
            .close_button(false)
            // Enter closes it like Cancel: opening takes a click.
            .on_ok(|_, _, _| true)
            .content(move |body, _, _| {
                let c = content.clone();
                let emphasis = HighlightStyle {
                    color: Some(p.text),
                    font_weight: Some(FontWeight::MEDIUM),
                    ..Default::default()
                };
                let highlight = HighlightStyle {
                    color: Some(p.yellow_text),
                    font_weight: Some(FontWeight::MEDIUM),
                    ..Default::default()
                };
                let open_url = c.url.clone();
                body.child(
                    v_flex()
                        .text_size(px(13.5))
                        .child(
                            h_flex()
                                .gap_2()
                                .px(px(18.))
                                .pt(px(16.))
                                .pb(px(6.))
                                .text_size(px(15.))
                                .font_semibold()
                                .child(icon(Lucide::TriangleAlert).text_color(p.yellow))
                                .child(c.title),
                        )
                        .child(
                            v_flex()
                                .gap(px(10.))
                                .px(px(18.))
                                .pb(px(14.))
                                .text_color(p.subtext1)
                                .child(div().child(c.intro.render(emphasis)))
                                .child(
                                    div()
                                        .px(px(10.))
                                        .py_2()
                                        .rounded(px(6.))
                                        .bg(p.mantle)
                                        .border_1()
                                        .border_color(p.surface0)
                                        .font_family(crate::theme::mono())
                                        .text_size(px(12.5))
                                        .text_color(p.text)
                                        .child(c.target.render(highlight)),
                                )
                                .when_some(c.warning.clone(), |this, warning| {
                                    this.child(
                                        h_flex()
                                            .gap(px(6.))
                                            .text_size(px(12.5))
                                            .text_color(p.yellow_text)
                                            .child(icon(Lucide::CircleAlert).size(px(14.)))
                                            .child(warning),
                                    )
                                }),
                        )
                        .child(
                            h_flex()
                                .justify_end()
                                .gap_2()
                                .px(px(18.))
                                .py(px(12.))
                                .border_t_1()
                                .border_color(p.surface0)
                                .bg(p.mantle)
                                .rounded_b(radius)
                                .when_some(c.reveal.clone(), |this, path| {
                                    this.child(
                                        Button::new("link-reveal")
                                            .outline()
                                            .small()
                                            .label(if cfg!(target_os = "macos") {
                                                "Show in Finder"
                                            } else {
                                                "Show in folder"
                                            })
                                            .on_click(move |_, window, cx| {
                                                cx.reveal_path(&path);
                                                window.close_dialog(cx);
                                            }),
                                    )
                                })
                                .child(
                                    Button::new("link-open")
                                        .outline()
                                        .small()
                                        .label(c.open_label)
                                        .on_click(move |_, window, cx| {
                                            cx.open_url(&open_url);
                                            window.close_dialog(cx);
                                        }),
                                )
                                .child(
                                    Button::new("link-cancel")
                                        .primary()
                                        .small()
                                        .label("Cancel  esc")
                                        .on_click(|_, window, cx| window.close_dialog(cx)),
                                ),
                        ),
                )
            })
    });
}
