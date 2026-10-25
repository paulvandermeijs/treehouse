//! How each piece of agent output looks in the chat.
//!
//! One text column: the user's messages carry an avatar, Pi's don't. Icons sit
//! in a 34px gutter, so text, cards and code blocks start on the same line.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::Input;
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::shimmer::ShimmerText;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::{TextView, TextViewStyle};
use gpui_kit::component::{IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::Value;

use super::chat::ChatView;
use super::icon;
use crate::session::{AgentStatus, ApprovalChoice, PendingApproval, Session};
use crate::theme::{MONO, Palette, mix, palette};
use crate::transcript::{
    Item, ItemId, ShellRun, SignIn, SignInState, ToolCall, ToolStatus, format_duration,
};

pub const GUTTER: f32 = 34.;

/// Renders the transcript, grouping three or more finished reads in a row.
pub fn render_items(view: &ChatView, cx: &mut Context<ChatView>) -> Vec<AnyElement> {
    let session = view.session.clone();
    let mut items: Vec<Item> = session.read(cx).items().to_vec();
    // A sub-agent's first message is its task, which the task card shows.
    if let crate::session::SessionKind::SubAgent { task, .. } = &session.read(cx).kind
        && matches!(items.first(), Some(Item::User { text, .. }) if text.trim() == task.trim())
    {
        items.remove(0);
    }
    let mut out = Vec::new();
    let mut ix = 0;
    while ix < items.len() {
        if let Item::Tool(tool) = &items[ix]
            && tool.name == "read"
            && tool.status == ToolStatus::Done
        {
            let run: Vec<&ToolCall> = items[ix..]
                .iter()
                .map_while(|item| match item {
                    Item::Tool(t) if t.name == "read" && t.status == ToolStatus::Done => Some(t),
                    _ => None,
                })
                .collect();
            if run.len() >= 3 {
                out.push(render_read_group(view, &run, cx));
                ix += run.len();
                continue;
            }
        }
        out.push(render_item(view, &items[ix], cx));
        ix += 1;
    }
    out
}

fn render_item(view: &ChatView, item: &Item, cx: &mut Context<ChatView>) -> AnyElement {
    let p = palette(cx);
    match item {
        Item::User { id, text } => h_flex()
            .id(("user", id.0))
            .mt_2()
            .gap_3()
            .items_start()
            // Centre the avatar on the first line, which grows with the text
            // size (GPUI's default line height is 1.618 × the text size).
            .child(avatar(&view.initials, &p).mt(first_line_offset(cx)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(SharedString::from(text.clone())),
            )
            .into_any_element(),
        Item::Text { id, text, .. } => {
            let body = match view.session.read(cx).text_view(*id) {
                Some(state) => markdown_style(TextView::new(state).stream_fade(true), cx),
                None => markdown_style(
                    TextView::markdown(
                        ("text", id.0),
                        SharedString::from(crate::markdown::align_numeric_columns(text)),
                    ),
                    cx,
                ),
            };
            div().pl(px(GUTTER)).child(body).into_any_element()
        }
        Item::Thinking {
            id,
            text,
            streaming,
            elapsed,
            ..
        } => render_thinking(view, *id, text, *streaming, *elapsed, cx),
        Item::Tool(tool) if tool.name == "subagent" => render_subagent_card(view, tool, cx),
        Item::Tool(tool) => render_tool(view, tool, cx),
        Item::Notice { id, text } => h_flex()
            .id(("notice", id.0))
            .gap_3()
            .my_1()
            .text_size(px(12.5))
            .text_color(p.subtext0)
            .child(div().flex_1().h(px(1.)).bg(p.surface0))
            .child(SharedString::from(text.clone()))
            .child(div().flex_1().h(px(1.)).bg(p.surface0))
            .into_any_element(),
        Item::Error { id, text, retrying } => render_error(view, *id, text, *retrying, cx),
        Item::McpAttention { id, servers } => render_mcp_attention(view, *id, servers, cx),
        Item::SignIn(sign_in) => render_sign_in(view, sign_in, cx),
        Item::Shell(run) => render_shell(view, run, cx),
    }
}

/// Pi's Markdown as the design has it: code blocks drawn by our own plugin,
/// inline code without the kit's tint, and tables with a mantle header,
/// hairline rows and a copy menu.
fn markdown_style(text: TextView, cx: &App) -> AnyElement {
    let p = palette(cx);
    let table = StyleRefinement::default()
        .border_1()
        .border_color(p.surface0)
        .rounded(px(8.))
        .text_size(px(13.));
    let table_head = StyleRefinement::default()
        .bg(p.mantle)
        .text_color(p.subtext0)
        .font_weight(FontWeight::NORMAL);
    // The kit draws column dividers in the cell's border colour; the design
    // has rows only.
    let table_cell = StyleRefinement::default()
        .px_3()
        .py(px(7.))
        .border_color(gpui_kit::transparent_black());
    text.style(
        TextViewStyle::default()
            .inline_code(HighlightStyle {
                background_color: Some(gpui_kit::transparent_black()),
                ..Default::default()
            })
            .table(table)
            .table_head(table_head)
            .table_cell(table_cell),
    )
    .plugin(super::code_block::CodeBlocks)
    .table_actions(|table, _, _| {
        let (markdown, csv) = (table.markdown.clone(), table_csv(table));
        h_flex().justify_end().child(
            Button::new("copy-table")
                .ghost()
                .xsmall()
                .icon(icon(Lucide::Copy).size(px(14.)))
                .tooltip("Copy table")
                .dropdown_menu(move |menu, _, _| {
                    let (markdown, csv) = (markdown.clone(), csv.clone());
                    menu.item(
                        PopupMenuItem::new("Copy as Markdown").on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(markdown.clone()))
                        }),
                    )
                    .item(PopupMenuItem::new("Copy as CSV").on_click(move |_, _, cx| {
                        cx.write_to_clipboard(ClipboardItem::new_string(csv.clone()))
                    }))
                }),
        )
    })
    .into_any_element()
}

/// A table as CSV, quoting cells that need it.
fn table_csv(table: &gpui_kit::component::text::TableData) -> String {
    let quote = |cell: &String| {
        if cell.contains([',', '"', '\n']) {
            format!("\"{}\"", cell.replace('"', "\"\""))
        } else {
            cell.clone()
        }
    };
    std::iter::once(&table.headers)
        .chain(table.rows.iter())
        .map(|row| row.iter().map(quote).collect::<Vec<_>>().join(","))
        .collect::<Vec<_>>()
        .join("\n")
}

/// A command's exit code: green when it succeeded, red when it didn't.
fn exit_pill(ok: bool, label: String, p: &Palette) -> Div {
    div()
        .flex_none()
        .px(px(6.))
        .rounded(px(4.))
        .text_size(px(11.5))
        .font_family(MONO)
        .bg(mix(if ok { p.green } else { p.red }, p.base, 0.16))
        .text_color(if ok { p.add } else { p.delete })
        .child(label)
}

