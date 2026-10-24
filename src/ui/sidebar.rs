//! Projects, their worktrees and the sub-agents in each worktree.

use gpui_kit::component::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::menu::PopupMenuItem;
use gpui_kit::component::sidebar::{
    Sidebar, SidebarFooter, SidebarHeader, SidebarItem, SidebarMenu, SidebarMenuItem,
};
use gpui_kit::component::{ActiveTheme as _, Collapsible, IconName, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;

use super::app::TreehouseApp;
use super::{BAR_HEIGHT, SIDEBAR_WIDTH, icon, status_dot};
use crate::actions::{
    CommandPalette, OpenProject, OpenSettings, ShowDiff, ToggleSidebar, ToggleTheme,
};
use crate::theme::{palette, toggle_label};
use gpui_kit::assets::IconName as Lucide;

pub fn render(app: &TreehouseApp, cx: &mut Context<TreehouseApp>) -> impl IntoElement {
    let p = palette(cx);
    let workspace = app.workspace.read(cx);
    let main = app.main_session_id(cx);
    let docked = app.docked_session_id(cx);

    let mut menu = SidebarMenu::new();
    for project in workspace.projects.iter() {
        let project_path = project.path.clone();
        let worktrees = project.worktrees.iter().map(|worktree| {
            let session = worktree.session.clone();
            let status = worktree.status(cx);
            let is_main = main == Some(session.entity_id());
            let is_docked = docked == Some(session.entity_id());
            let path = worktree.path.clone();
            let subagents = worktree.subagents.iter().map(|subagent| {
                let child = subagent.session.clone();
                let child_status = child.read(cx).status();
                let is_main = main == Some(child.entity_id());
                let is_docked = docked == Some(child.entity_id());
                SidebarMenuItem::new(child.read(cx).title())
                    .active(is_main)
                    .when(is_docked, |item| item.border_1().border_color(p.blue))
                    .suffix(move |_, cx| status_dot(child_status, cx))
                    .on_click(cx.listener({
                        let child = child.clone();
                        move |app, event: &ClickEvent, window, cx| {
                            app.open_session(
                                child.clone(),
                                event.modifiers().secondary(),
                                window,
                                cx,
                            )
                        }
                    }))
                    .context_menu({
                        let app = cx.entity().downgrade();
                        let child = child.clone();
                        move |menu, _, _| {
                            let (a1, c1) = (app.clone(), child.clone());
                            let (a2, c2) = (app.clone(), child.clone());
                            let c3 = child.clone();
                            menu.item(PopupMenuItem::new("Open in right dock").on_click(
                                move |_, window, cx| {
                                    let _ = a1.update(cx, |app, cx| {
                                        app.open_session(c1.clone(), true, window, cx)
                                    });
                                },
                            ))
                            .item(PopupMenuItem::new("Stop").on_click(move |_, _, cx| {
                                c3.update(cx, |s, cx| s.stop(cx));
                            }))
                            .item(
                                PopupMenuItem::new("Copy task").on_click(move |_, _, cx| {
                                    let task = match &c2.read(cx).kind {
                                        crate::session::SessionKind::SubAgent { task, .. } => {
                                            task.clone()
                                        }
                                        _ => String::new(),
                                    };
                                    cx.write_to_clipboard(ClipboardItem::new_string(task));
                                    let _ = a2.update(cx, |_, _| {});
                                }),
                            )
                        }
                    })
            });

            SidebarMenuItem::new(worktree.branch.clone())
                .active(is_main)
                .when(is_docked, |item| item.border_1().border_color(p.blue))
                .default_open(true)
                .suffix(move |_, cx| status_dot(status, cx))
                .children(subagents.collect::<Vec<_>>())
                .on_click(cx.listener({
                    let session = session.clone();
                    move |app, event: &ClickEvent, window, cx| {
                        app.open_session(session.clone(), event.modifiers().secondary(), window, cx)
                    }
                }))
                .context_menu({
                    let app = cx.entity().downgrade();
                    let session = session.clone();
                    let is_main_worktree = worktree.is_main;
                    move |menu, _, _| {
                        let (a1, s1) = (app.clone(), session.clone());
                        let (a2, s2) = (app.clone(), session.clone());
                        let (a3, p3) = (app.clone(), path.clone());
                        let (a4, p4) = (app.clone(), path.clone());
                        let menu = menu
                            .item(PopupMenuItem::new("Open in right dock").on_click(
                                move |_, window, cx| {
                                    let _ = a1.update(cx, |app, cx| {
                                        app.open_session(s1.clone(), true, window, cx)
                                    });
                                },
                            ))
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
                            .item(PopupMenuItem::new("Rename branch").on_click(
                                move |_, window, cx| {
                                    let _ = a3.update(cx, |app, cx| {
                                        app.prompt_rename_branch(p3.clone(), window, cx)
                                    });
                                },
                            ));
                        if is_main_worktree {
                            menu
                        } else {
                            menu.separator()
                                .item(PopupMenuItem::new("Delete worktree…").on_click(
                                    move |_, window, cx| {
                                        let _ = a4.update(cx, |app, cx| {
                                            app.confirm_delete_worktree(p4.clone(), window, cx)
                                        });
                                    },
                                ))
                        }
                    }
                })
        });

        let open = !project.worktrees.is_empty();
        menu = menu.child(
            SidebarMenuItem::new(project.name.clone())
                .icon(
                    icon(if open {
                        IconName::FolderOpen
                    } else {
                        IconName::Folder
                    })
                    .text_color(p.subtext0),
                )
                .default_open(true)
                .click_to_toggle(true)
                .children(worktrees.collect::<Vec<_>>())
                .when_some(project.error.clone(), |item, error| {
                    item.suffix(move |_, cx| {
                        div()
                            .text_xs()
                            .text_color(palette(cx).red)
                            .child(SharedString::from(error.clone()))
                    })
                })
                .context_menu({
                    let app = cx.entity().downgrade();
                    move |menu, _, _| {
                        let (a1, p1) = (app.clone(), project_path.clone());
                        let p2 = project_path.clone();
                        let p3 = project_path.clone();
                        let (a4, p4) = (app.clone(), project_path.clone());
                        menu.item(PopupMenuItem::new("New worktree").on_click(
                            move |_, window, cx| {
                                let _ = a1.update(cx, |app, cx| {
                                    app.prompt_new_worktree(Some(p1.clone()), window, cx)
                                });
                            },
                        ))
                        .item(
                            PopupMenuItem::new("Reveal in file manager")
                                .on_click(move |_, _, cx| cx.reveal_path(&p2)),
                        )
                        .item(PopupMenuItem::new("Copy path").on_click(move |_, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(
                                p3.display().to_string(),
                            ))
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
                }),
        );
    }

    let empty = workspace.projects.is_empty();
    let connected = app.provider_label(cx);

    Sidebar::new("sidebar")
        .collapsible(false)
        .w(px(SIDEBAR_WIDTH))
        .header(
            SidebarHeader::new().w_full().child(
                h_flex()
                    .w_full()
                    .h(px(BAR_HEIGHT - 12.))
                    .gap(px(2.))
                    .child(
                        Button::new("toggle-sidebar")
                            .ghost()
                            .small()
                            .icon(icon(IconName::PanelLeft))
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
                        Button::new("command-palette")
                            .ghost()
                            .small()
                            .icon(icon(IconName::Search))
                            .tooltip_with_action("Command palette", &CommandPalette, None)
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(CommandPalette), cx)
                            }),
                    )
                    .child(
                        Button::new("toggle-theme")
                            .ghost()
                            .small()
                            .icon(icon(if cx.theme().is_dark() {
                                IconName::Sun
                            } else {
                                IconName::Moon
                            }))
                            .tooltip(toggle_label(cx))
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(ToggleTheme), cx)
                            }),
                    ),
            ),
        )
        .child(ProjectsSection {
            menu,
            empty,
            collapsed: false,
            on_add: Rc::new(|window, cx| window.dispatch_action(Box::new(OpenProject), cx)),
        })
        .footer(
            SidebarFooter::new().w_full().child(
                h_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        Button::new("settings")
                            .ghost()
                            .small()
                            .icon(icon(IconName::Settings))
                            .tooltip_with_action("Settings", &OpenSettings, None)
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(OpenSettings), cx)
                            }),
                    )
                    .child(div().flex_1())
                    .when_some(connected, |this, (label, ok)| {
                        this.child(
                            Button::new("provider")
                                .ghost()
                                .xsmall()
                                .child(
                                    h_flex()
                                        .gap(px(6.))
                                        .text_color(p.subtext0)
                                        .child(div().size(px(7.)).rounded_full().bg(if ok {
                                            p.green
                                        } else {
                                            p.red
                                        }))
                                        .child(label),
                                )
                                .tooltip(if ok {
                                    "Provider connected"
                                } else {
                                    "Provider needs attention"
                                })
                                .on_click(|_, window, cx| {
                                    window.dispatch_action(Box::new(OpenSettings), cx)
                                }),
                        )
                    }),
            ),
        )
}

