//! Projects, their worktrees and the sub-agents in each worktree.
//!
//! Laid out by hand like the chat and the diff dock, to the design's
//! measurements: 272px wide, 30px rows, names at 32px and sub-agent names at
//! 47px behind a guide line. Buttons and menus are gpui-kit's.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::{ContextMenuExt as _, PopupMenu, PopupMenuItem};
use gpui_kit::component::{ActiveTheme as _, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::PathBuf;

use super::app::TreehouseApp;
use super::{BAR_HEIGHT, SIDEBAR_WIDTH, icon, status_dot};
use crate::actions::{
    CommandPalette, OpenProject, OpenSettings, ShowDiff, ToggleSidebar, ToggleTheme,
};
use crate::session::{Session, SessionKind};
use crate::theme::{Palette, mix, palette, toggle_label};

/// How a row relates to what is open.
#[derive(Clone, Copy, PartialEq)]
enum Open {
    No,
    /// Filled: open in the main screen.
    Main,
    /// Outlined: open in the right dock.
    Dock,
}

/// A 30px sidebar row: 8px side padding, 6px radius, 8px gap.
fn row(id: impl Into<ElementId>, open: Open, p: &Palette) -> Stateful<Div> {
    let hover = mix(p.surface0, p.mantle, 0.55);
    h_flex()
        .id(id)
        .w_full()
        .h(px(30.))
        .flex_none()
        .px(px(7.))
        .gap_2()
        .rounded(px(6.))
        .border_1()
        .border_color(if open == Open::Dock {
            p.blue
        } else {
            gpui_kit::transparent_black()
        })
        .text_size(px(13.5))
        .whitespace_nowrap()
        .cursor_pointer()
        .when(open == Open::Main, |this| this.bg(p.surface0))
        .when(open != Open::Main, |this| this.hover(|s| s.bg(hover)))
}

fn label(text: impl Into<SharedString>) -> Div {
    div()
        .flex_1()
        .min_w_0()
        .overflow_hidden()
        .text_ellipsis()
        .child(text.into())
}

fn open_state(session: &Entity<Session>, main: Option<EntityId>, docked: Option<EntityId>) -> Open {
    let id = Some(session.entity_id());
    if main == id {
        Open::Main
    } else if docked == id {
        Open::Dock
    } else {
        Open::No
    }
}

pub fn render(app: &TreehouseApp, cx: &mut Context<TreehouseApp>) -> impl IntoElement {
    let p = palette(cx);
    let main = app.main_session_id(cx);
    let docked = app.docked_session_id(cx);
    let weak = cx.entity().downgrade();

    let mut rows: Vec<AnyElement> = Vec::new();
    let workspace = app.workspace.read(cx);
    for project in workspace.projects.iter() {
        let project_path = project.path.clone();
        let project_open = !app.is_collapsed(&project.path);
        rows.push(
            row(
                SharedString::from(format!("project:{}", project.path.display())),
                Open::No,
                &p,
            )
            .child(
                icon(if project_open {
                    IconName::FolderOpen
                } else {
                    IconName::Folder
                })
                .text_color(p.subtext0),
            )
            .child(label(project.name.clone()))
            .when_some(project.error.clone(), |this, error| {
                this.child(icon(Lucide::CircleAlert).text_color(p.red))
                    .tooltip(move |window, cx| {
                        gpui_kit::component::tooltip::Tooltip::new(error.clone()).build(window, cx)
                    })
            })
            .on_click(cx.listener({
                let path = project_path.clone();
                move |app, _, _, cx| app.toggle_collapsed(path.clone(), cx)
            }))
            .context_menu(project_menu(weak.clone(), project_path.clone()))
            .into_any_element(),
        );
        if !project_open {
            continue;
        }

        for worktree in project.worktrees.iter() {
            let session = worktree.session.clone();
            let has_kids = !worktree.subagents.is_empty();
            let expanded = has_kids && !app.is_collapsed(&worktree.path);
            let path = worktree.path.clone();
            rows.push(
                row(
                    SharedString::from(format!("worktree:{}", path.display())),
                    open_state(&session, main, docked),
                    &p,
                )
                .child(
                    div()
                        .id(SharedString::from(format!("chevron:{}", path.display())))
                        .size(px(16.))
                        .flex_none()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_color(p.subtext0)
                        .when(has_kids, |this| {
                            let path = path.clone();
                            this.child(
                                icon(if expanded {
                                    IconName::ChevronDown
                                } else {
                                    IconName::ChevronRight
                                })
                                .size(px(14.)),
                            )
                            .on_click(cx.listener(
                                move |app, _, _, cx| {
                                    cx.stop_propagation();
                                    app.toggle_collapsed(path.clone(), cx);
                                },
                            ))
                        }),
                )
                .child(label(worktree.branch.clone()))
                .child(status_dot(worktree.status(cx), cx))
                .on_click(cx.listener({
                    let session = session.clone();
                    move |app, event: &ClickEvent, window, cx| {
                        app.open_session(session.clone(), event.modifiers().secondary(), window, cx)
                    }
                }))
                .context_menu(worktree_menu(
                    weak.clone(),
                    session.clone(),
                    path.clone(),
                    worktree.is_main,
                ))
                .into_any_element(),
            );
            if !expanded {
                continue;
            }

            let kids = worktree.subagents.iter().map(|subagent| {
                let child = subagent.session.clone();
                let status = child.read(cx).status();
                row(
                    ("subagent", child.entity_id().as_u64()),
                    open_state(&child, main, docked),
                    &p,
                )
                .pl(px(14.))
                .text_size(px(13.))
                .child(status_dot(status, cx))
                .child(label(child.read(cx).title()))
                .on_click(cx.listener({
                    let child = child.clone();
                    move |app, event: &ClickEvent, window, cx| {
                        app.open_session(child.clone(), event.modifiers().secondary(), window, cx)
                    }
                }))
                .context_menu(subagent_menu(weak.clone(), child))
            });
            rows.push(
                v_flex()
                    .ml(px(15.))
                    .pl(px(1.))
                    .border_l_1()
                    .border_color(p.surface0)
                    .children(kids)
                    .into_any_element(),
            );
        }
    }

    let empty = workspace.projects.is_empty();
    let provider = app.provider_label(cx);

    v_flex()
        .w(px(SIDEBAR_WIDTH))
        .h_full()
        .flex_none()
        .bg(p.mantle)
        .border_r_1()
        .border_color(p.surface0)
        .child(
            h_flex()
                .h(px(BAR_HEIGHT))
                .flex_none()
                .px_2()
                .gap(px(2.))
                .border_b_1()
                .border_color(p.surface0)
                .child(
                    icon_button("toggle-sidebar", IconName::PanelLeft)
                        .tooltip_with_action("Toggle sidebar", &ToggleSidebar, None)
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(ToggleSidebar), cx)
                        }),
                )
                .child(
                    div()
                        .px_1()
                        .font_semibold()
                        .text_size(px(13.5))
                        .child("Treehouse"),
                )
                .child(div().flex_1())
                .child(
                    icon_button("command-palette", IconName::Search)
                        .tooltip_with_action("Command palette", &CommandPalette, None)
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(CommandPalette), cx)
                        }),
                )
                .child(
                    icon_button(
                        "toggle-theme",
                        if cx.theme().is_dark() {
                            IconName::Sun
                        } else {
                            IconName::Moon
                        },
                    )
                    .tooltip(toggle_label(cx))
                    .on_click(|_, window, cx| window.dispatch_action(Box::new(ToggleTheme), cx)),
                ),
        )
        .child(
            v_flex()
                .id("projects")
                .flex_1()
                .min_h_0()
                .overflow_y_scroll()
                .px(px(6.))
                .py_1()
                .child(
                    h_flex()
                        .justify_between()
                        .pl_2()
                        .pr(px(2.))
                        .pt_2()
                        .pb_1()
                        .text_size(px(13.))
                        .text_color(p.subtext0)
                        .child("Projects")
                        .child(
                            icon_button("add-project", Lucide::FolderPlus)
                                .tooltip("Add project — choose a folder")
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(OpenProject), cx)
                                }),
                        ),
                )
                .children(rows)
                .when(empty, |this| this.child(render_empty(&p))),
        )
        .child(
            h_flex()
                .h(px(BAR_HEIGHT))
                .flex_none()
                .px_2()
                .gap_1()
                .border_t_1()
                .border_color(p.surface0)
                .child(
                    icon_button("settings", IconName::Settings)
                        .tooltip_with_action("Settings", &OpenSettings, None)
                        .on_click(|_, window, cx| {
                            window.dispatch_action(Box::new(OpenSettings), cx)
                        }),
                )
                .child(div().flex_1())
                .when_some(provider, |this, (name, ok)| {
                    this.child(
                        Button::new("provider")
                            .ghost()
                            .small()
                            .child(
                                h_flex()
                                    .gap(px(6.))
                                    .text_size(px(12.5))
                                    .text_color(p.subtext0)
                                    .child(div().size(px(7.)).rounded_full().bg(if ok {
                                        p.green
                                    } else {
                                        p.red
                                    }))
                                    .child(name.clone()),
                            )
                            .tooltip(if ok {
                                format!("{name}: connected")
                            } else {
                                format!("{name}: no key — open Settings › Providers")
                            })
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(OpenSettings), cx)
                            }),
                    )
                }),
        )
}