/// The badge that stands in for the avatar on `!` commands.
/// Marks a command as yours; Red once it fails, like the rest of its row.
pub fn shell_badge(p: &Palette, failed: bool) -> Div {
    div()
        .flex_none()
        .size(px(22.))
        .rounded(px(6.))
        .bg(if failed { p.red } else { p.shell })
        .flex()
        .items_center()
        .justify_center()
        .child(icon(Lucide::Terminal).size(px(13.)).text_color(if failed {
            p.on_accent
        } else {
            p.on_shell
        }))
}

/// A command the user ran with `!`, and its output.
fn render_shell(view: &ChatView, run: &ShellRun, cx: &mut Context<ChatView>) -> AnyElement {
    let p = palette(cx);
    let lines: Vec<&str> = run.output.trim_end().lines().collect();
    // As Pi's bash card: the last 5 lines while it runs, up to 40 after.
    let keep = if run.running { 5 } else { 40 };
    let failed = !run.running && !run.cancelled && run.exit_code.is_some_and(|c| c != 0);
    // A failure turns the header Red, as Pi's failed bash rows do.
    let accent = if failed { p.red } else { p.shell_text };
    let hidden = lines.len().saturating_sub(keep);
    let shown = lines[hidden..].join("\n");
    let mut meta = Vec::new();
    if run.cancelled {
        meta.push("stopped".to_string());
    }
    if !run.running {
        // A run restored from Pi's session file has no duration.
        if let Some(duration) = run.duration {
            meta.push(format_duration(duration));
        }
        meta.push(
            if run.excluded {
                "kept from Pi"
            } else {
                "sent to Pi"
            }
            .to_string(),
        );
    }
    let session = view.session.clone();
    h_flex()
        .id(("shell", run.id.0))
        .mt_2()
        .gap_3()
        .items_start()
        .child(shell_badge(&p, failed))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    h_flex()
                        .min_h(px(22.))
                        .gap_2()
                        .text_size(px(13.))
                        .text_color(p.subtext0)
                        .child(div().flex_none().text_color(accent).child(if run.running {
                            ShimmerText::new("Running").into_any_element()
                        } else {
                            "Ran".into_any_element()
                        }))
                        .child(
                            mono(run.command.clone(), accent)
                                .flex_1()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap(),
                        )
                        .when_some(run.exit_code.filter(|_| !run.cancelled), |this, code| {
                            this.child(exit_pill(code == 0, format!("exit {code}"), &p))
                        })
                        .when(!meta.is_empty(), |this| {
                            this.child(
                                div()
                                    .flex_none()
                                    .text_size(px(12.5))
                                    .child(meta.join(" · ")),
                            )
                        })
                        .when(run.running, |this| {
                            this.child(
                                Button::new(("shell-stop", run.id.0))
                                    .ghost()
                                    .xsmall()
                                    .label("Stop")
                                    .on_click(move |_, _, cx| {
                                        session.update(cx, |s, cx| s.stop_shell(cx))
                                    }),
                            )
                        }),
                )
                .when_some(run.error.clone(), |this, error| {
                    this.child(
                        div()
                            .mt_1()
                            .text_size(px(13.))
                            .text_color(p.red)
                            .child(error),
                    )
                })
                // A finished command with nothing to show says so, as Pi's does.
                .when(!shown.is_empty() || !run.running, |this| {
                    this.child(
                        v_flex()
                            .mt(px(6.))
                            .px_3()
                            .py_2()
                            .rounded(px(6.))
                            // The well of Pi's own bash card, so output reads
                            // the same; the faint yellow ties it to the badge.
                            .bg(p.mantle)
                            .border_1()
                            .border_color(if failed {
                                mix(p.red, p.base, 0.5)
                            } else {
                                mix(p.shell, p.surface0, 0.28)
                            })
                            .font_family(MONO)
                            .text_size(px(12.))
                            .line_height(px(19.))
                            .when(hidden > 0, |this| {
                                this.child(
                                    div()
                                        .text_color(p.subtext0)
                                        .child(format!("{hidden} earlier lines")),
                                )
                            })
                            .child(if shown.is_empty() {
                                div().text_color(p.subtext0).child("No output")
                            } else {
                                div().whitespace_normal().child(SharedString::from(shown))
                            })
                            .when(run.running, |this| {
                                this.child(
                                    div()
                                        .mt_1()
                                        .font_family(crate::theme::SANS)
                                        .text_color(p.subtext0)
                                        .child("Live output · last 5 lines"),
                                )
                            }),
                    )
                })
                .when_some(run.full_output.clone(), |this, path| {
                    this.child(
                        div()
                            .mt_1()
                            .text_size(px(12.))
                            .text_color(p.subtext0)
                            .child(format!("Output cut short · the whole of it is in {path}")),
                    )
                }),
        )
        .into_any_element()
}

/// MCP servers Pi couldn't use, with a Sign in button for those that need one.
fn render_mcp_attention(
    view: &ChatView,
    id: ItemId,
    servers: &[(String, String)],
    cx: &mut Context<ChatView>,
) -> AnyElement {
    let p = palette(cx);
    let session = view.session.clone();
    // A later sign-in to a server settles or covers its row.
    let sign_in_of = |server: &str| {
        session
            .read(cx)
            .items()
            .iter()
            .rev()
            .find_map(|item| match item {
                Item::SignIn(s) if s.server == server && s.id.0 > id.0 => Some(s.state.clone()),
                _ => None,
            })
    };
    let rows: Vec<_> = servers
        .iter()
        .map(|(name, state)| {
            let later = sign_in_of(name);
            let shown = match &later {
                Some(SignInState::Done(_)) => "signed in".to_string(),
                _ if state.starts_with("needs sign-in") => "needs sign-in".to_string(),
                _ => state.clone(),
            };
            let offer = state.starts_with("needs sign-in")
                && !matches!(later, Some(SignInState::Done(_) | SignInState::Waiting));
            (name.clone(), shown, offer)
        })
        .collect();
    v_flex()
        .id(("mcp-attention", id.0))
        .ml(px(GUTTER))
        .min_w_0()
        .gap_1()
        .px_3()
        .py_2()
        .rounded(px(8.))
        .border_1()
        .border_color(mix(p.yellow, p.base, 0.4))
        .bg(mix(p.yellow, p.base, 0.08))
        .text_size(px(13.))
        .child(
            h_flex()
                .gap_2()
                .min_h(px(24.))
                .child(icon(Lucide::Plug).text_color(p.yellow_text))
                .child("MCP servers need attention"),
        )
        .children(
            rows.into_iter()
                .enumerate()
                .map(|(ix, (name, state, offer))| {
                    let (session, server) = (session.clone(), name.clone());
                    h_flex()
                        .min_h(px(28.))
                        .gap_3()
                        .pl(px(24.))
                        .child(mono(name.clone(), p.text).flex_none())
                        .child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .truncate()
                                .text_color(p.subtext0)
                                .child(SharedString::from(state.clone())),
                        )
                        .when(offer, |this| {
                            this.child(
                                Button::new(("mcp-sign-in", id.0 * 100 + ix as u64))
                                    .small()
                                    .label("Sign in")
                                    .on_click(move |_, _, cx| {
                                        session.update(cx, |s, cx| s.sign_in(&server, cx))
                                    }),
                            )
                        })
                }),
        )
        .into_any_element()
}

