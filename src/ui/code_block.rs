//! Fenced code blocks in Pi's replies, drawn to the design.
//!
//! The text view's own code block has no header row, so this Markdown plugin
//! takes over every fenced block: a header with the language and a copy
//! button over the highlighted code.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonGroup, ButtonVariants as _};
use gpui_kit::component::highlighter::SyntaxHighlighter;
use gpui_kit::component::text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast};
use gpui_kit::component::{
    ActiveTheme as _, IconName, Rope, Selectable as _, Sizable as _, h_flex, v_flex,
};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::hash::{Hash as _, Hasher as _};
use std::ops::Range;
use std::sync::{Arc, Mutex};

use super::icon;
use crate::theme::palette;

const NAME: &str = "treehouse-code";

struct Code {
    lang: Option<SharedString>,
    code: SharedString,
}

/// A ```mermaid block, drawn as its diagram.
struct Mermaid {
    code: SharedString,
    /// The closing fence has arrived; until then Pi is still writing it.
    closed: bool,
}

pub struct CodeBlocks;

impl MarkdownPlugin for CodeBlocks {
    fn is_block(&self) -> bool {
        true
    }

    fn name(&self) -> &str {
        NAME
    }

    fn parse(
        &self,
        node: &markdown_ast::Node,
        context: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let markdown_ast::Node::Code(code) = node else {
            return None;
        };
        let lang = code.lang.as_deref().and_then(fence_language);
        if lang.as_deref() == Some("mermaid") {
            let closed = code
                .position
                .as_ref()
                .and_then(|p| context.source().get(p.start.offset..p.end.offset))
                .is_none_or(crate::mermaid::fence_closed);
            return Some(
                MarkdownNode::new(
                    NAME,
                    Mermaid {
                        code: code.value.clone().into(),
                        closed,
                    },
                )
                .text(code.value.clone())
                .markdown(format!("```mermaid\n{}\n```", code.value)),
            );
        }
        let fence = format!(
            "```{}\n{}\n```",
            code.lang.as_deref().unwrap_or(""),
            code.value
        );
        Some(
            MarkdownNode::new(
                NAME,
                Code {
                    lang,
                    code: code.value.clone().into(),
                },
            )
            .text(code.value.clone())
            .markdown(fence),
        )
    }

    fn render(&self, node: &MarkdownNode, window: &mut Window, cx: &mut App) -> impl IntoElement {
        if let Some(mermaid) = node.data::<Mermaid>() {
            let id = node.source_range().map_or(0, |r| r.start);
            return render_mermaid(mermaid, id, window, cx);
        }
        let Some(code) = node.data::<Code>() else {
            return div().into_any_element();
        };
        let p = palette(cx);
        let id = node.source_range().map_or(0, |r| r.start);
        let highlights = code
            .lang
            .as_ref()
            .map(|lang| highlight(lang, &code.code, cx))
            .unwrap_or_default();
        let copied = code.code.clone();
        v_flex()
            .id(("code-block", id))
            .w_full()
            .min_w_0()
            .my_1()
            .rounded(px(6.))
            .border_1()
            .border_color(p.surface0)
            .bg(p.mantle)
            .overflow_hidden()
            .child(
                h_flex()
                    .h(px(30.))
                    .pl_3()
                    .pr_1()
                    .border_b_1()
                    .border_color(p.surface0)
                    .font_family(crate::theme::sans())
                    .text_size(px(12.))
                    .text_color(p.subtext0)
                    .child(div().flex_1().child(code.lang.clone().unwrap_or_default()))
                    .child(
                        Button::new(("copy-code", id))
                            .ghost()
                            .xsmall()
                            .icon(icon(Lucide::Copy).size(px(14.)))
                            .tooltip("Copy")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copied.to_string()))
                            }),
                    ),
            )
            .child(
                div()
                    .px_4()
                    .py(px(10.))
                    .font_family(crate::theme::mono())
                    .text_size(px(12.5))
                    .line_height(px(20.))
                    .text_color(p.text)
                    .when(!code.code.is_empty(), |this| {
                        this.child(
                            StyledText::new(code.code.clone())
                                .with_highlights(highlights.iter().cloned()),
                        )
                    }),
            )
            .into_any_element()
    }
}

