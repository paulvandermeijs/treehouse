//! Fenced code blocks in Pi's replies, drawn to the design.
//!
//! The text view's own code block has no header row, so this Markdown plugin
//! takes over every fenced block: a header with the language and a copy
//! button over the highlighted code.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::highlighter::SyntaxHighlighter;
use gpui_kit::component::text::{MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast};
use gpui_kit::component::{ActiveTheme as _, Rope, Sizable as _, h_flex, v_flex};
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
        _: &MarkdownParseContext<'_>,
    ) -> Option<MarkdownNode> {
        let markdown_ast::Node::Code(code) = node else {
            return None;
        };
        let lang = code.lang.as_deref().and_then(fence_language);
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

    fn render(&self, node: &MarkdownNode, _: &mut Window, cx: &mut App) -> impl IntoElement {
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