/// A sign-in to an MCP server: waiting for the browser, then how it went.
fn render_sign_in(view: &ChatView, sign_in: &SignIn, cx: &mut Context<ChatView>) -> AnyElement {
    let p = palette(cx);
    let session = view.session.clone();
    let server = sign_in.server.clone();
    let id = sign_in.id;
    let retry = {
        let (session, server) = (session.clone(), server.clone());
        Button::new(("sign-in-retry", id.0))
            .ghost()
            .xsmall()
            .label("Try again")
            .on_click(move |_, _, cx| session.update(cx, |s, cx| s.sign_in(&server, cx)))
    };
    let outcome = |name: Lucide, color: Hsla, text: String| {
        h_flex()
            .id(("sign-in", id.0))
            .gap_2()
            .min_h(px(24.))
            .text_size(px(13.))
            .text_color(p.subtext0)
            .child(
                div()
                    .ml(px(3.))
                    .mr(px(7.))
                    .child(icon(name).text_color(color)),
            )
            .child(div().flex_1().min_w_0().child(SharedString::from(text)))
    };
    match &sign_in.state {
        SignInState::Done(detail) if detail.is_empty() => {
            outcome(Lucide::Plug, p.green, format!("Signed in to {server}")).into_any_element()
        }
        SignInState::Done(detail) => outcome(
            Lucide::Plug,
            p.green,
            format!("Signed in to {server} · {detail}"),
        )
        .into_any_element(),
        SignInState::Cancelled => outcome(
            Lucide::Plug,
            p.subtext0,
            format!("Sign-in to {server} cancelled"),
        )
        .child(retry)
        .into_any_element(),
        SignInState::Failed(error) => outcome(
            Lucide::CircleAlert,
            p.red,
            match crate::mcp::sign_in_hint(error) {
                Some(hint) => format!("Couldn't sign in to {server}: {error}. {hint}"),
                None => format!("Couldn't sign in to {server}: {error}"),
            },
        )
        .child(retry)
        .into_any_element(),
        SignInState::Waiting => {
            let ready = sign_in.request_id.is_some();
            let url = sign_in.url.clone();
            let input = view.redirect_input.clone();
            v_flex()
                .id(("sign-in", id.0))
                .mt_1()
                .ml(px(GUTTER))
                .gap_2()
                .p_3()
                .rounded(px(8.))
                .border_1()
                .border_color(p.blue)
                .bg(mix(p.blue, p.base, 0.06))
                .child(
                    h_flex()
                        .gap_2()
                        .text_size(px(13.5))
                        .font_medium()
                        .child(Spinner::new().with_size(px(14.)).color(p.blue))
                        .child(format!("Signing in to {server}")),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .text_color(p.subtext0)
                        .child(if url.is_some() {
                            "Finish signing in in your browser. If the browser can't reach this machine, paste the address it ended up on."
                        } else {
                            "Starting the sign-in…"
                        }),
                )
                .when(ready, |this| {
                    let (session_paste, session_cancel) = (session.clone(), session.clone());
                    let paste = input.clone();
                    this.child(
                        h_flex()
                            .gap_2()
                            .child(div().flex_1().min_w_0().child(Input::new(&input).small()))
                            .child(
                                Button::new(("sign-in-paste", id.0))
                                    .small()
                                    .label("Use address")
                                    .on_click(move |_, window, cx| {
                                        let value = paste.read(cx).value().trim().to_string();
                                        if value.is_empty() {
                                            return;
                                        }
                                        paste.update(cx, |state, cx| state.set_value("", window, cx));
                                        session_paste
                                            .update(cx, |s, cx| s.answer_sign_in(Some(value), cx));
                                    }),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .when_some(url, |this, url| {
                                this.child(
                                    Button::new(("sign-in-open", id.0))
                                        .primary()
                                        .small()
                                        .icon(icon(Lucide::ExternalLink))
                                        .label("Open sign-in page")
                                        .on_click(move |_, _, cx| cx.open_url(&url)),
                                )
                            })
                            .child(
                                Button::new(("sign-in-cancel", id.0))
                                    .ghost()
                                    .small()
                                    .label("Cancel")
                                    .on_click(move |_, _, cx| {
                                        session_cancel.update(cx, |s, cx| s.answer_sign_in(None, cx))
                                    }),
                            ),
                    )
                })
                .into_any_element()
        }
    }
}

/// A model or provider error. The first line says what went wrong; details
/// such as a stack trace stay folded so the box keeps to the chat column.
fn render_error(
    view: &ChatView,
    id: ItemId,
    text: &str,
    retrying: bool,
    cx: &mut Context<ChatView>,
) -> AnyElement {
    let p = palette(cx);
    let mut lines = text.lines().filter(|l| !l.trim().is_empty());
    let summary = lines.next().unwrap_or("").trim().to_string();
    let details: Vec<&str> = lines.collect();
    let expanded = view.is_expanded(id, false);
    v_flex()
        .id(("error", id.0))
        .ml(px(GUTTER))
        .min_w_0()
        .overflow_hidden()
        .gap_2()
        .px_3()
        .py_2()
        .rounded(px(8.))
        .border_1()
        .border_color(mix(p.red, p.base, 0.4))
        .bg(mix(p.red, p.base, 0.1))
        .text_size(px(13.))
        .child(
            h_flex()
                .items_start()
                .gap_2()
                .child(
                    div()
                        .flex_none()
                        .mt(px(2.))
                        .child(icon(Lucide::CircleAlert).text_color(p.red)),
                )
                .child(div().flex_1().min_w_0().child(SharedString::from(summary)))
                .when(retrying, |this| {
                    this.child(Spinner::new().xsmall().color(p.subtext0))
                })
                .when(!details.is_empty(), |this| {
                    this.child(
                        Button::new(("error-details", id.0))
                            .ghost()
                            .xsmall()
                            .label(if expanded { "Hide details" } else { "Details" })
                            .on_click(cx.listener(move |view, _, _, cx| view.toggle(id, cx))),
                    )
                }),
        )
        .when(expanded && !details.is_empty(), |this| {
            this.child(
                div()
                    .ml(px(24.))
                    .min_w_0()
                    .overflow_hidden()
                    .font_family(MONO)
                    .text_size(px(11.5))
                    .text_color(p.subtext0)
                    .child(SharedString::from(details.join("\n"))),
            )
        })
        .into_any_element()
}

/// How far a 22px avatar or badge moves down to sit centred on the first
/// line of body text.
pub fn first_line_offset(cx: &App) -> Pixels {
    let line = crate::config::Config::global(cx).text_size * 1.618;
    px(((line - 22.) / 2.).max(0.))
}

pub fn avatar(initials: &SharedString, p: &Palette) -> Div {
    div()
        .flex_none()
        .size(px(22.))
        .rounded_full()
        .bg(p.blue)
        .text_color(p.on_accent)
        .flex()
        .items_center()
        .justify_center()
        .text_size(px(10.))
        .font_semibold()
        .child(initials.clone())
}

fn mono(text: impl Into<SharedString>, color: Hsla) -> Div {
    div()
        .font_family(MONO)
        .text_size(px(12.))
        .text_color(color)
        .child(text.into())
}

fn render_thinking(
    view: &ChatView,
    id: ItemId,
    text: &str,
    streaming: bool,
    elapsed: Option<std::time::Duration>,
    cx: &mut Context<ChatView>,
) -> AnyElement {
    let p = palette(cx);
    let expanded = view.is_expanded(id, crate::config::Config::global(cx).always_expand_thinking);
    let label: AnyElement = if streaming {
        ShimmerText::new("Thinking…").into_any_element()
    } else {
        // A chat restored from Pi's session file doesn't know how long.
        match elapsed {
            Some(e) => SharedString::from(format!("Thought for {}s", e.as_secs().max(1))),
            None => SharedString::from("Thought"),
        }
        .into_any_element()
    };
    v_flex()
        .id(("thinking", id.0))
        .child(
            h_flex()
                .id(("thinking-row", id.0))
                .gap_2()
                .min_h(px(24.))
                .text_size(px(13.))
                .text_color(p.subtext0)
                .cursor_pointer()
                .hover(|s| s.text_color(p.text))
                .child(div().ml(px(3.)).mr(px(7.)).child(icon(Lucide::Lightbulb)))
                .child(label)
                .on_click(cx.listener(move |view, _, _, cx| view.toggle(id, cx))),
        )
        .when(expanded && !text.is_empty(), |this| {
            this.child(
                div()
                    .ml(px(10.))
                    .pl(px(23.))
                    .border_l_1()
                    .border_color(p.surface0)
                    .text_size(px(13.))
                    .text_color(p.subtext0)
                    .child(SharedString::from(text.to_string())),
            )
        })
        .into_any_element()
}

pub(super) struct ToolText {
    pub icon: Lucide,
    pub running: &'static str,
    pub done: &'static str,
    pub target: String,
    pub result: Option<String>,
}

pub(super) fn describe(tool: &ToolCall) -> ToolText {
    let path = tool.path().unwrap_or("").to_string();
    match tool.name.as_str() {
        "read" => {
            let range = match (tool.args["offset"].as_u64(), tool.args["limit"].as_u64()) {
                (Some(offset), Some(limit)) => {
                    Some(format!("lines {offset}–{}", offset + limit - 1))
                }
                (Some(offset), None) => Some(format!("from line {offset}")),
                _ => None,
            };
            let lines = tool.output.lines().count();
            ToolText {
                icon: Lucide::File,
                running: "Reading",
                done: "Read",
                target: path,
                result: range.or_else(|| (lines > 0).then(|| format!("{lines} lines"))),
            }
        }
        "edit" => {
            let edits = tool.args["edits"].as_array().map(|e| e.len()).unwrap_or(1);
            ToolText {
                icon: Lucide::Pencil,
                running: "Editing",
                done: "Edited",
                target: path,
                result: (edits > 1).then(|| format!("{edits} edits")),
            }
        }
        "write" => ToolText {
            icon: Lucide::FilePlus,
            running: "Writing",
            done: if tool.previous_content.is_some() {
                "Rewrote"
            } else {
                "Created"
            },
            target: path,
            result: None,
        },
        "bash" => ToolText {
            icon: Lucide::Terminal,
            running: "Running",
            done: "Ran",
            target: tool.command().unwrap_or("").to_string(),
            result: None,
        },
        "grep" | "find" | "ls" => ToolText {
            icon: Lucide::Search,
            running: "Searching",
            done: "Searched",
            target: tool.args["pattern"]
                .as_str()
                .or(tool.args["path"].as_str())
                .unwrap_or("")
                .to_string(),
            result: None,
        },
        "codemode" => ToolText {
            icon: Lucide::SquareTerminal,
            running: "Running a script",
            done: "Ran a script",
            target: tool.args["code"]
                .as_str()
                .and_then(|code| {
                    code.lines().find(|l| {
                        !l.trim().is_empty() && !l.trim_start().starts_with("// @options")
                    })
                })
                .unwrap_or("")
                .trim()
                .to_string(),
            result: None,
        },
        "tool_search" => ToolText {
            icon: Lucide::Search,
            running: "Looking for tools",
            done: "Looked for tools",
            target: tool.args["query"].as_str().unwrap_or("").to_string(),
            result: None,
        },
        name if name.starts_with("mcp__") => {
            // `mcp__<server>__<tool>`
            let rest = &name[5..];
            let (server, tool_name) = rest.split_once("__").unwrap_or(("", rest));
            ToolText {
                icon: Lucide::Plug,
                running: "Using",
                done: "Used",
                target: if server.is_empty() {
                    tool_name.to_string()
                } else {
                    format!("{server} · {tool_name}")
                },
                result: None,
            }
        }
        _ if tool.is_unknown_tool() => ToolText {
            icon: Lucide::CircleAlert,
            running: "Running",
            done: "Tried a tool Pi doesn't have",
            target: tool.clean_name().to_string(),
            result: None,
        },
        _ => ToolText {
            icon: Lucide::Wrench,
            running: "Running",
            done: "Used",
            target: tool.clean_name().to_string(),
            result: None,
        },
    }
}

/// `+3 −1` coloured like the right dock.
fn counts(additions: usize, deletions: usize, p: &Palette) -> Div {
    h_flex()
        .gap_1()
        .font_family(MONO)
        .text_size(px(12.))
        .when(additions > 0, |this| {
            this.child(div().text_color(p.add).child(format!("+{additions}")))
        })
        .when(deletions > 0, |this| {
            this.child(div().text_color(p.delete).child(format!("−{deletions}")))
        })
}

fn render_tool(view: &ChatView, tool: &ToolCall, cx: &mut Context<ChatView>) -> AnyElement {
    let p = palette(cx);
    let text = describe(tool);
    let id = tool.id;
    let patch = tool.patch();
    let stat = patch.as_deref().map(crate::git::diff_stat);
    let output_lines = tool.output.lines().count();
    let running = matches!(tool.status, ToolStatus::Running | ToolStatus::Preparing);
    let unknown = tool.is_unknown_tool();
    let failed = tool.status == ToolStatus::Failed;
    let session_running = view.session.read(cx).is_running();
    let keep_diffs = crate::config::Config::global(cx).keep_diffs_expanded;

    // What a row shows without the user asking.
    let default_open = match tool.name.as_str() {
        "bash" => running || failed || (output_lines > 0 && output_lines <= 6),
        "edit" | "write" => session_running || keep_diffs || failed,
        _ => false,
    };
    let expanded = view.is_expanded(id, default_open);

    let verb: AnyElement = match tool.status {
        ToolStatus::Running | ToolStatus::Preparing => {
            ShimmerText::new(text.running).into_any_element()
        }
        ToolStatus::Denied => "Didn't run".into_any_element(),
        ToolStatus::Failed if tool.name == "edit" => "Edit failed".into_any_element(),
        ToolStatus::AwaitingApproval => "Wants to run".into_any_element(),
        _ => text.done.into_any_element(),
    };
    let lead: AnyElement = if running {
        div()
            .ml(px(4.))
            .mr(px(8.))
            .child(Spinner::new().with_size(px(14.)).color(p.subtext1))
            .into_any_element()
    } else {
        div()
            .ml(px(3.))
            .mr(px(7.))
            .child(icon(text.icon).text_color(if failed { p.red } else { p.subtext0 }))
            .into_any_element()
    };

    let result: Option<AnyElement> = match (tool.name.as_str(), tool.status) {
        (_, ToolStatus::Denied) => Some(div().child("denied").into_any_element()),
        _ if unknown => Some(div().child("nothing ran").into_any_element()),
        ("bash", ToolStatus::Stopped) => Some(
            h_flex()
                .gap_2()
                .child("stopped by you")
                .when_some(tool.duration, |this, d| this.child(format_duration(d)))
                .into_any_element(),
        ),
        ("bash", ToolStatus::Done | ToolStatus::Failed) => {
            let code = tool.exit_code().unwrap_or(1);
            let ok = code == 0;
            let timed_out = tool.output.contains("timed out");
            Some(
                h_flex()
                    .gap_2()
                    .child(exit_pill(
                        ok,
                        if timed_out {
                            "timed out".to_string()
                        } else {
                            format!("exit {code}")
                        },
                        &p,
                    ))
                    .when_some(tool.duration, |this, d| this.child(format_duration(d)))
                    .into_any_element(),
            )
        }
        ("bash", ToolStatus::Running) => Some(
            div()
                .child(format_duration(tool.started.elapsed()))
                .into_any_element(),
        ),
        (_, ToolStatus::Failed) => Some(
            div()
                .text_color(p.red)
                .child(SharedString::from(first_line(&tool.output).to_string()))
                .into_any_element(),
        ),
        ("edit" | "write", ToolStatus::Done) => {
            stat.map(|s| counts(s.additions, s.deletions, &p).into_any_element())
        }
        _ => text
            .result
            .clone()
            .map(|r| div().child(r).into_any_element()),
    };

    let header = h_flex()
        .id(("tool", id.0))
        .gap_2()
        .min_h(px(24.))
        .text_size(px(13.))
        .text_color(p.subtext0)
        .cursor_pointer()
        .hover(|s| s.text_color(p.text))
        .child(lead)
        .child(div().flex_none().whitespace_nowrap().child(verb))
        .child(
            mono(
                text.target.clone(),
                if failed && !unknown {
                    p.red
                } else {
                    p.subtext1
                },
            )
            .flex_1()
            .min_w_0()
            .overflow_hidden()
            .text_ellipsis()
            .whitespace_nowrap(),
        )
        .when_some(result, |this, result| {
            this.child(div().flex_none().text_size(px(12.5)).child(result))
        })
        .on_click(cx.listener(move |view, _, _, cx| view.toggle(id, cx)));

    let approval = view
        .session
        .read(cx)
        .approvals
        .iter()
        .find(|a| a.tool_call_id == tool.call_id)
        .cloned();

    v_flex()
        .id(("tool-wrap", id.0))
        .child(header)
        .when_some(approval, |this, approval| this.child(render_approval(view, &approval, cx)))
        .when(expanded, |this| match tool.name.as_str() {
            "bash" => this.child(render_bash_output(tool, &p)),
            "edit" | "write" if failed => this.child(error_card(
                "The text to replace wasn't found in the file, so nothing changed. Pi usually reads the file again and retries.",
                &p,
            )),
            "edit" | "write" => match patch.as_deref() {
                Some(patch) => this.child(render_mini_diff(view, tool, patch, cx)),
                None => this,
            },
            _ if unknown => this.child(error_card(
                &if view.session.read(cx).trusted {
                    format!("Pi has no tool called “{}”, so nothing ran. The model may have mangled the call; Pi usually tries again.", tool.clean_name())
                } else {
                    format!("Pi has no tool called “{}”, so nothing ran. This folder isn't trusted, so Pi can only read files.", tool.clean_name())
                },
                &p,
            )),
            _ if !tool.output.is_empty() && failed => this.child(error_card(&tool.output, &p)),
            _ => this.child(render_tool_output(tool, &p)),
        })
        .into_any_element()
}

fn first_line(text: &str) -> &str {
    text.lines()
        .find(|l| !l.trim().is_empty())
        .unwrap_or("")
        .trim()
}

fn error_card(text: &str, p: &Palette) -> Div {
    div()
        .mt_1()
        .ml(px(GUTTER))
        .px_3()
        .py_2()
        .rounded(px(6.))
        .border_1()
        .border_color(mix(p.red, p.base, 0.4))
        .bg(mix(p.red, p.base, 0.08))
        .text_size(px(13.))
        .min_w_0()
        .overflow_hidden()
        .child(SharedString::from(text.to_string()))
}

fn card(p: &Palette) -> Div {
    div()
        .mt_1()
        .ml(px(GUTTER))
        .rounded(px(6.))
        .border_1()
        .border_color(p.surface0)
        .bg(p.mantle)
        .overflow_hidden()
}

/// What a tool was given and what it returned, for tools without their own
/// view: MCP tools, scripts, searches and reads.
fn render_tool_output(tool: &ToolCall, p: &Palette) -> Div {
    let (input_label, input) = match tool.name.as_str() {
        "codemode" => (
            "Script",
            tool.args["code"].as_str().unwrap_or("").trim().to_string(),
        ),
        _ => (
            "Input",
            match &tool.args {
                Value::Object(map) if map.is_empty() => String::new(),
                Value::Null => String::new(),
                args => serde_json::to_string_pretty(args).unwrap_or_default(),
            },
        ),
    };
    // MCP tools often return JSON on one line.
    let output = serde_json::from_str::<Value>(tool.output.trim())
        .ok()
        .filter(|v| v.is_object() || v.is_array())
        .and_then(|v| serde_json::to_string_pretty(&v).ok())
        .unwrap_or_else(|| tool.output.trim_end().to_string());
    let section = |label: &'static str, text: &str, keep: usize, first: bool| {
        let lines: Vec<&str> = text.lines().collect();
        let more = lines.len().saturating_sub(keep);
        v_flex()
            .px_3()
            .py_2()
            .gap_1()
            .when(!first, |this| this.border_t_1().border_color(p.surface0))
            .child(
                div()
                    .text_size(px(11.5))
                    .text_color(p.subtext0)
                    .child(label),
            )
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(12.))
                    .line_height(px(19.))
                    .whitespace_normal()
                    .child(SharedString::from(lines[..lines.len() - more].join("\n"))),
            )
            .when(more > 0, |this| {
                this.child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.subtext0)
                        .child(format!("{more} more lines")),
                )
            })
    };
    let has_input = !input.is_empty();
    card(p)
        .when(has_input, |this| {
            this.child(section(input_label, &input, 20, true))
        })
        .child(if output.is_empty() {
            section("Output", "(no output)", 1, !has_input)
        } else {
            section("Output", &output, 40, !has_input)
        })
}