/// The design's 28px icon-only button.
fn icon_button(id: impl Into<ElementId>, name: impl Into<gpui_kit::component::Icon>) -> Button {
    Button::new(id).ghost().small().icon(icon(name))
}

fn render_empty(p: &Palette) -> impl IntoElement {
    v_flex()
        .items_center()
        .gap_3()
        .px_5()
        .py_7()
        .text_center()
        .child(
            icon(IconName::FolderOpen)
                .size(px(24.))
                .text_color(p.subtext0),
        )
        .child(
            div().text_size(px(13.)).text_color(p.subtext0).child(
                "Open a folder to add your first project. Treehouse lists its worktrees here.",
            ),
        )
        .child(
            Button::new("open-folder")
                .primary()
                .small()
                .label("Open folder…")
                .on_click(|_, window, cx| window.dispatch_action(Box::new(OpenProject), cx)),
        )
}

type MenuBuilder = Box<dyn Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu>;

fn project_menu(
    app: WeakEntity<TreehouseApp>,
    path: PathBuf,
) -> impl Fn(PopupMenu, &mut Window, &mut Context<PopupMenu>) -> PopupMenu + 'static {
    move |menu, _, _| {
        let (a1, p1) = (app.clone(), path.clone());
        let p2 = path.clone();
        let p3 = path.clone();
        let (a4, p4) = (app.clone(), path.clone());
        menu.item(
            PopupMenuItem::new("New worktree").on_click(move |_, window, cx| {
                let _ = a1.update(cx, |app, cx| {
                    app.prompt_new_worktree(Some(p1.clone()), window, cx)
                });
            }),
        )
        .item(
            PopupMenuItem::new("Reveal in file manager")
                .on_click(move |_, _, cx| cx.reveal_path(&p2)),
        )
        .item(PopupMenuItem::new("Copy path").on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(p3.display().to_string()))
        }))
        .separator()
        .item(
            PopupMenuItem::new("Remove project").on_click(move |_, window, cx| {
                let _ = a4.update(cx, |app, cx| {
                    app.confirm_remove_project(p4.clone(), window, cx)
                });
            }),
        )
    }
}

