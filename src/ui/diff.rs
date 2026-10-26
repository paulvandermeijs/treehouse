//! The worktree diff in the right dock, built on gpui-kit's `Diff`.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonGroup, ButtonVariants as _};
use gpui_kit::component::diff::{Diff, DiffFile, DiffMode, DiffState};
use gpui_kit::component::{IconName, Selectable as _, Sizable as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::PathBuf;

use super::{BAR_HEIGHT, icon};
use crate::actions::FullScreenDock;
use crate::git::{self, DiffStat};
use crate::theme::{MONO, palette};

pub enum DiffPanelEvent {
    Close,
    /// Commit with Pi or Finish branch, sent to the worktree's chat.
    Run(crate::slash::Lifecycle),
}

pub struct DiffPanel {
    pub worktree: PathBuf,
    branch: SharedString,
    base: String,
    state: Entity<DiffState>,
    stat: DiffStat,
    mode: DiffMode,
    loading: bool,
    error: Option<String>,
    revision: u64,
    pending_scroll: Option<String>,
    pub full_screen: bool,
    /// The main worktree has nothing to finish.
    pub is_main: bool,
    /// See `ChatView::sidebar_lead`.
    pub sidebar_lead: Option<Pixels>,
    /// The panel's width when last painted; see `render`.
    width: std::rc::Rc<std::cell::Cell<Pixels>>,
}

impl EventEmitter<DiffPanelEvent> for DiffPanel {}

impl DiffPanel {
    pub fn new(
        worktree: PathBuf,
        branch: SharedString,
        base: String,
        cx: &mut Context<Self>,
    ) -> Self {
        let state = cx.new(|cx| DiffState::new(Vec::new(), cx));
        let mut this = Self {
            worktree,
            branch,
            base,
            state,
            stat: DiffStat::default(),
            mode: DiffMode::Unified,
            loading: true,
            error: None,
            revision: 0,
            pending_scroll: None,
            full_screen: false,
            is_main: false,
            sidebar_lead: None,
            width: Default::default(),
        };
        this.reload(cx);
        this
    }

    /// Re-reads the diff on a background thread. A stale result is dropped.
    pub fn reload(&mut self, cx: &mut Context<Self>) {
        self.revision += 1;
        let revision = self.revision;
        let (worktree, base) = (self.worktree.clone(), self.base.clone());
        self.loading = true;
        cx.spawn(async move |this, cx| {
            let result = cx
                .background_spawn(async move {
                    let patch = git::diff_since(&worktree, &base)?;
                    let files = DiffFile::parse(&patch).map_err(|e| anyhow::anyhow!("{e:?}"))?;
                    anyhow::Ok((git::diff_stat(&patch), files))
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.revision != revision {
                    return;
                }
                this.loading = false;
                match result {
                    Ok((stat, files)) => {
                        this.stat = stat;
                        this.error = None;
                        this.state
                            .update(cx, |state, cx| state.set_files(files, cx));
                        if let Some(path) = this.pending_scroll.take() {
                            this.state
                                .update(cx, |state, cx| state.scroll_to_file(&path, cx));
                        }
                    }
                    Err(err) => this.error = Some(format!("{err:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    pub fn scroll_to_file(&mut self, path: &str, cx: &mut Context<Self>) {
        if self.loading {
            self.pending_scroll = Some(path.to_string());
        } else {
            self.state
                .update(cx, |state, cx| state.scroll_to_file(path, cx));
        }
    }

    pub fn set_full_screen(&mut self, full_screen: bool, cx: &mut Context<Self>) {
        self.full_screen = full_screen;
        // Full screen has the room for split diffs.
        self.set_mode(
            if full_screen {
                DiffMode::Split
            } else {
                DiffMode::Unified
            },
            cx,
        );
    }

    fn set_mode(&mut self, mode: DiffMode, cx: &mut Context<Self>) {
        self.mode = mode;
        self.state.update(cx, |state, cx| state.set_mode(mode, cx));
        cx.notify();
    }
}

impl Render for DiffPanel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let unified = self.mode == DiffMode::Unified;
        let empty = !self.loading && self.error.is_none() && self.stat.files == 0;

        // The kit's diff learns its new width while painting and asks for
        // another frame from there, which the window ignores mid-draw. On
        // macOS nothing else draws until the next input, so after a resize
        // (full screen, the sidebar) the diff kept its old width. Schedule
        // that frame ourselves.
        let width = self.width.clone();
        let watch_width = canvas(
            move |bounds, window, _| {
                if width.replace(bounds.size.width) != bounds.size.width {
                    window.on_next_frame(|window, _| window.refresh());
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        v_flex()
            .relative()
            .size_full()
            .min_w_0()
            .bg(p.base)
            .child(watch_width)
            .child(
                super::drags_window(h_flex().id("diff-top"))
                    .h(px(BAR_HEIGHT))
                    .flex_none()
                    .gap_2()
                    .px_2()
                    .when_some(self.sidebar_lead, |this, lead| {
                        this.pl(lead).child(super::sidebar_toggle("diff-sidebar"))
                    })
                    .border_b_1()
                    .border_color(p.surface0)
                    .text_size(px(13.))
                    .child(
                        Button::new("dock-full")
                            .ghost()
                            .small()
                            .icon(icon(if self.full_screen {
                                Lucide::Minimize2
                            } else {
                                Lucide::Maximize2
                            }))
                            .tooltip_with_action(
                                if self.full_screen {
                                    "Exit full screen"
                                } else {
                                    "Full screen"
                                },
                                &FullScreenDock,
                                None,
                            )
                            .on_click(|_, window, cx| {
                                window.dispatch_action(Box::new(FullScreenDock), cx)
                            }),
                    )
                    .child(div().font_medium().child("Changes"))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(p.subtext0)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            // The main worktree is on the base: only what's uncommitted.
                            .child(if self.is_main {
                                "Uncommitted changes".to_string()
                            } else {
                                format!("{} since {}", self.branch, self.base)
                            }),
                    )
                    .child(
                        Button::new("commit-with-pi")
                            .ghost()
                            .small()
                            .icon(icon(Lucide::GitCommitHorizontal))
                            .tooltip("Commit with Pi · /commit")
                            .on_click(cx.listener(|_, _, _, cx| {
                                cx.emit(DiffPanelEvent::Run(crate::slash::Lifecycle::Commit))
                            })),
                    )
                    .when(!self.is_main, |this| {
                        this.child(
                            Button::new("finish-branch")
                                .ghost()
                                .small()
                                .icon(icon(Lucide::GitMerge))
                                .tooltip("Finish branch · /finish-branch")
                                .on_click(cx.listener(|_, _, _, cx| {
                                    cx.emit(DiffPanelEvent::Run(
                                        crate::slash::Lifecycle::FinishBranch,
                                    ))
                                })),
                        )
                    })
                    .child(
                        Button::new("refresh")
                            .ghost()
                            .small()
                            .icon(icon(IconName::Redo))
                            .tooltip("Refresh")
                            .on_click(cx.listener(|this, _, _, cx| this.reload(cx))),
                    )
                    .child(
                        Button::new("dock-close")
                            .ghost()
                            .small()
                            .icon(icon(Lucide::X))
                            .tooltip("Close diff")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(DiffPanelEvent::Close))),
                    ),
            )
            .child(
                h_flex()
                    .h(px(44.))
                    .flex_none()
                    .gap_2()
                    .px_4()
                    .border_b_1()
                    .border_color(p.surface0)
                    .child(
                        ButtonGroup::new("diff-layout")
                            .outline()
                            .small()
                            .child(Button::new("unified").label("Unified").selected(unified))
                            .child(Button::new("split").label("Split").selected(!unified))
                            .on_click(cx.listener(|this, selected: &Vec<usize>, _, cx| {
                                let mode = if selected.first() == Some(&1) {
                                    DiffMode::Split
                                } else {
                                    DiffMode::Unified
                                };
                                this.set_mode(mode, cx);
                            })),
                    )
                    .child(
                        ButtonGroup::new("diff-nav")
                            .outline()
                            .small()
                            .child(
                                Button::new("previous-change")
                                    .icon(icon(IconName::ArrowUp).size(px(14.)))
                                    .tooltip("Previous change"),
                            )
                            .child(
                                Button::new("next-change")
                                    .icon(icon(IconName::ArrowDown).size(px(14.)))
                                    .tooltip("Next change"),
                            )
                            .on_click(cx.listener(|this, selected: &Vec<usize>, _, cx| {
                                let next = selected.first() == Some(&1);
                                this.state.update(cx, |state, cx| {
                                    if next {
                                        state.next_change(cx)
                                    } else {
                                        state.previous_change(cx)
                                    }
                                });
                            })),
                    )
                    .child(div().flex_1())
                    .child(
                        h_flex()
                            .gap_1()
                            .font_family(MONO)
                            .text_size(px(12.))
                            .child(
                                div()
                                    .text_color(p.add)
                                    .child(format!("+{}", self.stat.additions)),
                            )
                            .child(
                                div()
                                    .text_color(p.delete)
                                    .child(format!("−{}", self.stat.deletions)),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_1()
                    .min_h_0()
                    .when_some(self.error.clone(), |this, error| {
                        this.child(
                            div()
                                .p_4()
                                .text_color(p.red)
                                .child(SharedString::from(error)),
                        )
                    })
                    .when(empty, |this| {
                        this.child(
                            v_flex()
                                .items_center()
                                .pt(px(96.))
                                .gap_2()
                                .text_color(p.subtext0)
                                .child(icon(Lucide::GitBranch).size(px(24.)))
                                .child(if self.is_main {
                                    "Nothing uncommitted".to_string()
                                } else {
                                    format!("No changes since {}", self.base)
                                }),
                        )
                    })
                    .when(!empty && self.error.is_none(), |this| {
                        // The dock draws its own edges; drop the viewer's outer border.
                        // Split columns are as wide as the longest line, so one long
                        // line pushes the right side out of view; wrap instead.
                        this.child(
                            Diff::new(&self.state)
                                .size_full()
                                .border_0()
                                .soft_wrap(self.mode == DiffMode::Split),
                        )
                    }),
            )
    }
}