fn render_bash_output(tool: &ToolCall, p: &Palette) -> Div {
    let running = tool.status == ToolStatus::Running;
    let failed = tool.status == ToolStatus::Failed;
    let lines: Vec<&str> = tool
        .output
        .lines()
        .filter(|l| !l.starts_with("Command exited with code "))
        .collect();
    let keep = if running {
        5
    } else if failed {
        12
    } else {
        40
    };
    let hidden = lines.len().saturating_sub(keep);
    let shown = lines[hidden..].join("\n");

    card(p)
        .when(failed, |this| this.border_color(mix(p.red, p.base, 0.5)))
        .child(
            v_flex()
                .px_3()
                .py_2()
                .font_family(MONO)
                .text_size(px(12.))
                .line_height(px(19.))
                .when(hidden > 0, |this| {
                    this.child(
                        div()
                            .text_color(p.subtext0)
                            .child(format!("{hidden} earlier lines")),
                    )
                })
                .child(if shown.is_empty() && !running {
                    div().text_color(p.subtext0).child("No output")
                } else {
                    div().whitespace_normal().child(SharedString::from(shown))
                }),
        )
        .when(running, |this| {
            this.child(
                h_flex()
                    .px_3()
                    .py_1()
                    .border_t_1()
                    .border_color(p.surface0)
                    .text_size(px(12.))
                    .text_color(p.subtext0)
                    .child("Live output · last 5 lines"),
            )
        })
}