type AddProject = dyn Fn(&mut Window, &mut App);

/// The "Projects" group: its label carries the add-project button, and it
/// shows the empty state when there are no projects yet.
#[derive(Clone)]
struct ProjectsSection {
    menu: SidebarMenu,
    empty: bool,
    collapsed: bool,
    on_add: Rc<AddProject>,
}

impl Collapsible for ProjectsSection {
    fn is_collapsed(&self) -> bool {
        self.collapsed
    }

    fn collapsed(mut self, collapsed: bool) -> Self {
        self.collapsed = collapsed;
        self
    }
}

impl SidebarItem for ProjectsSection {
    fn render(
        self,
        id: impl Into<ElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let p = palette(cx);
        let on_add = self.on_add.clone();
        let label = h_flex()
            .h_8()
            .pl_2()
            .justify_between()
            .text_size(px(13.))
            .text_color(p.subtext0)
            .child("Projects")
            .child(
                Button::new("add-project")
                    .ghost()
                    .xsmall()
                    .icon(icon(Lucide::FolderPlus))
                    .tooltip("Add project — choose a folder")
                    .on_click(move |_, window, cx| on_add(window, cx)),
            );

        let body = if self.empty {
            let on_add = self.on_add.clone();
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
                .child(div().text_size(px(13.)).text_color(p.subtext0).child(
                    "Open a folder to add your first project. Treehouse lists its worktrees here.",
                ))
                .child(
                    Button::new("open-folder")
                        .primary()
                        .small()
                        .label("Open folder…")
                        .on_click(move |_, window, cx| on_add(window, cx)),
                )
                .into_any_element()
        } else {
            self.menu.render(id, window, cx).into_any_element()
        };

        v_flex().child(label).child(body)
    }
}
