//! The root view: sidebar, main screen and right dock.

use gpui_kit::component::button::{Button, ButtonVariant, ButtonVariants as _};
use gpui_kit::component::command::CommandState;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::component::{ActiveTheme as _, IconName, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::path::PathBuf;

use super::chat::{ChatEvent, ChatView};
use super::diff::{DiffPanel, DiffPanelEvent};
use super::settings::Section;
use super::{icon, palette as command_palette, settings, sidebar};
use crate::actions::*;
use crate::config::{Config, ThemeChoice};
use crate::session::Session;
use crate::theme::palette;
use crate::workspace::{Workspace, WorkspaceEvent};

enum Dock {
    Diff(Entity<DiffPanel>),
    Session(Entity<ChatView>),
}

/// What a sidebar disclosure folds.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Fold {
    /// A project's worktrees.
    Project,
    /// A worktree's sub-agents.
    Worktree,
}

pub struct TreehouseApp {
    pub workspace: Entity<Workspace>,
    focus_handle: FocusHandle,
    main: Option<Entity<ChatView>>,
    dock: Option<Dock>,
    dock_full: bool,
    sidebar_open: bool,
    /// Projects and worktrees the user collapsed in the sidebar. A main
    /// worktree shares its project's folder, so the kind is part of the key.
    collapsed: std::collections::HashSet<(Fold, PathBuf)>,
    chats: HashMap<EntityId, Entity<ChatView>>,
    /// Undecided projects whose trust prompt was shown this run.
    trust_asked: std::collections::HashSet<PathBuf>,
    palette_state: Entity<CommandState>,
    _subscriptions: Vec<Subscription>,
}