fn render_mini_diff(
    view: &ChatView,
    tool: &ToolCall,
    patch: &str,
    cx: &mut Context<ChatView>,
) -> AnyElement {
    let p = palette(cx);
    let hunks = parse_hunks(patch);
    let created = tool.name == "write" && tool.previous_content.is_none();
    let total_changed: usize = hunks.iter().map(|h| h.changed()).sum();
    let limit = if created {
        6
    } else if total_changed <= 10 {
        usize::MAX
    } else {
        12
    };
    let path = tool.path().unwrap_or("").to_string();

    let mut rows = Vec::new();
    let mut shown = 0;
    let show_all = total_changed <= 10 && !created;
    for hunk in hunks.iter().take(if show_all { hunks.len() } else { 1 }) {
        if !created {
            rows.push(
                div()
                    .px_3()
                    .py(px(3.))
                    .text_size(px(12.))
                    .text_color(p.subtext0)
                    .child(SharedString::from(hunk.header.clone()))
                    .into_any_element(),
            );
        }
        for line in hunk.lines.iter() {
            if shown >= limit {
                break;
            }
            shown += 1;
            let (bg, fg, sign) = match line.kind {
                LineKind::Add => (Some(mix(p.green, p.base, 0.12)), p.add, "+"),
                LineKind::Delete => (Some(mix(p.red, p.base, 0.12)), p.delete, "-"),
                LineKind::Context => (None, p.subtext0, ""),
            };
            rows.push(
                h_flex()
                    .font_family(MONO)
                    .text_size(px(12.5))
                    .line_height(px(22.))
                    .whitespace_nowrap()
                    .when_some(bg, |this, bg| this.bg(bg))
                    .child(
                        div()
                            .w(px(40.))
                            .pr_2()
                            .text_right()
                            .text_color(fg)
                            .child(line.number.map(|n| n.to_string()).unwrap_or_default()),
                    )
                    .child(div().w(px(18.)).text_center().text_color(fg).child(sign))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .overflow_hidden()
                            .child(SharedString::from(line.text.clone())),
                    )
                    .into_any_element(),
            );
        }
    }
    let total_lines: usize = hunks.iter().map(|h| h.lines.len()).sum();
    let more = if created {
        total_lines.saturating_sub(shown)
    } else if show_all {
        0
    } else {
        hunks.len().saturating_sub(1)
    };

    let worktree = view.session.read(cx).cwd.clone();
    card(&p)
        .children(rows)
        .child(
            h_flex()
                .px_2()
                .py_1()
                .gap_1()
                .border_t_1()
                .border_color(p.surface0)
                .when(more > 0, |this| {
                    this.child(
                        Button::new(("more", tool.id.0))
                            .ghost()
                            .xsmall()
                            .label(if created {
                                format!("Show {more} more lines")
                            } else {
                                format!("Show {more} more hunks")
                            })
                            .on_click(cx.listener({
                                let path = path.clone();
                                let worktree = worktree.clone();
                                move |view, _, window, cx| {
                                    view.open_diff_at(worktree.clone(), path.clone(), window, cx)
                                }
                            })),
                    )
                })
                .child(div().flex_1())
                .child(
                    Button::new(("open-diff", tool.id.0))
                        .ghost()
                        .xsmall()
                        .label("Open in diff")
                        .tooltip_with_action("Open in diff", &crate::actions::ShowDiff, None)
                        .on_click(cx.listener(move |view, _, window, cx| {
                            view.open_diff_at(worktree.clone(), path.clone(), window, cx)
                        })),
                ),
        )
        .into_any_element()
}