fn worktree_menu(
    app: WeakEntity<TreehouseApp>,
    session: Entity<Session>,
    path: PathBuf,
    is_main: bool,
) -> MenuBuilder {
    Box::new(move |menu, _, _| {
        let (a1, s1) = (app.clone(), session.clone());
        let (a2, s2) = (app.clone(), session.clone());
        let (a3, p3) = (app.clone(), path.clone());
        let (a4, p4) = (app.clone(), path.clone());
        let menu = menu
            .item(
                PopupMenuItem::new("Open in right dock").on_click(move |_, window, cx| {
                    let _ = a1.update(cx, |app, cx| app.open_session(s1.clone(), true, window, cx));
                }),
            )
            .item(
                PopupMenuItem::new("Show diff")
                    .action(Box::new(ShowDiff))
                    .on_click(move |_, window, cx| {
                        let _ = a2.update(cx, |app, cx| {
                            app.open_session(s2.clone(), false, window, cx);
                            app.show_diff(window, cx);
                        });
                    }),
            )
            .item(
                PopupMenuItem::new("Rename branch").on_click(move |_, window, cx| {
                    let _ = a3.update(cx, |app, cx| {
                        app.prompt_rename_branch(p3.clone(), window, cx)
                    });
                }),
            );
        if is_main {
            menu
        } else {
            menu.separator()
                .item(
                    PopupMenuItem::new("Delete worktree…").on_click(move |_, window, cx| {
                        let _ = a4.update(cx, |app, cx| {
                            app.confirm_delete_worktree(p4.clone(), window, cx)
                        });
                    }),
                )
        }
    })
}

fn subagent_menu(app: WeakEntity<TreehouseApp>, child: Entity<Session>) -> MenuBuilder {
    Box::new(move |menu, _, _| {
        let (a1, c1) = (app.clone(), child.clone());
        let c2 = child.clone();
        let c3 = child.clone();
        menu.item(
            PopupMenuItem::new("Open in right dock").on_click(move |_, window, cx| {
                let _ = a1.update(cx, |app, cx| app.open_session(c1.clone(), true, window, cx));
            }),
        )
        .item(
            PopupMenuItem::new("Stop").on_click(move |_, _, cx| c3.update(cx, |s, cx| s.stop(cx))),
        )
        .item(PopupMenuItem::new("Copy task").on_click(move |_, _, cx| {
            if let SessionKind::SubAgent { task, .. } = &c2.read(cx).kind {
                cx.write_to_clipboard(ClipboardItem::new_string(task.clone()));
            }
        }))
    })
}