impl TreehouseApp {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let workspace = cx.new(Workspace::new);
        let subscriptions = vec![
            // Entering or leaving full screen changes the room the traffic
            // lights need.
            cx.observe_window_bounds(window, |_, _, cx| cx.notify()),
            cx.observe(&workspace, |_, _, cx| cx.notify()),
            cx.subscribe_in(
                &workspace,
                window,
                |app, _, event: &WorkspaceEvent, _, cx| {
                    let WorkspaceEvent::DiffChanged(path) = event;
                    if let Some(Dock::Diff(panel)) = &app.dock
                        && &panel.read(cx).worktree == path
                    {
                        panel.update(cx, |panel, cx| panel.reload(cx));
                    }
                },
            ),
            cx.observe_window_appearance(window, |_, window, cx| {
                if Config::global(cx).theme == ThemeChoice::System {
                    crate::theme::apply(ThemeChoice::System, Some(window), cx);
                }
            }),
        ];
        let mut this = Self {
            workspace,
            focus_handle: cx.focus_handle(),
            main: None,
            dock: None,
            dock_full: false,
            sidebar_open: true,
            collapsed: Default::default(),
            chats: HashMap::new(),
            trust_asked: Default::default(),
            palette_state: cx.new(|cx| CommandState::new(window, cx)),
            _subscriptions: subscriptions,
        };
        let first = this
            .workspace
            .read(cx)
            .projects
            .iter()
            .flat_map(|p| p.worktrees.iter())
            .next()
            .map(|w| w.session.clone());
        if let Some(session) = first {
            this.open_session(session, false, window, cx);
        }
        // Start typing straight into the open chat's message box.
        match &this.main {
            Some(chat) => chat.update(cx, |chat, cx| chat.focus_composer(window, cx)),
            None => this.focus_handle.focus(window, cx),
        }
        register_actions(window.window_handle(), cx.entity().downgrade(), cx);
        if Config::global(cx).theme == ThemeChoice::System {
            crate::theme::apply(ThemeChoice::System, Some(window), cx);
        }
        this
    }

    pub fn is_collapsed(&self, fold: Fold, path: &std::path::Path) -> bool {
        self.collapsed.contains(&(fold, path.to_path_buf()))
    }

    pub fn toggle_collapsed(&mut self, fold: Fold, path: PathBuf, cx: &mut Context<Self>) {
        let key = (fold, path);
        if !self.collapsed.remove(&key) {
            self.collapsed.insert(key);
        }
        cx.notify();
    }

    pub fn main_session_id(&self, cx: &App) -> Option<EntityId> {
        self.main
            .as_ref()
            .map(|chat| chat.read(cx).session.entity_id())
    }

    pub fn docked_session_id(&self, cx: &App) -> Option<EntityId> {
        match &self.dock {
            Some(Dock::Session(chat)) => Some(chat.read(cx).session.entity_id()),
            _ => None,
        }
    }

    /// The provider shown in the sidebar footer, and whether it has a key.
    pub fn provider_label(&self, cx: &App) -> Option<(SharedString, bool)> {
        let provider = self
            .main
            .as_ref()
            .and_then(|chat| {
                chat.read(cx)
                    .session
                    .read(cx)
                    .model
                    .as_ref()
                    .map(|m| m.provider.clone())
            })
            .or_else(|| {
                Config::global(cx)
                    .model
                    .as_ref()
                    .and_then(|m| m.split_once('/'))
                    .map(|(p, _)| p.to_string())
            })?;
        let ok = crate::pi::auth::credential(&provider).is_set();
        Some((provider.into(), ok))
    }

    fn chat_for(
        &mut self,
        session: Entity<Session>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Entity<ChatView> {
        if let Some(chat) = self.chats.get(&session.entity_id()) {
            return chat.clone();
        }
        session.update(cx, |session, cx| session.restore_history(cx));
        let workspace = self.workspace.clone();
        let chat = cx.new(|cx| ChatView::new(session.clone(), workspace, window, cx));
        self._subscriptions
            .push(cx.subscribe_in(&chat, window, Self::on_chat_event));
        self.chats.insert(session.entity_id(), chat.clone());
        chat
    }

    fn on_chat_event(
        &mut self,
        chat: &Entity<ChatView>,
        event: &ChatEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            ChatEvent::Open { session, dock } => {
                self.open_session(session.clone(), *dock, window, cx)
            }
            ChatEvent::ShowDiff { worktree, path } => {
                self.open_diff(worktree.clone(), path.clone(), window, cx)
            }
            ChatEvent::Back => {
                let session = chat.read(cx).session.entity_id();
                let parent = self
                    .workspace
                    .read(cx)
                    .worktree_of(session)
                    .map(|w| w.session.clone());
                if let Some(parent) = parent {
                    self.open_session(parent, false, window, cx);
                }
            }
            ChatEvent::Close => self.close_dock(cx),
        }
    }

    /// Opens a worktree chat or sub-agent in the main screen, or in the
    /// right dock with ⌘-click.
    pub fn open_session(
        &mut self,
        session: Entity<Session>,
        dock: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let chat = self.chat_for(session.clone(), window, cx);
        session.update(cx, |s, cx| s.mark_viewed(cx));
        if dock {
            if self.main.as_ref() == Some(&chat) {
                return;
            }
            self.dock = Some(Dock::Session(chat.clone()));
        } else {
            if matches!(&self.dock, Some(Dock::Session(docked)) if docked == &chat) {
                self.dock = None;
                self.dock_full = false;
            }
            // A diff belongs to the main screen's worktree; follow it.
            let worktree = self
                .workspace
                .read(cx)
                .worktree_of(session.entity_id())
                .map(|w| w.path.clone());
            let diff_follows = matches!(&self.dock, Some(Dock::Diff(panel)) if Some(&panel.read(cx).worktree) != worktree.as_ref());
            self.main = Some(chat.clone());
            if diff_follows {
                self.dock = None;
                self.show_diff(window, cx);
            }
        }
        self.sync_chat_flags(cx);
        chat.update(cx, |chat, cx| chat.focus_composer(window, cx));
        if !dock {
            self.ask_trust(&session, window, cx);
        }
        cx.notify();
    }

    /// Asks once per run whether to trust a project nobody decided on yet.
    fn ask_trust(
        &mut self,
        session: &Entity<Session>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let workspace = self.workspace.read(cx);
        let Some(project) = workspace
            .worktree_of(session.entity_id())
            .and_then(|w| workspace.project_of(&w.path))
            .filter(|p| p.trust == crate::trust::Trust::Undecided)
            .map(|p| p.path.clone())
        else {
            return;
        };
        if self.trust_asked.insert(project.clone()) {
            // The first chat opens while the window is still being built.
            let workspace = self.workspace.clone();
            window.defer(cx, move |window, cx| {
                super::trust::open(workspace, project, true, window, cx)
            });
        }
    }

    fn sync_chat_flags(&mut self, cx: &mut Context<Self>) {
        let docked = self.docked_session_id(cx);
        let diff_open = matches!(self.dock, Some(Dock::Diff(_)));
        let dock_chat = match &self.dock {
            Some(Dock::Session(chat)) => Some(chat.clone()),
            _ => None,
        };
        for chat in self.chats.values() {
            let in_dock = dock_chat.as_ref() == Some(chat);
            chat.update(cx, |chat, cx| {
                chat.in_dock = in_dock;
                chat.docked_session = docked;
                chat.diff_open = diff_open && !in_dock;
                cx.notify();
            });
        }
    }

    /// Toggles the main screen worktree's diff in the right dock.
    pub fn show_diff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.dock, Some(Dock::Diff(_))) {
            self.close_dock(cx);
            return;
        }
        let Some(chat) = &self.main else { return };
        let session = chat.read(cx).session.entity_id();
        let Some(path) = self
            .workspace
            .read(cx)
            .worktree_of(session)
            .map(|w| w.path.clone())
        else {
            return;
        };
        self.open_diff(path, None, window, cx);
    }

    fn open_diff(
        &mut self,
        worktree: PathBuf,
        file: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let existing = match &self.dock {
            Some(Dock::Diff(panel)) if panel.read(cx).worktree == worktree => Some(panel.clone()),
            _ => None,
        };
        let panel = existing.unwrap_or_else(|| {
            let (branch, base) = self
                .workspace
                .read(cx)
                .worktree(&worktree)
                .map(|w| (w.branch.clone(), w.base.clone()))
                .unwrap_or_default();
            let panel = cx.new(|cx| DiffPanel::new(worktree.clone(), branch, base, cx));
            self._subscriptions.push(cx.subscribe_in(
                &panel,
                window,
                |app, _, event: &DiffPanelEvent, _, cx| {
                    let DiffPanelEvent::Close = event;
                    app.close_dock(cx);
                },
            ));
            panel
        });
        if let Some(file) = file {
            panel.update(cx, |panel, cx| panel.scroll_to_file(&file, cx));
        }
        let full = self.dock_full;
        panel.update(cx, |panel, cx| panel.set_full_screen(full, cx));
        self.dock = Some(Dock::Diff(panel));
        self.sync_chat_flags(cx);
        cx.notify();
    }

    fn close_dock(&mut self, cx: &mut Context<Self>) {
        self.dock = None;
        self.dock_full = false;
        self.sync_chat_flags(cx);
        cx.notify();
    }

    fn toggle_full_screen(&mut self, cx: &mut Context<Self>) {
        if self.dock.is_none() {
            return;
        }
        self.dock_full = !self.dock_full;
        if let Some(Dock::Diff(panel)) = &self.dock {
            let full = self.dock_full;
            panel.update(cx, |panel, cx| panel.set_full_screen(full, cx));
        }
        cx.notify();
    }

    fn open_project(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Add project".into()),
        });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else {
                return;
            };
            let Some(path) = paths.into_iter().next() else {
                return;
            };
            let _ = this.update(cx, |app, cx| {
                let result = app
                    .workspace
                    .update(cx, |w, cx| w.add_project(path.clone(), cx));
                if let Err(err) = result {
                    eprintln!("treehouse: couldn't add {}: {err:#}", path.display());
                }
            });
        })
        .detach();
    }

    /// The project new worktrees go into: the main screen's, else the first.
    fn current_project(&self, cx: &App) -> Option<PathBuf> {
        let workspace = self.workspace.read(cx);
        self.main
            .as_ref()
            .and_then(|chat| workspace.worktree_of(chat.read(cx).session.entity_id()))
            .and_then(|w| workspace.project_of(&w.path))
            .or_else(|| workspace.projects.first())
            .map(|p| p.path.clone())
    }

    pub fn prompt_new_worktree(
        &mut self,
        project: Option<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(project) = project.or_else(|| self.current_project(cx)) else {
            self.open_project(window, cx);
            return;
        };
        let name = project
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let input = cx.new(|cx| InputState::new(window, cx).placeholder("feat/my-change"));
        let app = cx.entity().downgrade();
        self.prompt_text(
            format!("New worktree in {name}"),
            "Branch name. Treehouse creates the branch from the current HEAD.",
            "Create worktree",
            input,
            window,
            cx,
            move |branch, window, cx| {
                let project = project.clone();
                let _ = app.update(cx, |app, cx| {
                    let created = app
                        .workspace
                        .update(cx, |w, cx| w.new_worktree(&project, &branch, cx));
                    match created {
                        Ok(path) => {
                            let session = app
                                .workspace
                                .read(cx)
                                .worktree(&path)
                                .map(|w| w.session.clone());
                            if let Some(session) = session {
                                app.open_session(session, false, window, cx);
                            }
                        }
                        Err(err) => window.push_notification(
                            format!("Couldn't create the worktree: {err:#}"),
                            cx,
                        ),
                    }
                });
            },
        );
    }

    pub fn prompt_rename_branch(
        &mut self,
        worktree: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let current = self
            .workspace
            .read(cx)
            .worktree(&worktree)
            .map(|w| w.branch.to_string())
            .unwrap_or_default();
        let input = cx.new(|cx| InputState::new(window, cx).default_value(current));
        let app = cx.entity().downgrade();
        self.prompt_text(
            "Rename branch",
            "The worktree keeps its folder.",
            "Rename",
            input,
            window,
            cx,
            move |name, window, cx| {
                let worktree = worktree.clone();
                let _ = app.update(cx, |app, cx| {
                    if let Err(err) = app
                        .workspace
                        .update(cx, |w, cx| w.rename_branch(&worktree, &name, cx))
                    {
                        window
                            .push_notification(format!("Couldn't rename the branch: {err:#}"), cx);
                    }
                });
            },
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn prompt_text(
        &mut self,
        title: impl Into<SharedString>,
        description: &'static str,
        ok: &'static str,
        input: Entity<InputState>,
        window: &mut Window,
        cx: &mut Context<Self>,
        on_ok: impl Fn(String, &mut Window, &mut App) + 'static,
    ) {
        let title = title.into();
        let on_ok = std::rc::Rc::new(on_ok);
        input.update(cx, |state, cx| state.focus(window, cx));
        window.open_dialog(cx, move |dialog, _, cx| {
            let p = palette(cx);
            let (input, on_ok) = (input.clone(), on_ok.clone());
            dialog
                .title(title.clone())
                .w(px(440.))
                .child(
                    v_flex()
                        .gap_2()
                        .child(
                            div()
                                .text_size(px(13.))
                                .text_color(p.subtext0)
                                .child(description),
                        )
                        .child(Input::new(&input)),
                )
                .button_props(gpui_kit::component::dialog::DialogButtonProps::default().ok_text(ok))
                .on_ok(move |_, window, cx| {
                    let value = input.read(cx).value().trim().to_string();
                    if value.is_empty() {
                        return false;
                    }
                    on_ok(value, window, cx);
                    true
                })
        });
    }

    pub fn confirm_delete_worktree(
        &mut self,
        worktree: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let branch = self
            .workspace
            .read(cx)
            .worktree(&worktree)
            .map(|w| w.branch.to_string())
            .unwrap_or_default();
        let delete = Config::global(cx).delete_on_remove;
        let mut unsaved = crate::git::unsaved_work(&worktree, &branch);
        if !delete {
            // The branch is kept, so its commits aren't lost.
            unsaved.commits = 0;
        }
        let description = if unsaved.is_empty() {
            if delete {
                "Its folder and branch are deleted too. Its chat and sub-agents stop.".to_string()
            } else {
                "The folder is removed; the branch is kept. Its chat and sub-agents stop."
                    .to_string()
            }
        } else {
            lost_work(&unsaved, &branch)
        };
        let force = !unsaved.is_empty();
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (app, worktree) = (app.clone(), worktree.clone());
            alert
                .title(format!("Delete worktree “{branch}”?"))
                .description(description.clone())
                .confirm()
                .ok_text(if force {
                    "Delete anyway"
                } else {
                    "Delete worktree"
                })
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, window, cx| {
                    let _ = app.update(cx, |app, cx| {
                        match app
                            .workspace
                            .update(cx, |w, cx| w.delete_worktree(&worktree, force, cx))
                        {
                            Ok(note) => {
                                app.forget_worktree(&worktree, cx);
                                if let Some(note) = note {
                                    window.push_notification(note, cx);
                                }
                            }
                            Err(err) => window.push_notification(
                                format!("Couldn't delete the worktree: {err:#}"),
                                cx,
                            ),
                        }
                    });
                    true
                })
        });
    }

    pub fn confirm_remove_project(
        &mut self,
        project: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = project
            .file_name()
            .map(|n| n.to_string_lossy().to_string())
            .unwrap_or_default();
        let app = cx.entity().downgrade();
        window.open_alert_dialog(cx, move |alert, _, _| {
            let (app, project) = (app.clone(), project.clone());
            alert
                .title(format!("Remove “{name}”?"))
                .description("Files on disk aren't deleted.")
                .confirm()
                .ok_text("Remove")
                .ok_variant(ButtonVariant::Danger)
                .on_ok(move |_, _, cx| {
                    let _ = app.update(cx, |app, cx| {
                        let worktrees: Vec<PathBuf> = app
                            .workspace
                            .read(cx)
                            .projects
                            .iter()
                            .filter(|p| p.path == project)
                            .flat_map(|p| p.worktrees.iter().map(|w| w.path.clone()))
                            .collect();
                        for worktree in worktrees {
                            app.forget_worktree(&worktree, cx);
                        }
                        app.workspace
                            .update(cx, |w, cx| w.remove_project(&project, cx));
                    });
                    true
                })
        });
    }

    /// Closes any view of a worktree that is going away.
    fn forget_worktree(&mut self, worktree: &PathBuf, cx: &mut Context<Self>) {
        let belongs =
            |chat: &Entity<ChatView>, cx: &App| chat.read(cx).session.read(cx).cwd == *worktree;
        if self.main.as_ref().is_some_and(|chat| belongs(chat, cx)) {
            self.main = None;
        }
        let dock_gone = match &self.dock {
            Some(Dock::Session(chat)) => belongs(chat, cx),
            Some(Dock::Diff(panel)) => &panel.read(cx).worktree == worktree,
            None => false,
        };
        if dock_gone {
            self.close_dock(cx);
        }
        self.chats.retain(|_, chat| !belongs(chat, cx));
        cx.notify();
    }

    fn main_session(&self, cx: &App) -> Option<Entity<Session>> {
        self.main.as_ref().map(|chat| chat.read(cx).session.clone())
    }

    fn switch_worktree(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Cycle to the next worktree; the palette lists them by name too.
        let sessions: Vec<Entity<Session>> = self
            .workspace
            .read(cx)
            .projects
            .iter()
            .flat_map(|p| p.worktrees.iter().map(|w| w.session.clone()))
            .collect();
        if sessions.is_empty() {
            return;
        }
        let current = self.main_session_id(cx);
        let ix = sessions
            .iter()
            .position(|s| Some(s.entity_id()) == current)
            .map(|i| i + 1)
            .unwrap_or(0);
        self.open_session(sessions[ix % sessions.len()].clone(), false, window, cx);
    }

    fn render_empty(
        &self,
        sidebar_lead: Option<Pixels>,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let p = palette(cx);
        v_flex()
            .flex_1()
            .h_full()
            .bg(p.base)
            .when_some(sidebar_lead, |this, lead| {
                this.child(
                    super::drags_window(h_flex().id("empty-top"))
                        .h(px(super::BAR_HEIGHT))
                        .flex_none()
                        .pl(lead)
                        .border_b_1()
                        .border_color(p.surface0)
                        .child(super::sidebar_toggle("empty-sidebar")),
                )
            })
            .child(self.render_empty_body(cx))
    }

    fn render_empty_body(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        v_flex()
            .flex_1()
            .items_center()
            .justify_center()
            .gap_3()
            .text_color(p.subtext0)
            .child(icon(IconName::FolderOpen).size(px(32.)))
            .child(
                div()
                    .text_size(px(16.))
                    .text_color(p.text)
                    .child("No worktree open"),
            )
            .child(
                div()
                    .text_size(px(13.))
                    .child("Add a project folder; each of its git worktrees gets its own Pi chat."),
            )
            .child(
                Button::new("empty-open")
                    .primary()
                    .label("Open folder…")
                    .on_click(cx.listener(|app, _, window, cx| app.open_project(window, cx))),
            )
    }
}

