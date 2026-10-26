//! The card that opens beside a worktree or sub-agent in the collapsed rail:
//! what the agent is doing, so a glance at the rail is enough.
//!
//! The card is built while the sidebar renders, so it follows the session
//! live; while an agent works its spinner redraws the window anyway.

use gpui_kit::component::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::hover_card::HoverCard;
use gpui_kit::component::shimmer::ShimmerText;
use gpui_kit::component::{Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::{Duration, Instant};

use super::app::TreehouseApp;
use super::items::describe;
use super::{icon, status_dot};
use crate::session::{AgentStatus, Session, SessionKind};
use crate::theme::{MONO, palette};
use crate::transcript::{Item, ToolStatus, format_duration};
use crate::workspace::Workspace;

/// "just now", "12m ago", "3h ago", "2d ago".
fn ago(since: Instant) -> String {
    let secs = since.elapsed().as_secs();
    match secs {
        0..60 => "just now".to_string(),
        60..3600 => format!("{}m ago", secs / 60),
        3600..86400 => format!("{}h ago", secs / 3600),
        _ => format!("{}d ago", secs / 86400),
    }
}

/// A reply's Markdown as one line of plain text, for a two-line excerpt.
fn plain(markdown: &str) -> String {
    let text: String = markdown
        .lines()
        .filter(|line| !line.trim_start().starts_with("```") && !line.contains("---"))
        .map(|line| {
            line.trim()
                .trim_start_matches(['#', '>', '-', '*', '|'])
                .trim()
        })
        .filter(|line| !line.is_empty())
        .collect::<Vec<_>>()
        .join(" ");
    text.replace(['*', '`', '|'], "")
}

/// Wraps a sidebar or rail row so hovering it opens the agent's status card.
pub fn wrap(
    id: impl Into<ElementId>,
    trigger: impl IntoElement + 'static,
    session: Entity<Session>,
    workspace: Entity<Workspace>,
    app: WeakEntity<TreehouseApp>,
) -> HoverCard {
    HoverCard::new(id)
        .anchor(Anchor::LeftCenter)
        .appearance(false)
        .open_delay(Duration::from_millis(400))
        .trigger(trigger)
        .on_open_change({
            // A chat restored after a restart is read in when it's first
            // needed; the card needs it too.
            let session = session.clone();
            move |open, _, cx| {
                if *open {
                    session.update(cx, |session, cx| session.restore_history(cx));
                }
            }
        })
        .content(move |_, _, cx| render(&session, &workspace, app.clone(), cx))
}

fn render(
    session: &Entity<Session>,
    workspace: &Entity<Workspace>,
    app: WeakEntity<TreehouseApp>,
    cx: &App,
) -> AnyElement {
    let p = palette(cx);
    let s = session.read(cx);
    let status = s.status();
    let workspace = workspace.read(cx);
    let worktree = workspace.worktree_of(session.entity_id());
    let project = worktree.and_then(|w| workspace.project_of(&w.path));

    let (name, project_name): (SharedString, Option<SharedString>) = match &s.kind {
        SessionKind::SubAgent { title, .. } => (title.clone(), None),
        SessionKind::Worktree => (
            worktree.map(|w| w.branch.clone()).unwrap_or_default(),
            project.map(|p| p.name.clone()),
        ),
    };
    let approval = s.approvals.first();
    let state = match status {
        AgentStatus::Working => s
            .transcript_run_started()
            .map(|t| format_duration(t.elapsed()))
            .unwrap_or_else(|| "Working".into()),
        AgentStatus::NeedsInput => "Needs input".into(),
        AgentStatus::Failed => "Failed".into(),
        AgentStatus::Finished => match s.transcript_settled_at() {
            Some(at) => format!("Finished {}", ago(at)),
            None => "Finished".into(),
        },
        AgentStatus::Idle => match s.transcript_settled_at() {
            Some(at) => format!("Last active {}", ago(at)),
            None => "Idle".into(),
        },
    };

    let running_tool = s.items().iter().rev().find_map(|item| match item {
        Item::Tool(tool) if matches!(tool.status, ToolStatus::Running | ToolStatus::Preparing) => {
            Some(tool)
        }
        _ => None,
    });
    let excerpt = s
        .last_assistant_text()
        .map(|text| plain(&text))
        .filter(|text| !text.is_empty());
    let error = s.items().iter().rev().find_map(|item| match item {
        Item::Error { text, .. } => text.lines().next().map(str::to_string),
        _ => None,
    });
    let excerpt_line = |text: String| {
        div()
            .w_full()
            .min_w_0()
            .text_ellipsis()
            .text_size(px(13.))
            .text_color(p.subtext1)
            .line_clamp(2)
            .child(text)
    };

    let mut body = v_flex().gap(px(6.)).px_3().py(px(10.));
    match status {
        AgentStatus::Working => {
            if let Some(tool) = running_tool {
                let mut text = describe(tool);
                if tool.name == "subagent" {
                    text.running = "Waiting on";
                    text.target = tool.args["title"].as_str().unwrap_or("a sub-agent").into();
                }
                body = body.child(
                    h_flex()
                        .gap_2()
                        .min_w_0()
                        .text_size(px(12.5))
                        .text_color(p.subtext0)
                        .child(icon(text.icon).size(px(14.)))
                        .child(div().flex_none().child(ShimmerText::new(text.running)))
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .font_family(MONO)
                                .text_size(px(12.))
                                .text_color(p.subtext1)
                                .child(text.target),
                        ),
                );
            }
            if let Some(text) = excerpt {
                body = body.child(excerpt_line(text));
            } else if running_tool.is_none() {
                body = body.child(excerpt_line("Thinking…".into()));
            }
        }
        AgentStatus::NeedsInput => {
            let wants: SharedString = match approval {
                Some(a) if a.tool_name == "bash" => a.input["command"]
                    .as_str()
                    .map(super::items::one_line)
                    .unwrap_or_else(|| "a command".to_string())
                    .into(),
                Some(a) => a.tool_name.clone().into(),
                None => "Pi is waiting for you".into(),
            };
            let session = session.clone();
            body = body
                .child(
                    h_flex()
                        .gap_2()
                        .min_w_0()
                        .text_size(px(12.5))
                        .text_color(p.subtext0)
                        .child(div().flex_none().child(match approval {
                            Some(a) if a.tool_name == "bash" => "Wants to run",
                            Some(_) => "Wants to use",
                            None => "Sign-in",
                        }))
                        .child(
                            div()
                                .min_w_0()
                                .overflow_hidden()
                                .text_ellipsis()
                                .whitespace_nowrap()
                                .font_family(MONO)
                                .text_size(px(12.))
                                .text_color(p.text)
                                .child(wants),
                        ),
                )
                .child(
                    h_flex().child(
                        Button::new("status-review")
                            .primary()
                            .small()
                            .label("Review")
                            .on_click(move |_, window, cx| {
                                let session = session.clone();
                                let _ = app.update(cx, |app, cx| {
                                    app.open_session(session, false, window, cx)
                                });
                            }),
                    ),
                );
        }
        AgentStatus::Failed => {
            body = body.child(
                div()
                    .text_size(px(13.))
                    .text_color(p.red)
                    .w_full()
                    .min_w_0()
                    .text_ellipsis()
                    .line_clamp(2)
                    .child(error.unwrap_or_else(|| "Stopped on an error".into())),
            );
        }
        AgentStatus::Finished | AgentStatus::Idle => {
            body = body.child(match excerpt {
                Some(text) => excerpt_line(text),
                None => div()
                    .text_size(px(13.))
                    .text_color(p.subtext0)
                    .child("No chat yet."),
            });
        }
    }

    let footer_left: AnyElement = match (&s.kind, status, approval) {
        (_, AgentStatus::NeedsInput, Some(a)) => {
            format!("Waiting {}", format_duration(a.asked.elapsed())).into_any_element()
        }
        (SessionKind::SubAgent { .. }, ..) => format!(
            "Sub-agent of {}",
            worktree
                .map(|w| w.branch.as_ref())
                .unwrap_or("its worktree")
        )
        .into_any_element(),
        (SessionKind::Worktree, ..) => match worktree.map(|w| w.stat) {
            Some(stat) if stat.files > 0 => h_flex()
                .gap_2()
                .child(
                    div()
                        .text_color(p.add)
                        .child(format!("+{}", stat.additions)),
                )
                .child(
                    div()
                        .text_color(p.delete)
                        .child(format!("−{}", stat.deletions)),
                )
                .child(format!(
                    "{} {}",
                    stat.files,
                    if stat.files == 1 { "file" } else { "files" }
                ))
                .into_any_element(),
            _ => "No changes".into_any_element(),
        },
    };
    let footer_right = match &s.kind {
        SessionKind::SubAgent { .. } => {
            let calls = s.tool_call_count();
            format!("{calls} tool {}", if calls == 1 { "call" } else { "calls" })
        }
        SessionKind::Worktree => "Click to open".to_string(),
    };

    v_flex()
        .id("status-card")
        // Clear of the sidebar's border: rows end 6px inside it.
        .ml(px(14.))
        .w(px(300.))
        .rounded(px(8.))
        .border_1()
        .border_color(if status == AgentStatus::NeedsInput {
            p.blue
        } else {
            p.surface1
        })
        .bg(p.base)
        .text_color(p.text)
        .shadow_lg()
        .overflow_hidden()
        .child(
            h_flex()
                .gap_2()
                .px_3()
                .py(px(9.))
                .border_b_1()
                .border_color(p.surface0)
                .text_size(px(13.))
                .child(status_dot(status, cx))
                .child(
                    div()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .font_medium()
                        .child(name),
                )
                .when_some(project_name, |this, name| {
                    this.child(div().flex_none().text_color(p.subtext0).child(name))
                })
                .child(div().flex_1())
                .child(
                    div()
                        .flex_none()
                        .text_size(px(12.))
                        .text_color(p.subtext0)
                        .child(state),
                ),
        )
        .child(body)
        .child(
            h_flex()
                .gap_2()
                .px_3()
                .py(px(7.))
                .border_t_1()
                .border_color(p.surface0)
                .bg(p.mantle)
                .text_size(px(12.))
                .text_color(p.subtext0)
                .child(footer_left)
                .child(div().flex_1())
                .child(footer_right),
        )
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::plain;

    #[test]
    fn plain_text_from_markdown() {
        let md = "## Done\n\nI fixed **the scroll** in `sidebar.rs`.\n\n```rust\nlet x = 1;\n```\n- kept the tests";
        assert_eq!(
            plain(md),
            "Done I fixed the scroll in sidebar.rs. let x = 1; kept the tests"
        );
    }
}
