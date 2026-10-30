//! Mermaid diagrams in Pi's replies, drawn with mermaid-rs-renderer: pure
//! Rust, no browser. The SVG it makes is shown as an image.

use std::collections::HashMap;
use std::hash::{Hash as _, Hasher as _};
use std::sync::{Arc, Mutex};

use gpui_kit::{Hsla, Image, ImageFormat, Rgba};
use mermaid_rs_renderer::{RenderOptions, Theme, render_strict};

use crate::theme::Palette;

/// A rendered diagram, or why it couldn't be.
pub type Rendered = Result<Diagram, String>;

#[derive(Clone)]
pub struct Diagram {
    pub image: Arc<Image>,
    /// The diagram's own size, in pixels at scale 1.
    pub width: f32,
    pub height: f32,
}

/// The diagram type a source starts with: `flowchart`, `sequenceDiagram`…
pub fn kind(source: &str) -> Option<&str> {
    source
        .lines()
        .map(str::trim)
        .find(|l| !l.is_empty() && !l.starts_with("%%") && *l != "---")
        .and_then(|l| l.split_whitespace().next())
}

/// Whether a fenced block's source, as written, ends with its closing fence:
/// while Pi streams the block it doesn't yet, and the diagram is incomplete.
pub fn fence_closed(block: &str) -> bool {
    let mut lines = block.trim_end().lines();
    let Some(open) = lines.next() else {
        return false;
    };
    let open = open.trim_start();
    let marker = if open.starts_with("~~~") {
        "~~~"
    } else {
        "```"
    };
    lines
        .last()
        .is_some_and(|last| last.trim().starts_with(marker))
}

/// Renders a diagram in the palette's colours on `background`, once per
/// source, colours and display scale.
pub fn render(source: &str, p: &Palette, background: Hsla, scale: f32) -> Rendered {
    static CACHE: Mutex<Option<HashMap<u64, Rendered>>> = Mutex::new(None);
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    (source, hex(p.text), hex(background), scale.to_bits()).hash(&mut hasher);
    let key = hasher.finish();
    let mut cache = CACHE.lock().unwrap_or_else(|e| e.into_inner());
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(rendered) = cache.get(&key) {
        return rendered.clone();
    }
    // The renderer is young; a panic in it mustn't take the app down.
    let options = RenderOptions {
        theme: theme(p, background),
        ..RenderOptions::default()
    };
    let rendered = match std::panic::catch_unwind(|| render_strict(source, options)) {
        Ok(Ok(svg)) => scaled(&svg, scale)
            .map(|(svg, width, height)| Diagram {
                image: Arc::new(Image::from_bytes(ImageFormat::Svg, svg.into_bytes())),
                width,
                height,
            })
            .ok_or_else(|| "the diagram has no size".to_string()),
        Ok(Err(err)) => Err(err.to_string()),
        Err(_) => Err("the renderer failed on this diagram".to_string()),
    };
    if cache.len() > 64 {
        cache.clear();
    }
    cache.insert(key, rendered.clone());
    rendered
}

/// The SVG with its size multiplied by the display scale, so GPUI, which
/// rasterises SVG images at their own size, draws it sharp; and that size.
fn scaled(svg: &str, scale: f32) -> Option<(String, f32, f32)> {
    let end = svg.find("<svg")? + svg[svg.find("<svg")?..].find('>')?;
    let tag = &svg[..end];
    let attr = |name: &str| -> Option<(usize, usize, f32)> {
        let start = tag.find(&format!(" {name}=\""))? + name.len() + 3;
        let len = tag[start..].find('"')?;
        let value = tag[start..start + len]
            .trim_end_matches("px")
            .parse()
            .ok()?;
        Some((start, start + len, value))
    };
    let (w_start, w_end, width) = attr("width")?;
    let (h_start, h_end, height) = attr("height")?;
    let mut out = String::with_capacity(svg.len() + 16);
    // Width and height can come in either order.
    let mut edits = [
        (w_start, w_end, width * scale),
        (h_start, h_end, height * scale),
    ];
    edits.sort_by_key(|e| e.0);
    let mut at = 0;
    for (start, end, value) in edits {
        out.push_str(&svg[at..start]);
        out.push_str(&format!("{value}"));
        at = end;
    }
    out.push_str(&svg[at..]);
    (width > 0. && height > 0.).then_some((out, width, height))
}

fn hex(color: Hsla) -> String {
    let c = Rgba::from(color);
    let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
    format!("#{:02x}{:02x}{:02x}", byte(c.r), byte(c.g), byte(c.b))
}

/// Catppuccin, as the design has it: nodes in Surface 0, borders in
/// Lavender, lines in Subtext 0.
fn theme(p: &Palette, background: Hsla) -> Theme {
    let mut theme = Theme::modern();
    theme.font_family = format!("{}, sans-serif", crate::theme::sans());
    theme.primary_color = hex(p.surface0);
    theme.primary_text_color = hex(p.text);
    theme.primary_border_color = hex(p.lavender);
    theme.line_color = hex(p.subtext0);
    theme.secondary_color = hex(p.surface1);
    theme.tertiary_color = hex(p.mantle);
    theme.edge_label_background = hex(background);
    theme.cluster_background = hex(background);
    theme.cluster_border = hex(p.surface1);
    // What it's drawn on, so the diagram has no box of its own.
    theme.background = hex(background);
    theme.sequence_actor_fill = hex(p.surface0);
    theme.sequence_actor_border = hex(p.lavender);
    theme.sequence_actor_line = hex(p.subtext0);
    theme.sequence_note_fill = hex(p.surface1);
    theme.sequence_note_border = hex(p.yellow);
    theme.sequence_activation_fill = hex(p.surface1);
    theme.sequence_activation_border = hex(p.lavender);
    theme.text_color = hex(p.text);
    theme.pie_title_text_color = hex(p.text);
    theme.pie_section_text_color = hex(p.base);
    theme.pie_legend_text_color = hex(p.text);
    theme.pie_stroke_color = hex(p.base);
    theme.pie_outer_stroke_color = hex(p.surface1);
    theme
}

#[cfg(test)]
mod tests {
    use super::{fence_closed, kind, scaled};

    #[test]
    fn finds_the_diagram_kind() {
        assert_eq!(
            kind("\n%% a comment\nflowchart LR\n  A --> B"),
            Some("flowchart")
        );
        assert_eq!(
            kind("sequenceDiagram\n  A->>B: hi"),
            Some("sequenceDiagram")
        );
        assert_eq!(kind("  "), None);
    }

    #[test]
    fn a_fence_is_closed_once_its_last_line_closes_it() {
        assert!(fence_closed("```mermaid\nflowchart LR\n  A --> B\n```\n"));
        assert!(fence_closed("~~~mermaid\nA\n~~~"));
        assert!(!fence_closed("```mermaid\nflowchart LR\n  A --> B"));
        assert!(!fence_closed("```mermaid"));
    }

    #[test]
    fn scales_the_svg_size_and_keeps_the_view_box() {
        let svg = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 100 50" height="50" width="100"><rect/></svg>"#;
        let (out, w, h) = scaled(svg, 2.).unwrap();
        assert_eq!((w, h), (100., 50.));
        assert!(out.contains(r#"height="100""#));
        assert!(out.contains(r#"width="200""#));
        assert!(out.contains(r#"viewBox="0 0 100 50""#));
    }
}
