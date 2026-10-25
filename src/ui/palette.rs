//! The command palette (⌘K), built on gpui-kit's `Command` in a dialog.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, WindowExt as _, h_flex};
use gpui_kit::*;
use std::rc::Rc;

use super::app::TreehouseApp;
use super::icon;
use crate::actions::*;
use crate::session::Session;
use crate::theme::{palette, toggle_label};
use crate::workspace::Workspace;

struct Entry {
    label: SharedString,
    icon: Icon,
    action: Box<dyn Action>,
    /// Words it's found by besides its label.
    keywords: &'static [&'static str],
}

fn entry(label: impl Into<SharedString>, icon_name: impl Into<Icon>, action: impl Action) -> Entry {
    Entry {
        label: label.into(),
        icon: icon(icon_name),
        action: Box::new(action),
        keywords: &[],
    }
}

impl Entry {
    fn keywords(mut self, keywords: &'static [&'static str]) -> Self {
        self.keywords = keywords;
        self
    }
}

fn groups(cx: &App) -> Vec<(&'static str, Vec<Entry>)> {
    vec![
        (
            "Workspace",
            vec![
                entry("Open project…", IconName::FolderOpen, OpenProject),
                entry("New worktree", Lucide::GitBranch, NewWorktree),
                entry(
                    "Go to worktree or sub-agent…",
                    IconName::ChevronsUpDown,
                    SwitchWorktree,
                ),
            ],
        ),
        (
            "View",
            vec![
                entry("Show diff", Lucide::PanelRight, ShowDiff),
                entry("Open right dock", Lucide::Columns2, ToggleDock),
                entry("Full-screen right dock", Lucide::Maximize2, FullScreenDock),
                entry("Toggle sidebar", IconName::PanelLeft, ToggleSidebar),
                entry("Toggle full screen", Lucide::Expand, ToggleFullScreen),
                entry(
                    toggle_label(cx),
                    if cx.theme().is_dark() {
                        IconName::Sun
                    } else {
                        IconName::Moon
                    },
                    ToggleTheme,
                )
                .keywords(&["theme", "dark", "light", "mocha", "latte"]),
            ],
        ),
        (
            "Agent",
            vec![
                entry("Change model…", Lucide::Sparkles, ChangeModel),
                entry("Set thinking level…", Lucide::Lightbulb, SetThinkingLevel),
                entry("Stop agent", Lucide::Square, StopAgent),
            ],
        ),
        (
            "App",
            vec![
                entry("Settings", IconName::Settings, OpenSettings),
                entry("About Treehouse", IconName::Info, About),
            ],
        ),
    ]
}

/// Opens the palette over the window.
pub fn open(state: &Entity<CommandState>, window: &mut Window, cx: &mut App) {
    let dialog_state = state.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        let state = dialog_state.clone();
        dialog
            .close_button(false)
            .w(px(560.))
            .p_0()
            .margin_top(px(96.))
            .content(move |content, window, cx| content.child(build(&state, window, cx)))
    });
    focus_fresh(state, window, cx);
}

/// Starts the search empty and focused; the dialog doesn't focus it itself.
fn focus_fresh(state: &Entity<CommandState>, window: &mut Window, cx: &mut App) {
    let state = state.clone();
    window.defer(cx, move |window, cx| {
        state.update(cx, |state, cx| {
            state.set_query("", window, cx);
            state.focus(window, cx);
        });
    });
}

/// How well `query` matches `text` as a fuzzy subsequence, or `None`.
/// Consecutive letters and letters starting a word score higher, and so do
/// shorter texts, so "fdv" finds "feat/diff-view" first.
pub fn fuzzy_score(query: &str, text: &str) -> Option<i32> {
    let text: Vec<char> = text.to_lowercase().chars().collect();
    let mut score = 0;
    let mut at = 0;
    let mut previous: Option<usize> = None;
    for q in query.to_lowercase().chars().filter(|c| !c.is_whitespace()) {
        let found = (at..text.len()).find(|&i| text[i] == q)?;
        score += 1;
        if previous == Some(found.wrapping_sub(1)) {
            score += 5;
        }
        if found == 0 || !text[found - 1].is_alphanumeric() {
            score += 3;
        }
        previous = Some(found);
        at = found + 1;
    }
    Some(score * 100 - text.len() as i32)
}

struct Target {
    session: Entity<Session>,
    title: SharedString,
    detail: String,
    status: crate::session::AgentStatus,
}

/// Every worktree and sub-agent, for the switcher.
fn targets(workspace: &Entity<Workspace>, cx: &App) -> Vec<Target> {
    let mut targets = Vec::new();
    for project in &workspace.read(cx).projects {
        for worktree in &project.worktrees {
            targets.push(Target {
                session: worktree.session.clone(),
                title: worktree.branch.clone(),
                detail: project.name.to_string(),
                status: worktree.session.read(cx).status(),
            });
            for subagent in &worktree.subagents {
                let session = subagent.session.read(cx);
                targets.push(Target {
                    session: subagent.session.clone(),
                    title: session.title(),
                    detail: format!("sub-agent of {} · {}", worktree.branch, project.name),
                    status: session.status(),
                });
            }
        }
    }
    targets
}

/// The switcher (⌘P): fuzzy-find a worktree or sub-agent and open it.
pub fn open_switcher(
    state: &Entity<CommandState>,
    workspace: Entity<Workspace>,
    app: WeakEntity<TreehouseApp>,
    window: &mut Window,
    cx: &mut App,
) {
    let dialog_state = state.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        let state = dialog_state.clone();
        let workspace = workspace.clone();
        let app = app.clone();
        dialog
            .close_button(false)
            .w(px(560.))
            .p_0()
            .margin_top(px(96.))
            .content(move |content, _, cx| {
                content.child(build_switcher(&state, &workspace, app.clone(), cx))
            })
    });
    focus_fresh(state, window, cx);
}