/// Handles the app's actions at application level, so they work wherever
/// focus is, including right after a dialog or the dock closes. The handler
/// runs deferred, once the dispatching window is free to update.
fn route<A: Action>(
    window: AnyWindowHandle,
    app: WeakEntity<TreehouseApp>,
    cx: &mut App,
    handler: impl Fn(&mut TreehouseApp, &mut Window, &mut Context<TreehouseApp>) + Clone + 'static,
) {
    cx.on_action(move |_: &A, cx| {
        let (app, handler) = (app.clone(), handler.clone());
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                app.update(cx, |app, cx| handler(app, window, cx))
            });
        });
    });
}

fn register_actions(window: AnyWindowHandle, app: WeakEntity<TreehouseApp>, cx: &mut App) {
    route::<CommandPalette>(window, app.clone(), cx, |app, window, cx| {
        command_palette::open(&app.palette_state, window, cx)
    });
    route::<OpenSettings>(window, app.clone(), cx, |_, window, cx| {
        settings::open(Section::Providers, window, cx)
    });
    route::<OpenProject>(window, app.clone(), cx, |app, window, cx| {
        app.open_project(window, cx)
    });
    route::<NewWorktree>(window, app.clone(), cx, |app, window, cx| {
        app.prompt_new_worktree(None, window, cx)
    });
    route::<SwitchWorktree>(window, app.clone(), cx, |app, window, cx| {
        app.switch_worktree(window, cx)
    });
    route::<ShowDiff>(window, app.clone(), cx, |app, window, cx| {
        app.show_diff(window, cx)
    });
    route::<ToggleDock>(window, app.clone(), cx, |app, window, cx| {
        if app.dock.is_some() {
            app.close_dock(cx)
        } else {
            app.show_diff(window, cx)
        }
    });
    route::<FullScreenDock>(window, app.clone(), cx, |app, _, cx| {
        app.toggle_full_screen(cx)
    });
    route::<ToggleSidebar>(window, app.clone(), cx, |app, _, cx| {
        app.sidebar_open = !app.sidebar_open;
        cx.notify();
    });
    route::<ToggleTheme>(window, app.clone(), cx, |_, window, cx| {
        let next = if cx.theme().is_dark() {
            ThemeChoice::Latte
        } else {
            ThemeChoice::Mocha
        };
        Config::update(cx, |c| c.theme = next);
        crate::theme::apply(next, Some(window), cx);
    });
    route::<StopAgent>(window, app.clone(), cx, |app, _, cx| {
        if let Some(session) = app.main_session(cx) {
            session.update(cx, |s, cx| s.stop(cx));
        }
    });
    route::<ChangeModel>(window, app.clone(), cx, |_, window, cx| {
        window.push_notification(
            "Pick a model from the model button under the message box.",
            cx,
        )
    });
    route::<SetThinkingLevel>(window, app, cx, |_, window, cx| {
        window.push_notification(
            "Pick a thinking level from the lightbulb button under the message box.",
            cx,
        )
    });
}

