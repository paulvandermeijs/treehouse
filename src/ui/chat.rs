//! A session's chat: header, history, message box and the Pi options bar.
//!
//! The same view is used for a worktree chat and a sub-agent, in the main
//! screen or in the right dock.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::menu::{DropdownMenu as _, PopupMenuItem};
use gpui_kit::component::{IconName, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashSet;
use std::path::PathBuf;

use super::items::{GUTTER, avatar, render_items};
use super::{BAR_HEIGHT, icon};
use crate::actions::{
    FullScreenDock, ShowDiff, SlashAccept, SlashDismiss, SlashDown, SlashUp, StopAgent,
};
use crate::config::{Config, THINKING_LEVELS};
use crate::session::{AgentStatus, Session, SessionEvent, SessionKind};
use crate::slash::{self, CommandKind, SlashCommand};
use crate::theme::{mix, palette};
use crate::transcript::{ItemId, format_tokens};
use crate::workspace::Workspace;

pub enum ChatEvent {
    /// Open a session, in the right dock when `dock` is set.
    Open {
        session: Entity<Session>,
        dock: bool,
    },
    /// Show the worktree diff, scrolled to a file when given.
    ShowDiff {
        worktree: PathBuf,
        path: Option<String>,
    },
    /// Back from a sub-agent to its worktree chat.
    Back,
    Close,
}

pub struct ChatView {
    pub session: Entity<Session>,
    pub workspace: Entity<Workspace>,
    composer: Entity<TextareaState>,
    /// Where a sign-in's redirect address is pasted.
    /// The composer's current placeholder; it changes with the folder's trust.
    placeholder: SharedString,
    pub redirect_input: Entity<gpui_kit::component::input::InputState>,
    /// Items the user expanded or collapsed against their default.
    toggled: HashSet<ItemId>,
    scroll: ScrollHandle,
    follow_tail: bool,
    pub in_dock: bool,
    /// The session open in the right dock, for card selection.
    pub docked_session: Option<EntityId>,
    pub diff_open: bool,
    pub initials: SharedString,
    pub branch: SharedString,
    /// The highlighted slash command.
    slash_selected: usize,
    /// Escape closed the slash menu; it stays closed until the text changes.
    slash_dismissed: bool,
    slash_scroll: ScrollHandle,
    _subscriptions: Vec<Subscription>,
}

impl EventEmitter<ChatEvent> for ChatView {}

impl ChatView {
    pub fn new(
        session: Entity<Session>,
        workspace: Entity<Workspace>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let placeholder = if session.read(cx).is_subagent() {
            "Steer this sub-agent…"
        } else {
            "Message Pi…"
        };
        let composer = cx.new(|cx| {
            TextareaState::new(window, cx)
                .auto_grow(2, 10)
                .submit_on_enter(true)
                .placeholder(placeholder)
        });
        let cwd = session.read(cx).cwd.clone();
        let initials = crate::git::user_name(&cwd)
            .map(|name| initials(&name))
            .unwrap_or_else(|| "You".chars().take(2).collect());
        let subscriptions = vec![
            cx.subscribe_in(
                &composer,
                window,
                |view, state, event: &InputEvent, window, cx| match event {
                    InputEvent::PressEnter { shift: false, .. } => view.send(window, cx),
                    InputEvent::Change => {
                        view.slash_selected = 0;
                        view.slash_dismissed = false;
                        // Pi lists its skills and templates once it runs.
                        let typing_command = slash::typed_name(&state.read(cx).value()).is_some();
                        if typing_command && view.session.read(cx).commands.is_empty() {
                            view.session.update(cx, |s, cx| s.load_commands(cx));
                        }
                        cx.notify();
                    }
                    _ => {}
                },
            ),
            cx.observe(&session, |view, _, cx| {
                if view.follow_tail {
                    view.scroll.scroll_to_bottom();
                }
                cx.notify();
            }),
            cx.subscribe(&session, |_, _, _: &SessionEvent, cx| cx.notify()),
            cx.observe(&workspace, |_, _, cx| cx.notify()),
        ];
        let branch = workspace
            .read(cx)
            .worktree_of(session.entity_id())
            .map(|w| w.branch.clone())
            .unwrap_or_default();
        Self {
            session,
            workspace,
            composer,
            placeholder: placeholder.into(),
            redirect_input: cx.new(|cx| {
                gpui_kit::component::input::InputState::new(window, cx)
                    .placeholder("http://127.0.0.1:…/callback?code=…")
            }),
            toggled: HashSet::new(),
            scroll: ScrollHandle::new(),
            follow_tail: true,
            in_dock: false,
            docked_session: None,
            diff_open: false,
            initials: initials.into(),
            branch,
            slash_selected: 0,
            slash_dismissed: false,
            slash_scroll: ScrollHandle::new(),
            _subscriptions: subscriptions,
        }
    }

    /// The project this chat's worktree belongs to, when Pi runs it
    /// read-only because the folder isn't trusted.
    fn untrusted_project(&self, cx: &App) -> Option<(PathBuf, SharedString)> {
        let session = self.session.read(cx);
        if session.trusted || session.is_subagent() {
            return None;
        }
        let workspace = self.workspace.read(cx);
        let worktree = workspace.worktree_of(self.session.entity_id())?;
        let project = workspace.project_of(&worktree.path)?;
        Some((project.path.clone(), project.name.clone()))
    }

    fn review_trust(&self, window: &mut Window, cx: &mut App) {
        if let Some((project, _)) = self.untrusted_project(cx) {
            super::trust::open(self.workspace.clone(), project, false, window, cx);
        }
    }

    pub fn focus_composer(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer
            .update(cx, |state, cx| state.focus(window, cx));
    }

    pub fn is_expanded(&self, id: ItemId, default: bool) -> bool {
        default != self.toggled.contains(&id)
    }

    pub fn toggle(&mut self, id: ItemId, cx: &mut Context<Self>) {
        if !self.toggled.remove(&id) {
            self.toggled.insert(id);
        }
        self.follow_tail = false;
        cx.notify();
    }

    pub fn open_session(
        &mut self,
        session: Entity<Session>,
        dock: bool,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.emit(ChatEvent::Open { session, dock });
    }

    pub fn open_diff_at(
        &mut self,
        worktree: PathBuf,
        path: String,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        cx.emit(ChatEvent::ShowDiff {
            worktree,
            path: Some(path),
        });
    }

    /// The slash commands matching what's typed; empty when the menu is shut.
    fn slash_matches(&self, cx: &App) -> Vec<SlashCommand> {
        if self.slash_dismissed {
            return Vec::new();
        }
        let value = self.composer.read(cx).value();
        let Some(query) = slash::typed_name(&value) else {
            return Vec::new();
        };
        let session = self.session.read(cx);
        // A sub-agent reports back from its session, so it can't start a new one.
        let mut commands: Vec<SlashCommand> = slash::builtins()
            .into_iter()
            .filter(|c| !(session.is_subagent() && c.name == "new"))
            .collect();
        commands.extend(session.commands.iter().cloned());
        slash::matches(&commands, query)
            .into_iter()
            .cloned()
            .collect()
    }

    fn move_slash_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let count = self.slash_matches(cx).len();
        if count == 0 {
            return;
        }
        self.slash_selected =
            (self.slash_selected as isize + delta).rem_euclid(count as isize) as usize;
        self.slash_scroll.scroll_to_item(self.slash_selected);
        cx.notify();
    }

    /// Puts the command in the message box, ready for its arguments.
    fn accept_slash(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(command) = self.slash_matches(cx).into_iter().nth(ix) else {
            return;
        };
        let text = format!("/{} ", command.name);
        self.composer.update(cx, |state, cx| {
            state.set_value("", window, cx);
            state.insert(text, window, cx);
            state.focus(window, cx);
        });
        cx.notify();
    }

    fn render_slash_menu(
        &self,
        commands: &[SlashCommand],
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = palette(cx);
        let selected = self.slash_selected.min(commands.len().saturating_sub(1));
        let loading = self.session.read(cx).commands.is_empty()
            && self.session.read(cx).start_error.is_none();
        v_flex()
            .absolute()
            .bottom_full()
            .left(px(16.))
            .right(px(16.))
            .mb_1()
            .max_w(px(560.))
            .rounded(px(10.))
            .border_1()
            .border_color(p.surface1)
            .bg(p.base)
            .shadow_lg()
            .overflow_hidden()
            .child(
                v_flex()
                    .id("slash-commands")
                    .track_scroll(&self.slash_scroll)
                    .overflow_y_scroll()
                    .max_h(px(320.))
                    .p(px(4.))
                    .children(commands.iter().enumerate().map(|(ix, command)| {
                        let (kind_icon, kind_label) = match command.kind {
                            CommandKind::Builtin => (icon(Lucide::Terminal), "Command"),
                            CommandKind::Skill => (icon(Lucide::Sparkles), "Skill"),
                            CommandKind::Template => (icon(IconName::File), "Template"),
                        };
                        h_flex()
                            .id(("slash", ix))
                            .h(px(34.))
                            .flex_none()
                            .gap(px(10.))
                            .px(px(10.))
                            .rounded(px(6.))
                            .cursor_pointer()
                            .text_size(px(13.5))
                            .when(ix == selected, |this| this.bg(p.surface0))
                            .when(ix != selected, |this| {
                                this.hover(|s| s.bg(mix(p.surface0, p.base, 0.55)))
                            })
                            .child(kind_icon.text_color(p.subtext0))
                            .child(
                                div()
                                    .flex_none()
                                    .font_family(crate::theme::MONO)
                                    .text_size(px(12.5))
                                    .child(format!("/{}", command.name)),
                            )
                            .when_some(command.argument_hint.clone(), |this, hint| {
                                this.child(
                                    div()
                                        .flex_none()
                                        .font_family(crate::theme::MONO)
                                        .text_size(px(12.5))
                                        .text_color(p.subtext0)
                                        .child(hint),
                                )
                            })
                            .child(
                                div()
                                    .flex_1()
                                    .min_w_0()
                                    .overflow_hidden()
                                    .text_ellipsis()
                                    .whitespace_nowrap()
                                    .text_size(px(12.5))
                                    .text_color(p.subtext0)
                                    .child(command.description.clone()),
                            )
                            .child(
                                div()
                                    .flex_none()
                                    .text_size(px(11.5))
                                    .text_color(p.subtext0)
                                    .child(kind_label),
                            )
                            .on_click(cx.listener(move |view, _, window, cx| {
                                view.accept_slash(ix, window, cx)
                            }))
                    })),
            )
            .child(
                h_flex()
                    .px_3()
                    .py_1()
                    .gap_3()
                    .border_t_1()
                    .border_color(p.surface0)
                    .text_size(px(11.5))
                    .text_color(p.subtext0)
                    .child("↑↓ choose · Tab or Enter to pick · Esc to close")
                    .when(loading, |this| {
                        this.child(div().flex_1()).child("Loading Pi's skills…")
                    }),
            )
    }

    fn send(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let message = self.composer.read(cx).value().to_string();
        if message.trim().is_empty() {
            // Enter on an empty message box runs the waiting command.
            let waiting = self
                .session
                .read(cx)
                .approvals
                .first()
                .map(|a| a.request_id.clone());
            if let Some(request_id) = waiting {
                self.session.update(cx, |s, cx| {
                    s.resolve_approval(&request_id, crate::session::ApprovalChoice::Run, cx)
                });
            }
            return;
        }
        self.composer
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.follow_tail = true;
        self.session
            .update(cx, |session, cx| session.send(&message, cx));
    }

    fn render_header(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let session = self.session.read(cx);
        let workspace = self.workspace.read(cx);
        let worktree = workspace.worktree_of(self.session.entity_id());
        let project = worktree.and_then(|w| workspace.project_of(&w.path));
        let is_subagent = session.is_subagent();
        let status = session.status();

        let title: SharedString = match &session.kind {
            SessionKind::SubAgent { title, .. } => title.clone(),
            SessionKind::Worktree => self.branch.clone(),
        };
        let description = match (&session.kind, worktree, project) {
            (SessionKind::SubAgent { .. }, _, _) => format!("sub-agent of {}", self.branch),
            (_, Some(w), Some(project)) if w.is_main => project.name.to_string(),
            (_, Some(w), Some(project)) => format!("{} · worktree from {}", project.name, w.base),
            _ => String::new(),
        };
        let stat = worktree.map(|w| w.stat).unwrap_or_default();
        let untrusted = self.untrusted_project(cx).is_some();

        h_flex()
            .h(px(BAR_HEIGHT))
            .flex_none()
            .gap_2()
            // The read-only button sits as close to the edge as the diff button
            // on the right.
            .pl(px(if self.in_dock || is_subagent || untrusted {
                8.
            } else {
                16.
            }))
            .pr_2()
            .border_b_1()
            .border_color(p.surface0)
            .text_size(px(13.))
            .when(self.in_dock, |this| {
                this.child(
                    Button::new("dock-full")
                        .ghost()
                        .small()
                        .icon(icon(Lucide::Maximize2))
                        .tooltip_with_action("Full screen", &FullScreenDock, None)
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(FullScreenDock), cx)
                        }),
                )
            })
            .when(!self.in_dock && is_subagent, |this| {
                this.child(
                    Button::new("back")
                        .ghost()
                        .small()
                        .icon(icon(IconName::ChevronLeft))
                        .tooltip(format!("Back to {} chat", self.branch))
                        .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatEvent::Back))),
                )
            })
            .when(untrusted, |this| {
                this.child(
                    h_flex()
                        .id("not-trusted")
                        .flex_none()
                        .mr(px(6.))
                        .h(px(26.))
                        .gap(px(6.))
                        .pl_2()
                        .pr(px(10.))
                        .rounded(px(6.))
                        .border_1()
                        .border_color(mix(p.yellow, p.base, 0.45))
                        .bg(mix(p.yellow, p.base, 0.16))
                        .hover(|s| s.bg(mix(p.yellow, p.base, 0.26)))
                        .text_color(p.yellow_text)
                        .text_size(px(12.5))
                        .font_medium()
                        .whitespace_nowrap()
                        .cursor_pointer()
                        .child(icon(Lucide::Lock).size(px(14.)))
                        .child("Read only")
                        .on_click(cx.listener(|view, _, window, cx| view.review_trust(window, cx))),
                )
            })
            .child(
                div()
                    .font_medium()
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .text_ellipsis()
                    .child(title),
            )
            .when(is_subagent, |this| this.child(status_pill(status, cx)))
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .gap_1()
                    .text_color(p.subtext0)
                    .whitespace_nowrap()
                    .overflow_hidden()
                    .child(SharedString::from(description))
                    .when(
                        !is_subagent && (stat.additions > 0 || stat.deletions > 0),
                        |this| {
                            this.child("·")
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
                        },
                    ),
            )
            .map(|this| {
                if self.in_dock {
                    this.child(
                        Button::new("dock-close")
                            .ghost()
                            .small()
                            .icon(icon(Lucide::X))
                            .tooltip("Close split")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(ChatEvent::Close))),
                    )
                } else {
                    this.child(
                        Button::new("toggle-diff")
                            .ghost()
                            .small()
                            .selected(self.diff_open)
                            .icon(icon(Lucide::PanelRight))
                            .tooltip_with_action(
                                if self.diff_open {
                                    "Hide diff"
                                } else {
                                    "Show diff"
                                },
                                &ShowDiff,
                                None,
                            )
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(ShowDiff), cx)
                            }),
                    )
                }
            })
    }

    fn render_task(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let p = palette(cx);
        let session = self.session.read(cx);
        let SessionKind::SubAgent { task, .. } = &session.kind else {
            return None;
        };
        let elapsed = crate::transcript::format_duration(session.started.elapsed());
        Some(
            v_flex()
                .ml(px(GUTTER))
                .px(px(14.))
                .py_3()
                .rounded(px(8.))
                .bg(p.mantle)
                .border_1()
                .border_color(p.surface0)
                .child(
                    div()
                        .text_size(px(12.))
                        .text_color(p.subtext0)
                        .mb_1()
                        .child(format!("Task from Pi · {elapsed} ago")),
                )
                .child(SharedString::from(task.clone()))
                .into_any_element(),
        )
    }

    fn render_composer(&self, window: &Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let commands = self.slash_matches(cx);
        let menu_open = !commands.is_empty();
        h_flex()
            .relative()
            .when(menu_open, |this| {
                this.key_context("SlashMenu")
                    .on_action(
                        cx.listener(|view, _: &SlashUp, _, cx| view.move_slash_selection(-1, cx)),
                    )
                    .on_action(
                        cx.listener(|view, _: &SlashDown, _, cx| view.move_slash_selection(1, cx)),
                    )
                    .on_action(cx.listener(|view, _: &SlashAccept, window, cx| {
                        let ix = view.slash_selected;
                        view.accept_slash(ix, window, cx)
                    }))
                    .on_action(cx.listener(|view, _: &SlashDismiss, _, cx| {
                        view.slash_dismissed = true;
                        cx.notify();
                    }))
                    .child(self.render_slash_menu(&commands, cx))
            })
            .flex_none()
            .items_start()
            .gap_3()
            .px_4()
            .py_3()
            .border_t_1()
            .border_color(p.surface0)
            .child(avatar(&self.initials, &p))
            .child(
                // A multi-line input pads its text by 10px across and 8px down
                // (medium size). Pull it back so the text starts at the gap
                // and its first line is centred on the 22px avatar.
                div()
                    .flex_1()
                    .min_w_0()
                    .ml(px(-10.))
                    .mt(px(-8.) + (px(22.) - window.rem_size() * 1.25) / 2.)
                    .child(
                        Textarea::new(&self.composer)
                            .appearance(false)
                            .aria_label("Message Pi"),
                    ),
            )
    }

    fn render_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let session = self.session.read(cx);
        let running = session.is_running();
        let is_subagent = session.is_subagent();
        let config = Config::global(cx);
        let narrow = self.in_dock;

        let model_label = session
            .model
            .as_ref()
            .map(|m| m.id.clone())
            .unwrap_or_else(|| "Default model".into());
        let provider = session.model.as_ref().map(|m| m.provider.clone());
        let thinking = capitalize(&session.thinking);
        let tools = if narrow || config.tools.len() > 4 {
            format!("{} tools", config.tools.len())
        } else {
            config.tools.join(", ")
        };
        let context = session.context_tokens;
        let window_size = session.model.as_ref().and_then(|m| m.context_window);
        let models = session.models.clone();
        let current_model = session.model.clone();
        let current_thinking = session.thinking.clone();
        let session_entity = self.session.clone();
        let read_only = self.untrusted_project(cx).is_some();

        h_flex()
            .flex_none()
            .h(px(BAR_HEIGHT))
            .gap(px(2.))
            .pl(px(10.))
            .pr_2()
            .border_t_1()
            .border_color(p.surface0)
            .text_size(px(13.))
            .child(
                Button::new("model")
                    .ghost()
                    .small()
                    .icon(icon(Lucide::Sparkles))
                    .child(
                        h_flex()
                            .gap(px(6.))
                            .child(SharedString::from(model_label))
                            .when_some(provider.filter(|_| !narrow), |this, provider| {
                                this.child(
                                    div()
                                        .text_size(px(12.))
                                        .text_color(p.subtext0)
                                        .child(provider),
                                )
                            }),
                    )
                    .on_click({
                        let session = session_entity.clone();
                        move |_, _, cx| session.update(cx, |s, cx| s.load_models(cx))
                    })
                    .dropdown_menu_with_anchor(Anchor::BottomLeft, {
                        let session = session_entity.clone();
                        move |menu, _, _| {
                            let mut menu = menu.scrollable(true).max_h(px(360.));
                            if models.is_empty() {
                                menu = menu.label("Send a message to load Pi's models");
                            }
                            for model in models.iter() {
                                let checked = current_model.as_ref() == Some(model);
                                let (session, model) = (session.clone(), model.clone());
                                menu = menu.item(
                                    PopupMenuItem::new(model.full_name())
                                        .checked(checked)
                                        .on_click(move |_, _, cx| {
                                            session.update(cx, |s, cx| s.set_model(&model, cx))
                                        }),
                                );
                            }
                            menu
                        }
                    }),
            )
            .child(
                Button::new("thinking")
                    .ghost()
                    .small()
                    .icon(icon(Lucide::Lightbulb))
                    .label(thinking)
                    .dropdown_menu_with_anchor(Anchor::BottomLeft, {
                        let session = session_entity.clone();
                        move |mut menu, _, _| {
                            for level in THINKING_LEVELS {
                                let session = session.clone();
                                menu = menu.item(
                                    PopupMenuItem::new(capitalize(level))
                                        .checked(current_thinking == level)
                                        .on_click(move |_, _, cx| {
                                            session.update(cx, |s, cx| s.set_thinking(level, cx))
                                        }),
                                );
                            }
                            menu
                        }
                    }),
            )
            .when(read_only, |this| {
                this.child(
                    Button::new("read-only")
                        .ghost()
                        .small()
                        .icon(icon(Lucide::Wrench))
                        .label("read")
                        .tooltip("Only read until the folder is trusted")
                        .on_click(cx.listener(|view, _, window, cx| view.review_trust(window, cx))),
                )
            })
            .when(!read_only, |this| {
                this.child(
                    Button::new("tools")
                        .ghost()
                        .small()
                        .icon(icon(Lucide::Wrench))
                        .label(tools)
                        .tooltip("Tools Pi may use · Settings › Agent defaults")
                        .on_click(|_, window, cx| {
                            super::settings::open(super::settings::Section::Defaults, window, cx)
                        }),
                )
            })
            .when(!narrow && !is_subagent && !read_only, |this| {
                this.child(
                    Button::new("approval")
                        .ghost()
                        .small()
                        .icon(icon(Lucide::Shield))
                        .label(config.approval.label())
                        .on_click(|_, window, cx| {
                            super::settings::open(super::settings::Section::Approvals, window, cx)
                        }),
                )
            })
            .child(div().flex_1())
            .when_some(context, |this, tokens| {
                let label = match window_size {
                    Some(w) => format!(
                        "Context used: {} of {} tokens",
                        format_tokens(tokens),
                        format_tokens(w)
                    ),
                    None => format!("Context used: {} tokens", format_tokens(tokens)),
                };
                this.child(
                    Button::new("context")
                        .ghost()
                        .small()
                        .icon(icon(Lucide::CircleDashed))
                        .label(format_tokens(tokens))
                        .tooltip(label),
                )
            })
            .child(div().w(px(1.)).h(px(16.)).mx_1().bg(p.surface0))
            .when(running, |this| {
                this.child(
                    Button::new("stop")
                        .ghost()
                        .small()
                        .icon(icon(Lucide::Square))
                        .tooltip_with_action(
                            if is_subagent {
                                "Stop sub-agent"
                            } else {
                                "Stop agent"
                            },
                            &StopAgent,
                            None,
                        )
                        .on_click(cx.listener(|view, _, _, cx| {
                            view.session.update(cx, |s, cx| s.stop(cx))
                        })),
                )
            })
            .child(
                Button::new("send")
                    .ghost()
                    .small()
                    .icon(icon(Lucide::Send))
                    .tooltip(if running {
                        "Steer · Enter"
                    } else {
                        "Send · Enter"
                    })
                    .on_click(cx.listener(|view, _, window, cx| view.send(window, cx))),
            )
    }
}