fn build_switcher(
    state: &Entity<CommandState>,
    workspace: &Entity<Workspace>,
    app: WeakEntity<TreehouseApp>,
    cx: &mut App,
) -> impl IntoElement {
    let p = palette(cx);
    let query = state.read(cx).query(cx);
    let mut targets: Vec<(i32, Target)> = targets(workspace, cx)
        .into_iter()
        .filter_map(|t| {
            if query.trim().is_empty() {
                return Some((0, t));
            }
            // The title counts most; the project and parent help narrow it.
            let title = fuzzy_score(&query, &t.title).map(|s| s + 1000);
            let all = fuzzy_score(&query, &format!("{} {}", t.title, t.detail));
            title.or(all).map(|score| (score, t))
        })
        .collect();
    // Stable, so equal scores keep the sidebar's order.
    targets.sort_by_key(|t| std::cmp::Reverse(t.0));
    let sessions: Rc<Vec<Entity<Session>>> =
        Rc::new(targets.iter().map(|(_, t)| t.session.clone()).collect());

    let mut command = Command::new(state)
        .bordered(false)
        .filterable(false)
        .max_h(px(480.))
        .placeholder("Go to a worktree or sub-agent…");
    for (_, target) in targets {
        let dot = target.status;
        let title = target.title.clone();
        let detail = SharedString::from(target.detail);
        command = command.item(CommandItem::new().label(target.title).child(move |_, cx| {
            h_flex()
                .w_full()
                .gap(px(10.))
                .child(
                    div()
                        .w(px(16.))
                        .flex()
                        .justify_center()
                        .child(super::status_dot(dot, cx)),
                )
                .child(div().flex_none().child(title.clone()))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .overflow_hidden()
                        .text_ellipsis()
                        .whitespace_nowrap()
                        .text_color(p.subtext0)
                        .child(detail.clone()),
                )
        }));
    }
    let open = {
        let sessions = sessions.clone();
        move |row: usize, in_dock: bool, window: &mut Window, cx: &mut App| {
            let Some(session) = sessions.get(row).cloned() else {
                return;
            };
            window.close_dialog(cx);
            let app = app.clone();
            window.defer(cx, move |window, cx| {
                let _ = app.update(cx, |app, cx| app.open_session(session, in_dock, window, cx));
            });
        }
    };
    let open_in_dock = open.clone();
    let state = state.clone();
    let key = |keys: &str| Kbd::new(Keystroke::parse(keys).unwrap_or_default());
    gpui_kit::component::v_flex()
        .on_action(move |_: &OpenInDock, window, cx| {
            if let Some(index) = state.read(cx).selected_index() {
                open_in_dock(index.row, true, window, cx);
            }
        })
        .child(command.on_confirm(move |index, window, cx| open(index.row, false, window, cx)))
        .child(
            h_flex()
                .gap_2()
                .px_3()
                .py_2()
                .border_t_1()
                .border_color(p.surface0)
                .text_size(px(12.))
                .text_color(p.subtext0)
                .child(key("enter"))
                .child("open")
                .child(div().w_2())
                .child(key("secondary-enter"))
                .child("open in the right dock"),
        )
}

fn build(state: &Entity<CommandState>, window: &mut Window, cx: &mut App) -> Command {
    let groups = groups(cx);
    let actions: Rc<Vec<Vec<Box<dyn Action>>>> = Rc::new(
        groups
            .iter()
            .map(|(_, entries)| entries.iter().map(|e| e.action.boxed_clone()).collect())
            .collect(),
    );
    let p = palette(cx);
    let mut command = Command::new(state)
        .bordered(false)
        .max_h(px(600.))
        .placeholder("Type a command or search…");
    for (ix, (label, entries)) in groups.into_iter().enumerate() {
        if ix > 0 {
            command = command.separator();
        }
        let mut group = CommandGroup::new().label(label);
        for entry in entries {
            let kbd = Kbd::global_binding_for_action(entry.action.as_ref(), window);
            let row_label = entry.label.clone();
            let row_icon = entry.icon.clone();
            let item = CommandItem::new()
                .label(entry.label)
                .keywords(entry.keywords.iter().copied());
            group = group.item(item.child(move |_, _| {
                h_flex()
                    .w_full()
                    .gap(px(10.))
                    .child(row_icon.clone().text_color(p.subtext0))
                    .child(div().flex_1().child(row_label.clone()))
                    .children(kbd.clone())
            }));
        }
        command = command.group(group);
    }
    command.on_confirm(move |index, window, cx| {
        let Some(action) = actions.get(index.section).and_then(|g| g.get(index.row)) else {
            return;
        };
        let action = action.boxed_clone();
        window.close_dialog(cx);
        window.defer(cx, move |window, cx| window.dispatch_action(action, cx));
    })
}

#[cfg(test)]
mod tests {
    use super::fuzzy_score;

    #[test]
    fn fuzzy_finds_and_ranks() {
        assert!(fuzzy_score("fdv", "feat/diff-view").is_some());
        assert!(fuzzy_score("xyz", "feat/diff-view").is_none());
        // Word starts and runs beat scattered letters.
        assert!(fuzzy_score("diff", "feat/diff-view") > fuzzy_score("diff", "dark fifth fork"));
        assert!(fuzzy_score("main", "main") > fuzzy_score("main", "maintenance"));
    }
}