fn render_approval(
    view: &ChatView,
    approval: &PendingApproval,
    cx: &mut Context<ChatView>,
) -> AnyElement {
    let p = palette(cx);
    let session = view.session.clone();
    let branch = view.branch.clone();
    let request_id = approval.request_id.clone();
    let detail = match approval.tool_name.as_str() {
        "bash" => approval.input["command"].as_str().unwrap_or("").to_string(),
        _ => approval.input["path"]
            .as_str()
            .map(|p| format!("{} {p}", approval.tool_name))
            .unwrap_or_else(|| approval.tool_name.clone()),
    };
    let what = if approval.tool_name == "bash" {
        "a command"
    } else {
        "a tool"
    };

    v_flex()
        .mt_1()
        .ml(px(GUTTER))
        .gap_2()
        .p_3()
        .rounded(px(8.))
        .border_1()
        .border_color(p.blue)
        .bg(mix(p.blue, p.base, 0.06))
        .child(
            h_flex()
                .gap_2()
                .text_size(px(13.5))
                .font_medium()
                .child(icon(Lucide::Shield).text_color(p.blue))
                .child(format!("Wants to run {what}")),
        )
        .child(
            div()
                .text_size(px(13.))
                .text_color(p.subtext0)
                .child(format!("Pi wants to run this in {branch}:")),
        )
        .child(
            div()
                .px_3()
                .py_2()
                .rounded(px(6.))
                .bg(p.mantle)
                .border_1()
                .border_color(p.surface0)
                .font_family(MONO)
                .text_size(px(12.5))
                .child(SharedString::from(format!("$ {detail}"))),
        )
        .child(
            h_flex()
                .gap_2()
                .min_w_0()
                .child(
                    Button::new(("approve", approval.tool_call_id.len()))
                        .primary()
                        .small()
                        .label("Run ⏎")
                        .on_click({
                            let (session, id) = (session.clone(), request_id.clone());
                            move |_, _, cx| {
                                session.update(cx, |s, cx| {
                                    s.resolve_approval(&id, ApprovalChoice::Run, cx)
                                })
                            }
                        }),
                )
                .when_some(approval.pattern.clone(), |this, pattern| {
                    let (session, id) = (session.clone(), request_id.clone());
                    this.child(
                        // A long pattern shrinks to an ellipsis; the tooltip has it all.
                        Button::new("always-allow")
                            .small()
                            .flex_shrink(1.)
                            .min_w_0()
                            .label(format!("Always allow {pattern}"))
                            .tooltip(format!("Always allow {pattern}"))
                            .on_click(move |_, _, cx| {
                                let pattern = pattern.clone();
                                session.update(cx, |s, cx| {
                                    s.resolve_approval(
                                        &id,
                                        ApprovalChoice::AlwaysAllow(pattern),
                                        cx,
                                    )
                                })
                            }),
                    )
                })
                .child(
                    Button::new("deny")
                        .ghost()
                        .small()
                        .label("Deny  esc")
                        .on_click({
                            let (session, id) = (session.clone(), request_id.clone());
                            move |_, _, cx| {
                                session.update(cx, |s, cx| {
                                    s.resolve_approval(
                                        &id,
                                        ApprovalChoice::Deny("Denied by the user".into()),
                                        cx,
                                    )
                                })
                            }
                        }),
                ),
        )
        .into_any_element()
}

