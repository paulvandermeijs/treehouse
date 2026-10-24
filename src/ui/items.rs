//! How each piece of agent output looks in the chat.
//!
//! One text column: the user's messages carry an avatar, Pi's don't. Icons sit
//! in a 34px gutter, so text, cards and code blocks start on the same line.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::shimmer::ShimmerText;
use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::text::TextView;
use gpui_kit::component::{IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::component::StyledExt as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::chat::ChatView;
use super::icon;
use crate::session::{AgentStatus, ApprovalChoice, PendingApproval, Session};
use crate::theme::{MONO, Palette, mix, palette};
use crate::transcript::{Item, ItemId, ToolCall, ToolStatus, format_duration};

pub const GUTTER: f32 = 34.;

/// Renders the transcript, grouping three or more finished reads in a row.
pub fn render_items(view: &ChatView, cx: &mut Context<ChatView>) -> Vec<AnyElement> {
    let session = view.session.clone();
    let items: Vec<Item> = session.read(cx).items().to_vec();
    let mut out = Vec::new();
    let mut ix = 0;
    while ix < items.len() {
        if let Item::Tool(tool) = &items[ix] {
            if tool.name == "read" && tool.status == ToolStatus::Done {
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
            .child(avatar(&view.initials, &p))
            .child(div().flex_1().min_w_0().child(SharedString::from(text.clone())))
            .into_any_element(),
        Item::Text { id, text, .. } => {
            let body = match view.session.read(cx).text_view(*id) {
                Some(state) => TextView::new(state).stream_fade(true).into_any_element(),
                None => TextView::markdown(("text", id.0), SharedString::from(text.clone())).into_any_element(),
            };
            div().pl(px(GUTTER)).child(body).into_any_element()
        }
        Item::Thinking { id, text, streaming, elapsed, .. } => render_thinking(view, *id, text, *streaming, *elapsed, cx),
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
        Item::Error { id, text, retrying } => h_flex()
            .id(("error", id.0))
            .ml(px(GUTTER))
            .gap_2()
            .px_3()
            .py_2()
            .rounded(px(8.))
            .border_1()
            .border_color(mix(p.red, p.base, 0.4))
            .bg(mix(p.red, p.base, 0.1))
            .text_size(px(13.))
            .child(icon(Lucide::CircleAlert).text_color(p.red))
            .child(div().flex_1().child(SharedString::from(text.clone())))
            .when(*retrying, |this| this.child(Spinner::new().xsmall().color(p.subtext0)))
            .into_any_element(),
    }
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
    div().font_family(MONO).text_size(px(12.)).text_color(color).child(text.into())
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
        let secs = elapsed.map(|e| e.as_secs().max(1)).unwrap_or(1);
        SharedString::from(format!("Thought for {secs}s")).into_any_element()
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

struct ToolText {
    icon: Lucide,
    running: &'static str,
    done: &'static str,
    target: String,
    result: Option<String>,
}

fn describe(tool: &ToolCall) -> ToolText {
    let path = tool.path().unwrap_or("").to_string();
    match tool.name.as_str() {
        "read" => {
            let range = match (tool.args["offset"].as_u64(), tool.args["limit"].as_u64()) {
                (Some(offset), Some(limit)) => Some(format!("lines {offset}–{}", offset + limit - 1)),
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
            done: if tool.previous_content.is_some() { "Rewrote" } else { "Created" },
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
        other => ToolText {
            icon: Lucide::Wrench,
            running: "Running",
            done: "Used",
            target: other.to_string(),
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
        .when(additions > 0, |this| this.child(div().text_color(p.add).child(format!("+{additions}"))))
        .when(deletions > 0, |this| this.child(div().text_color(p.delete).child(format!("−{deletions}"))))
}

fn render_tool(view: &ChatView, tool: &ToolCall, cx: &mut Context<ChatView>) -> AnyElement {
    let p = palette(cx);
    let text = describe(tool);
    let id = tool.id;
    let patch = tool.patch();
    let stat = patch.as_deref().map(crate::git::diff_stat);
    let output_lines = tool.output.lines().count();
    let running = matches!(tool.status, ToolStatus::Running | ToolStatus::Preparing);
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
        ToolStatus::Running | ToolStatus::Preparing => ShimmerText::new(text.running).into_any_element(),
        ToolStatus::Denied => "Didn't run".into_any_element(),
        ToolStatus::Failed if tool.name == "edit" => "Edit failed".into_any_element(),
        ToolStatus::AwaitingApproval => "Wants to run".into_any_element(),
        _ => text.done.into_any_element(),
    };
    let lead: AnyElement = if running {
        div().ml(px(4.)).mr(px(8.)).child(Spinner::new().with_size(px(14.)).color(p.subtext1)).into_any_element()
    } else {
        div()
            .ml(px(3.))
            .mr(px(7.))
            .child(icon(text.icon).text_color(if failed { p.red } else { p.subtext0 }))
            .into_any_element()
    };

    let result: Option<AnyElement> = match (tool.name.as_str(), tool.status) {
        (_, ToolStatus::Denied) => Some(div().child("denied").into_any_element()),
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
                    .child(
                        div()
                            .px(px(6.))
                            .rounded(px(4.))
                            .text_size(px(11.5))
                            .font_family(MONO)
                            .bg(mix(if ok { p.green } else { p.red }, p.base, 0.16))
                            .text_color(if ok { p.add } else { p.delete })
                            .child(if timed_out { "timed out".to_string() } else { format!("exit {code}") }),
                    )
                    .when_some(tool.duration, |this, d| this.child(format_duration(d)))
                    .into_any_element(),
            )
        }
        ("bash", ToolStatus::Running) => Some(div().child(format_duration(tool.started.elapsed())).into_any_element()),
        (_, ToolStatus::Failed) => Some(
            div()
                .text_color(p.red)
                .child(SharedString::from(first_line(&tool.output).to_string()))
                .into_any_element(),
        ),
        ("edit" | "write", ToolStatus::Done) => stat.map(|s| counts(s.additions, s.deletions, &p).into_any_element()),
        _ => text.result.clone().map(|r| div().child(r).into_any_element()),
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
        .child(verb)
        .child(
            mono(text.target.clone(), if failed { p.red } else { p.subtext1 })
                .min_w_0()
                .overflow_hidden()
                .text_ellipsis()
                .whitespace_nowrap(),
        )
        .when_some(result, |this, result| this.child(div().flex_none().text_size(px(12.5)).child(result)))
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
            _ if !tool.output.is_empty() && failed => this.child(error_card(&tool.output, &p)),
            _ => this,
        })
        .into_any_element()
}

fn first_line(text: &str) -> &str {
    text.lines().find(|l| !l.trim().is_empty()).unwrap_or("").trim()
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

fn render_bash_output(tool: &ToolCall, p: &Palette) -> Div {
    let running = tool.status == ToolStatus::Running;
    let failed = tool.status == ToolStatus::Failed;
    let lines: Vec<&str> = tool
        .output
        .lines()
        .filter(|l| !l.starts_with("Command exited with code "))
        .collect();
    let keep = if running { 5 } else if failed { 12 } else { 40 };
    let hidden = lines.len().saturating_sub(keep);
    let shown = lines[hidden..].join("\n");
    let command = tool.command().unwrap_or("");

    card(p)
        .when(failed, |this| this.border_color(mix(p.red, p.base, 0.5)))
        .child(
            v_flex()
                .px_3()
                .py_2()
                .font_family(MONO)
                .text_size(px(12.))
                .line_height(px(19.))
                .child(div().text_color(p.subtext0).child(SharedString::from(format!("$ {command}"))))
                .when(hidden > 0, |this| {
                    this.child(div().text_color(p.subtext0).child(format!("{hidden} earlier lines")))
                })
                .child(div().whitespace_normal().child(SharedString::from(shown))),
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

fn render_mini_diff(view: &ChatView, tool: &ToolCall, patch: &str, cx: &mut Context<ChatView>) -> AnyElement {
    let p = palette(cx);
    let hunks = parse_hunks(patch);
    let created = tool.name == "write" && tool.previous_content.is_none();
    let total_changed: usize = hunks.iter().map(|h| h.changed()).sum();
    let limit = if created { 6 } else if total_changed <= 10 { usize::MAX } else { 12 };
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
                    .child(div().w(px(40.)).pr_2().text_right().text_color(fg).child(line.number.map(|n| n.to_string()).unwrap_or_default()))
                    .child(div().w(px(18.)).text_center().text_color(fg).child(sign))
                    .child(div().flex_1().min_w_0().overflow_hidden().child(SharedString::from(line.text.clone())))
                    .into_any_element(),
            );
        }
    }
    let total_lines: usize = hunks.iter().map(|h| h.lines.len()).sum();
    let more = if created {
        total_lines.saturating_sub(shown).max(0)
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
                                move |view, _, window, cx| view.open_diff_at(worktree.clone(), path.clone(), window, cx)
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

fn render_approval(view: &ChatView, approval: &PendingApproval, cx: &mut Context<ChatView>) -> AnyElement {
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
    let what = if approval.tool_name == "bash" { "a command" } else { "a tool" };

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
                .flex_wrap()
                .child(
                    Button::new(("approve", approval.tool_call_id.len()))
                        .primary()
                        .small()
                        .label("Run ⏎")
                        .on_click({
                            let (session, id) = (session.clone(), request_id.clone());
                            move |_, _, cx| session.update(cx, |s, cx| s.resolve_approval(&id, ApprovalChoice::Run, cx))
                        }),
                )
                .when_some(approval.pattern.clone(), |this, pattern| {
                    let (session, id) = (session.clone(), request_id.clone());
                    this.child(
                        Button::new("always-allow")
                            .small()
                            .label(format!("Always allow {pattern}"))
                            .on_click(move |_, _, cx| {
                                let pattern = pattern.clone();
                                session.update(cx, |s, cx| s.resolve_approval(&id, ApprovalChoice::AlwaysAllow(pattern), cx))
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
                                    s.resolve_approval(&id, ApprovalChoice::Deny("Denied by the user".into()), cx)
                                })
                            }
                        }),
                ),
        )
        .into_any_element()
}

fn render_read_group(view: &ChatView, reads: &[&ToolCall], cx: &mut Context<ChatView>) -> AnyElement {
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
                .child(icon(if expanded { IconName::ChevronDown } else { IconName::ChevronRight }).size(px(14.)))
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

fn render_subagent_card(view: &ChatView, tool: &ToolCall, cx: &mut Context<ChatView>) -> AnyElement {
    let p = palette(cx);
    let workspace = view.workspace.read(cx);
    let child = workspace.subagent_for_call(&tool.call_id).map(|s| s.session.clone());
    let title = tool.args["title"].as_str().unwrap_or("Sub-agent").to_string();
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
    let elapsed = tool.and_then(|t| t.duration).unwrap_or_else(|| session.started.elapsed());
    let selected = view.docked_session == Some(child.entity_id());

    let (state, lead) = match status {
        AgentStatus::Working => ("Working", Spinner::new().with_size(px(16.)).color(p.blue).into_any_element()),
        AgentStatus::NeedsInput => ("Needs input", icon(Lucide::Shield).text_color(p.blue).into_any_element()),
        AgentStatus::Finished | AgentStatus::Idle => ("Finished", icon(IconName::Check).text_color(p.green).into_any_element()),
        AgentStatus::Failed => ("Failed", icon(Lucide::CircleAlert).text_color(p.red).into_any_element()),
    };
    let detail = match status {
        AgentStatus::NeedsInput => session
            .approvals
            .first()
            .map(|a| format!("Needs input · wants to run {}", a.input["command"].as_str().unwrap_or(&a.tool_name)))
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
        _ => format!("{state} · {calls} tool calls · {}", format_duration(elapsed)),
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
        .border_color(if needs_input || selected { p.blue } else { p.surface0 })
        .bg(if selected { mix(p.blue, p.base, 0.08) } else { p.mantle })
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
                    .on_click(cx.listener(move |view, _, window, cx| view.open_session(child.clone(), false, window, cx))),
            )
        })
        .when(!needs_input && status == AgentStatus::Working, |this| {
            this.child(
                h_flex()
                    .gap_1()
                    .text_size(px(12.))
                    .text_color(p.subtext0)
                    .child(gpui_kit::component::kbd::Kbd::new(Keystroke::parse("secondary").unwrap_or_default()))
                    .child("click to split"),
            )
        })
        .child(icon(IconName::ChevronRight).text_color(p.subtext0))
        .on_click(cx.listener(move |view, event: &ClickEvent, window, cx| {
            view.open_session(child_entity.clone(), event.modifiers().secondary(), window, cx)
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
        self.lines.iter().filter(|l| l.kind != LineKind::Context).count()
    }
}

fn parse_hunks(patch: &str) -> Vec<Hunk> {
    let mut hunks: Vec<Hunk> = Vec::new();
    let (mut old, mut new) = (0usize, 0usize);
    for line in patch.lines() {
        if let Some(rest) = line.strip_prefix("@@") {
            let mut parts = rest.split_whitespace();
            let parse = |s: Option<&str>| {
                s.and_then(|s| s[1..].split(',').next()).and_then(|n| n.parse::<usize>().ok()).unwrap_or(1)
            };
            old = parse(parts.next());
            new = parse(parts.next());
            hunks.push(Hunk { header: line.to_string(), lines: Vec::new() });
            continue;
        }
        let Some(hunk) = hunks.last_mut() else { continue };
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
        hunk.lines.push(Line { kind, number, text: text.to_string() });
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