fn render_mermaid(mermaid: &Mermaid, id: usize, window: &mut Window, cx: &mut App) -> AnyElement {
    let p = palette(cx);
    // Diagram or source, per block.
    let show_source = window.use_keyed_state(("mermaid-source", id), cx, |_, _| false);
    let source_on = *show_source.read(cx);
    let rendered = mermaid
        .closed
        .then(|| crate::mermaid::render(&mermaid.code, &p, window.scale_factor()));
    let failed = matches!(rendered, Some(Err(_)));
    let copied = mermaid.code.clone();
    let source = |error_line: Option<SharedString>| {
        v_flex()
            .when_some(error_line, |this, error| {
                this.child(
                    h_flex()
                        .gap_2()
                        .px_4()
                        .py(px(8.))
                        .border_b_1()
                        .border_color(p.surface0)
                        .font_family(crate::theme::sans())
                        .text_size(px(12.5))
                        .text_color(p.red)
                        .child(icon(IconName::Info).size(px(14.)))
                        .child(error),
                )
            })
            .child(
                div()
                    .px_4()
                    .py(px(10.))
                    .font_family(crate::theme::mono())
                    .text_size(px(12.5))
                    .line_height(px(20.))
                    .text_color(p.text)
                    .child(mermaid.code.clone()),
            )
    };
    let body = match rendered {
        // Still streaming: the diagram is drawn once the fence closes.
        None => h_flex()
            .gap_2()
            .px_4()
            .py(px(14.))
            .font_family(crate::theme::sans())
            .text_size(px(12.5))
            .text_color(p.subtext0)
            .child(div().size(px(6.)).rounded_full().bg(p.blue))
            .child("Rendering diagram…")
            .into_any_element(),
        Some(Err(error)) => {
            source(Some(format!("Couldn't render: {error}").into())).into_any_element()
        }
        Some(Ok(_)) if source_on => source(None).into_any_element(),
        // A diagram wider than the chat is scaled down to fit it, so none of
        // it is hidden behind a sideways scroll.
        Some(Ok(diagram)) => h_flex()
            .w_full()
            .justify_center()
            .p_4()
            .child(
                img(diagram.image.clone())
                    .w_full()
                    .max_w(px(diagram.width))
                    .aspect_ratio(diagram.width / diagram.height),
            )
            .into_any_element(),
    };
    v_flex()
        .id(("mermaid-block", id))
        .w_full()
        .min_w_0()
        .my_1()
        .rounded(px(6.))
        .border_1()
        .border_color(p.surface0)
        .bg(p.mantle)
        .overflow_hidden()
        .child(
            // Taller than a code block's header, as the design's diagram
            // cards are, to make room for the switch.
            h_flex()
                .h(px(38.))
                .gap(px(10.))
                .pl_3()
                .pr(px(6.))
                .border_b_1()
                .border_color(p.surface0)
                .font_family(crate::theme::sans())
                .text_size(px(13.))
                .text_color(p.text)
                .child(div().font_weight(FontWeight::MEDIUM).child("mermaid"))
                .when_some(crate::mermaid::kind(&mermaid.code), |this, kind| {
                    this.child(
                        div()
                            .text_size(px(12.))
                            .text_color(p.subtext0)
                            .child(SharedString::from(kind.to_string())),
                    )
                })
                .child(div().flex_1())
                .when(mermaid.closed && !failed, |this| {
                    let state = show_source.clone();
                    // Fixed sizes, as the design has them, rather than the
                    // kit's, which grow with the text size.
                    let segment = |id: &'static str, label: &'static str, on: bool| {
                        Button::new(id)
                            .child(div().text_size(px(12.)).child(label))
                            .selected(on)
                            .h(px(24.))
                            .px(px(9.))
                    };
                    this.child(
                        ButtonGroup::new(("mermaid-view", id))
                            .outline()
                            .small()
                            .mr(px(4.))
                            .child(segment("diagram", "Diagram", !source_on))
                            .child(segment("source", "Source", source_on))
                            .on_click(move |selected: &Vec<usize>, _, cx| {
                                let to_source = selected.first() == Some(&1);
                                state.update(cx, |s, cx| {
                                    *s = to_source;
                                    cx.notify();
                                })
                            }),
                    )
                })
                .child(
                    Button::new(("copy-mermaid", id))
                        .ghost()
                        .small()
                        .size(px(28.))
                        .icon(icon(Lucide::Copy).size(px(14.)))
                        .tooltip("Copy source")
                        .on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(copied.to_string()))
                        }),
                ),
        )
        .child(body)
        .into_any_element()
}

/// The language a fence names: "rust,ignore" and "ts {title}" name it first.
fn fence_language(info: &str) -> Option<SharedString> {
    info.split([',', ' ', '{'])
        .next()
        .filter(|l| !l.is_empty())
        .map(|l| SharedString::from(l.to_string()))
}

type Highlights = Arc<Vec<(Range<usize>, HighlightStyle)>>;

/// Highlights are worked out once per block and theme; a streaming block is
/// a new block at every update, so the cache is cleared when it grows large.
fn highlight(lang: &str, code: &str, cx: &App) -> Highlights {
    static CACHE: Mutex<Option<HashMap<u64, Highlights>>> = Mutex::new(None);
    let theme = cx.theme().highlight_theme.clone();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (lang, code, Arc::as_ptr(&theme) as usize).hash(&mut hasher);
    let key = hasher.finish();

    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(found) = cache.get(&key) {
        return found.clone();
    }
    let mut highlighter = SyntaxHighlighter::new(lang);
    highlighter.update(None, &Rope::from_str(code), None);
    let styles = Arc::new(highlighter.styles(&(0..code.len()), theme.as_ref()));
    if cache.len() > 256 {
        cache.clear();
    }
    cache.insert(key, styles.clone());
    styles
}

#[cfg(test)]
mod tests {
    use super::fence_language;

    #[test]
    fn reads_the_fence_language() {
        assert_eq!(fence_language("rust,ignore").as_deref(), Some("rust"));
        assert_eq!(fence_language("ts {title=\"a.ts\"}").as_deref(), Some("ts"));
        assert_eq!(fence_language(""), None);
    }
}