fn render_read_group(
    view: &ChatView,
    reads: &[&ToolCall],
    cx: &mut Context<ChatView>,
) -> AnyElement {
    let p = palette(cx);
    let id = reads[0].id;
    let expanded = view.is_expanded(id, false);
    v_flex()
        .id(("reads", id.0))
        .child(
            h_flex()
                .id(("reads-row", id.0))
                .gap_2()
                .min_h(px(24.))
                .text_size(px(13.))
                .text_color(p.subtext0)
                .cursor_pointer()
                .hover(|s| s.text_color(p.text))
                .child(div().ml(px(3.)).mr(px(7.)).child(icon(Lucide::Files)))
                .child(format!("Read {} files", reads.len()))
                .child(
                    icon(if expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .size(px(14.)),
                )
                .on_click(cx.listener(move |view, _, _, cx| view.toggle(id, cx))),
        )
        .when(expanded, |this| {
            this.children(reads.iter().map(|read| {
                let text = describe(read);
                h_flex()
                    .pl(px(GUTTER))
                    .gap_2()
                    .min_h(px(22.))
                    .text_size(px(12.5))
                    .text_color(p.subtext0)
                    .child(mono(text.target, p.subtext1))
                    .when_some(text.result, |this, r| this.child(r))
            }))
        })
        .into_any_element()
}

fn render_subagent_card(
    view: &ChatView,
    tool: &ToolCall,
    cx: &mut Context<ChatView>,
) -> AnyElement {
    let p = palette(cx);
    let workspace = view.workspace.read(cx);
    let child = workspace
        .subagent_for_call(&tool.call_id)
        .map(|s| s.session.clone());
    let title = tool.args["title"]
        .as_str()
        .unwrap_or("Sub-agent")
        .to_string();
    let finished = !matches!(
        tool.status,
        ToolStatus::Preparing | ToolStatus::Running | ToolStatus::AwaitingApproval
    );
    if child.is_none() && finished {
        return render_past_subagent(view, tool, title, cx);
    }
    let Some(child) = child else {
        // The tool call is still being written, or Pi was started elsewhere.
        return h_flex()
            .id(("subagent-pending", tool.id.0))
            .gap_2()
            .min_h(px(24.))
            .text_size(px(13.))
            .text_color(p.subtext0)
            .child(div().ml(px(3.)).mr(px(7.)).child(icon(Lucide::Bot)))
            .child(ShimmerText::new("Starting sub-agent"))
            .child(mono(title, p.subtext1))
            .into_any_element();
    };
    render_agent_card(view, &child, Some(tool), cx)
}

/// A sub-agent from before Treehouse restarted: its session is gone, but
/// the parent's tool result holds its report, which the card expands to.
fn render_past_subagent(
    view: &ChatView,
    tool: &ToolCall,
    title: String,
    cx: &mut Context<ChatView>,
) -> AnyElement {
    let p = palette(cx);
    let id = tool.id;
    let expanded = view.is_expanded(id, false);
    let ok = tool.status == ToolStatus::Done;
    let detail = match tool.duration {
        Some(duration) if ok => format!("Finished · {}", format_duration(duration)),
        _ if ok => "Finished".to_string(),
        _ => "Stopped".to_string(),
    };
    v_flex()
        .ml(px(GUTTER))
        .rounded(px(8.))
        .border_1()
        .border_color(p.surface0)
        .bg(p.mantle)
        .overflow_hidden()
        .child(
            h_flex()
                .id(("past-agent", id.0))
                .gap_3()
                .px_3()
                .py(px(10.))
                .cursor_pointer()
                .hover(|s| s.bg(p.surface0))
                .child(
                    div()
                        .flex_none()
                        .size(px(30.))
                        .rounded(px(6.))
                        .bg(p.surface0)
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(if ok {
                            icon(IconName::Check).text_color(p.green)
                        } else {
                            icon(Lucide::CircleAlert).text_color(p.subtext0)
                        }),
                )
                .child(
                    v_flex()
                        .flex_1()
                        .min_w_0()
                        .child(div().text_size(px(13.5)).font_medium().child(title))
                        .child(
                            div()
                                .text_size(px(12.5))
                                .text_color(p.subtext0)
                                .child(detail),
                        ),
                )
                .child(
                    icon(if expanded {
                        IconName::ChevronDown
                    } else {
                        IconName::ChevronRight
                    })
                    .text_color(p.subtext0),
                )
                .on_click(cx.listener(move |view, _, _, cx| view.toggle(id, cx))),
        )
        .when(expanded, |this| {
            this.child(
                div()
                    .px_3()
                    .py_2()
                    .border_t_1()
                    .border_color(p.surface0)
                    .text_size(px(13.))
                    .whitespace_normal()
                    .child(SharedString::from(if tool.output.trim().is_empty() {
                        "No report.".to_string()
                    } else {
                        tool.output.trim().to_string()
                    })),
            )
        })
        .into_any_element()
}

/// The card that stands for a sub-agent in its parent's chat.
pub fn render_agent_card(
    view: &ChatView,
    child: &Entity<Session>,
    tool: Option<&ToolCall>,
    cx: &mut Context<ChatView>,
) -> AnyElement {
    let p = palette(cx);
    let session = child.read(cx);
    let status = session.status();
    let title = session.title();
    let calls = session.tool_call_count();
    let elapsed = tool
        .and_then(|t| t.duration)
        .unwrap_or_else(|| session.started.elapsed());
    let selected = view.docked_session == Some(child.entity_id());

    let (state, lead) = match status {
        AgentStatus::Working => (
            "Working",
            Spinner::new()
                .with_size(px(16.))
                .color(p.blue)
                .into_any_element(),
        ),
        AgentStatus::NeedsInput => (
            "Needs input",
            icon(Lucide::Shield).text_color(p.blue).into_any_element(),
        ),
        AgentStatus::Finished | AgentStatus::Idle => (
            "Finished",
            icon(IconName::Check).text_color(p.green).into_any_element(),
        ),
        AgentStatus::Failed => (
            "Failed",
            icon(Lucide::CircleAlert)
                .text_color(p.red)
                .into_any_element(),
        ),
    };
    let detail = match status {
        AgentStatus::NeedsInput => session
            .approvals
            .first()
            .map(|a| {
                format!(
                    "Needs input · wants to run {}",
                    a.input["command"].as_str().unwrap_or(&a.tool_name)
                )
            })
            .unwrap_or_else(|| "Needs input".into()),
        AgentStatus::Failed => format!(
            "Failed · {}",
            session
                .items()
                .iter()
                .rev()
                .find_map(|i| match i {
                    crate::transcript::Item::Error { text, .. } => Some(text.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "stopped on an error".into())
        ),
        _ => format!(
            "{state} · {calls} tool calls · {}",
            format_duration(elapsed)
        ),
    };
    let needs_input = status == AgentStatus::NeedsInput;
    let child_entity = child.clone();

    h_flex()
        .id(("agent-card", child.entity_id().as_u64()))
        .ml(px(GUTTER))
        .gap_3()
        .px_3()
        .py(px(10.))
        .rounded(px(8.))
        .border_1()
        .border_color(if needs_input || selected {
            p.blue
        } else {
            p.surface0
        })
        .bg(if selected {
            mix(p.blue, p.base, 0.08)
        } else {
            p.mantle
        })
        .cursor_pointer()
        .hover(|s| s.border_color(p.surface1))
        .child(
            div()
                .flex_none()
                .size(px(30.))
                .rounded(px(6.))
                .bg(p.surface0)
                .flex()
                .items_center()
                .justify_center()
                .child(lead),
        )
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(div().text_size(px(13.5)).font_medium().child(title))
                .child(
                    div()
                        .text_size(px(12.5))
                        .text_color(p.subtext0)
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .child(SharedString::from(detail)),
                ),
        )
        .when(needs_input, |this| {
            let child = child_entity.clone();
            this.child(
                Button::new(("review", child.entity_id().as_u64()))
                    .small()
                    .label("Review")
                    .on_click(cx.listener(move |view, _, window, cx| {
                        view.open_session(child.clone(), false, window, cx)
                    })),
            )
        })
        .when(!needs_input && status == AgentStatus::Working, |this| {
            this.child(
                h_flex()
                    .gap_1()
                    .text_size(px(12.))
                    .text_color(p.subtext0)
                    .child(gpui_kit::component::kbd::Kbd::new(
                        Keystroke::parse("secondary").unwrap_or_default(),
                    ))
                    .child("click to split"),
            )
        })
        .child(icon(IconName::ChevronRight).text_color(p.subtext0))
        .on_click(cx.listener(move |view, event: &ClickEvent, window, cx| {
            view.open_session(
                child_entity.clone(),
                event.modifiers().secondary(),
                window,
                cx,
            )
        }))
        .into_any_element()
}

#[derive(Clone, Copy, PartialEq)]
enum LineKind {
    Add,
    Delete,
    Context,
}

struct Line {
    kind: LineKind,
    /// The new file's number; removed lines show the old number.
    number: Option<usize>,
    text: String,
}

struct Hunk {
    header: String,
    lines: Vec<Line>,
}

impl Hunk {
    fn changed(&self) -> usize {
        self.lines
            .iter()
            .filter(|l| l.kind != LineKind::Context)
            .count()
    }
}

fn parse_hunks(patch: &str) -> Vec<Hunk> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let (mut old, mut new) = (0usize, 0usize);
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("@@") {
            let mut parts = rest.split_whitespace();
            let parse = |s: Option<&str>| {
                s.and_then(|s| s[1..].split(',').next())
                    .and_then(|n| n.parse::<usize>().ok())
                    .unwrap_or(1)
            };
            old = parse(parts.next());
            new = parse(parts.next());
            hunks.push(Hunk {
                header: line.to_string(),
                lines: Vec::new(),
            });
            continue;
        }
        let Some(hunk) = hunks.last_mut() else {
            continue;
        };
        if line.starts_with("+++") || line.starts_with("---") || line.starts_with('\\') {
            continue;
        }
        let (kind, text) = match line.chars().next() {
            Some('+') => (LineKind::Add, &line[1..]),
            Some('-') => (LineKind::Delete, &line[1..]),
            Some(' ') => (LineKind::Context, &line[1..]),
            _ => (LineKind::Context, line),
        };
        let number = match kind {
            LineKind::Add => {
                new += 1;
                Some(new - 1)
            }
            LineKind::Delete => {
                old += 1;
                Some(old - 1)
            }
            LineKind::Context => {
                old += 1;
                new += 1;
                Some(new - 1)
            }
        };
        hunk.lines.push(Line {
            kind,
            number,
            text: text.to_string(),
        });
    }
    hunks
}

#[cfg(test)]
mod tests {
    use super::parse_hunks;

    #[test]
    fn numbers_mini_diff_lines() {
        let patch = "--- a/x\n+++ b/x\n@@ -12,4 +12,5 @@ pub struct Worktree\n     pub path: PathBuf,\n-    pub status: Option<String>,\n+    pub base: String,\n+    pub diff: Entity<DiffState>,\n }\n";
        let hunks = parse_hunks(patch);
        assert_eq!(hunks.len(), 1);
        let numbers: Vec<_> = hunks[0].lines.iter().map(|l| l.number.unwrap()).collect();
        assert_eq!(numbers, [12, 13, 13, 14, 15]);
        assert_eq!(hunks[0].changed(), 3);
    }
}