impl Render for TreehouseApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // A closed dock or dialog can take focus with it; give it back to the
        // main message box so typing and shortcuts keep working.
        if window.focused(cx).is_none() && !window.has_active_dialog(cx) {
            match &self.main {
                Some(chat) => chat.update(cx, |chat, cx| chat.focus_composer(window, cx)),
                None => self.focus_handle.focus(window, cx),
            }
        }
        let p = palette(cx);
        let dock: Option<AnyElement> = match &self.dock {
            Some(Dock::Diff(panel)) => Some(panel.clone().into_any_element()),
            Some(Dock::Session(chat)) => Some(chat.clone().into_any_element()),
            None => None,
        };
        let full = self.dock_full && dock.is_some();

        // The pane beside a collapsed sidebar carries its toggle on macOS;
        // see `sidebar::render_rail`.
        let lead = (!self.sidebar_open)
            .then(|| super::rail_lead(window))
            .flatten();
        let beside = if full {
            match &self.dock {
                Some(Dock::Diff(panel)) => panel.entity_id(),
                Some(Dock::Session(chat)) => chat.entity_id(),
                None => unreachable!(),
            }
        } else {
            match &self.main {
                Some(chat) => chat.entity_id(),
                None => cx.entity_id(),
            }
        };
        for chat in self.chats.values() {
            let lead = lead.filter(|_| chat.entity_id() == beside);
            chat.update(cx, |chat, _| chat.sidebar_lead = lead);
        }
        match &self.dock {
            Some(Dock::Diff(panel)) => {
                let lead = lead.filter(|_| panel.entity_id() == beside);
                panel.update(cx, |panel, _| panel.sidebar_lead = lead);
            }
            Some(Dock::Session(chat)) => {
                let lead = lead.filter(|_| chat.entity_id() == beside);
                chat.update(cx, |chat, _| chat.sidebar_lead = lead);
            }
            None => {}
        }
        let empty_lead = lead.filter(|_| beside == cx.entity_id());

        h_flex()
            .id("treehouse")
            .track_focus(&self.focus_handle)
            .key_context("Treehouse")
            .size_full()
            .bg(p.base)
            .text_color(p.text)
            .font_family("IBM Plex Sans")
            .when(self.sidebar_open, |this| {
                this.child(sidebar::render(self, window, cx))
            })
            .when(!self.sidebar_open, |this| {
                this.child(sidebar::render_rail(self, lead.is_some(), cx))
            })
            .child(
                h_flex()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .when(!full, |this| match &self.main {
                        Some(chat) => {
                            this.child(div().flex_1().min_w_0().h_full().child(chat.clone()))
                        }
                        None => this.child(self.render_empty(empty_lead, cx)),
                    })
                    .when_some(dock, |this, dock| {
                        this.child(
                            div()
                                .flex_1()
                                .min_w_0()
                                .h_full()
                                .when(!full, |this| this.border_l_1().border_color(p.surface0))
                                .child(dock),
                        )
                    }),
            )
    }
}

/// The warning for deleting a worktree with work nothing else has.
fn lost_work(unsaved: &crate::git::UnsavedWork, branch: &str) -> String {
    let mut lost = Vec::new();
    if !unsaved.files.is_empty() {
        let n = unsaved.files.len();
        let mut names: Vec<&str> = unsaved.files.iter().take(3).map(String::as_str).collect();
        if n > 3 {
            names.push("…");
        }
        lost.push(format!(
            "{n} uncommitted {} ({})",
            if n == 1 { "file" } else { "files" },
            names.join(", ")
        ));
    }
    if unsaved.commits > 0 {
        let n = unsaved.commits;
        lost.push(format!(
            "{n} {} only on {branch}",
            if n == 1 { "commit" } else { "commits" }
        ));
    }
    format!(
        "Deleting it throws away {}. Commit or merge the work first to keep it.",
        lost.join(" and ")
    )
}