fn status_pill(status: AgentStatus, cx: &App) -> impl IntoElement {
    let p = palette(cx);
    let (label, color) = match status {
        AgentStatus::Working => ("Running", p.blue),
        AgentStatus::NeedsInput => ("Needs input", p.blue),
        AgentStatus::Failed => ("Failed", p.red),
        AgentStatus::Finished | AgentStatus::Idle => ("Finished", p.green),
    };
    h_flex()
        .flex_none()
        .gap_1()
        .h(px(20.))
        .px_2()
        .rounded_full()
        .bg(mix(color, p.base, 0.16))
        .text_color(color)
        .text_size(px(11.5))
        .when(status == AgentStatus::Working, |this| {
            this.child(
                gpui_kit::component::spinner::Spinner::new()
                    .with_size(px(11.))
                    .color(color),
            )
        })
        .child(label)
}

impl Render for ChatView {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let items = render_items(self, cx);
        let task = self.render_task(cx);
        let running = self.session.read(cx).is_running();
        let compacting = self.session.read(cx).is_compacting();
        let empty = items.is_empty() && task.is_none();
        let untrusted = self.untrusted_project(cx).map(|(_, name)| name);
        let placeholder: SharedString = match &untrusted {
            Some(name) => format!("Ask about {name}…").into(),
            None if self.session.read(cx).is_subagent() => "Steer this sub-agent…".into(),
            None => "Message Pi…".into(),
        };
        if placeholder != self.placeholder {
            self.placeholder = placeholder.clone();
            self.composer.update(cx, |state, cx| {
                state.set_placeholder(placeholder, window, cx)
            });
        }

        v_flex()
            .size_full()
            .min_w_0()
            .bg(p.base)
            .child(self.render_header(cx))
            .child(
                div()
                    .id("history")
                    .flex_1()
                    .min_h_0()
                    .track_scroll(&self.scroll)
                    .overflow_y_scroll()
                    .on_scroll_wheel(cx.listener(move |view, _, _, cx| {
                        let offset = view.scroll.offset().y;
                        let max = view.scroll.max_offset().y;
                        let at_bottom = (-offset - max).abs() < px(24.);
                        if view.follow_tail != at_bottom {
                            view.follow_tail = at_bottom;
                            cx.notify();
                        }
                    }))
                    .px(px(if self.in_dock { 28. } else { 32. }))
                    .py_6()
                    .child(
                        v_flex()
                            .w_full()
                            .max_w(px(780.))
                            .mx_auto()
                            .gap(px(14.))
                            .children(task)
                            .children(items)
                            .when(running || compacting, |this| {
                                this.child(
                                    h_flex()
                                        .gap_2()
                                        .text_size(px(13.))
                                        .text_color(p.subtext0)
                                        .child(
                                            div()
                                                .ml(px(4.))
                                                .mr(px(8.))
                                                .child(gpui_kit::component::spinner::Spinner::new().with_size(px(14.)).color(p.subtext1)),
                                        )
                                        .child(gpui_kit::component::shimmer::ShimmerText::new(if compacting { "Compacting context…" } else { "Working…" })),
                                )
                            })
                            .when_some(untrusted.filter(|_| empty), |this, name| {
                                this.child(
                                    v_flex()
                                        .items_center()
                                        .gap(px(10.))
                                        .pt(px(120.))
                                        .mx_auto()
                                        .max_w(px(440.))
                                        .text_center()
                                        .child(
                                            div()
                                                .size(px(44.))
                                                .rounded(px(12.))
                                                .bg(mix(p.yellow, p.base, 0.14))
                                                .flex()
                                                .items_center()
                                                .justify_center()
                                                .child(icon(Lucide::Lock).size(px(20.)).text_color(p.yellow)),
                                        )
                                        .child(div().text_size(px(16.)).font_semibold().text_color(p.text).child("Read-only until you trust this folder"))
                                        .child(div().text_size(px(13.5)).text_color(p.subtext0).child(format!(
                                            "Pi can read files and answer questions about {name}, but it won't edit files, run commands or load the project's own settings."
                                        )))
                                        .child(
                                            div()
                                                .id("review-trust")
                                                .text_size(px(13.5))
                                                .text_color(p.blue)
                                                .cursor_pointer()
                                                .hover(|s| s.underline())
                                                .child("Review trust…")
                                                .on_click(cx.listener(|view, _, window, cx| view.review_trust(window, cx))),
                                        ),
                                )
                            })
                            .when(empty && self.untrusted_project(cx).is_none(), |this| {
                                this.child(
                                    v_flex()
                                        .items_center()
                                        .gap_2()
                                        .pt(px(120.))
                                        .text_color(p.subtext0)
                                        .child(icon(Lucide::Bot).size(px(28.)))
                                        .child(div().text_size(px(15.)).text_color(p.text).child("Start a chat in this worktree"))
                                        .child(
                                            div()
                                                .text_size(px(13.))
                                                .child("Pi works in this folder only. Ask it to change something, then review the diff."),
                                        ),
                                )
                            }),
                    ),
            )
            .child(self.render_composer(window, cx))
            .child(self.render_bar(cx))
    }
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// `Paul van der Meijs` → `PM`.
fn initials(name: &str) -> String {
    let words: Vec<&str> = name.split_whitespace().collect();
    match words.as_slice() {
        [] => "?".into(),
        [one] => one.chars().take(2).collect::<String>().to_uppercase(),
        [first, .., last] => format!(
            "{}{}",
            first.chars().next().unwrap(),
            last.chars().next().unwrap()
        )
        .to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn initials_use_first_and_last_names() {
        assert_eq!(super::initials("Paul van der Meijs"), "PM");
        assert_eq!(super::initials("ada"), "AD");
    }
}
