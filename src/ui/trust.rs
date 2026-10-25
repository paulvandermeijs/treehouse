//! The trust dialog: whether Pi may change a project folder.
//!
//! Untrusted projects run Pi read-only. The dialog opens the first time a
//! project's worktree is opened, and again from the chat's "Read only"
//! button, the read-only empty state and the toolbar's tools button.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::checkbox::Checkbox;
use gpui_kit::component::{WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cell::Cell;
use std::path::{Path, PathBuf};
use std::rc::Rc;

use super::icon;
use crate::theme::{MONO, mix, palette};
use crate::trust::Decision;
use crate::workspace::Workspace;

/// Shows a path with the home folder as `~`.
pub fn display_path(path: &Path) -> String {
    match dirs::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Some(rest) => format!("~/{}", rest.display()),
        None => path.display().to_string(),
    }
}

/// Opens the dialog for a project. `first_open` asks for a decision before
/// anything happens; otherwise the project already runs read-only.
pub fn open(
    workspace: Entity<Workspace>,
    project: PathBuf,
    first_open: bool,
    window: &mut Window,
    cx: &mut App,
) {
    let name = project
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_default();
    let parent = project.parent().map(display_path);
    let whole_parent = Rc::new(Cell::new(false));

    let decide = {
        let (workspace, project) = (workspace.clone(), project.clone());
        Rc::new(
            move |decision: Decision, window: &mut Window, cx: &mut App| {
                let saved = workspace.update(cx, |w, cx| w.set_trust(&project, decision, cx));
                if let Err(err) = saved {
                    window.push_notification(
                        format!("Couldn't save the trust decision: {err:#}"),
                        cx,
                    );
                }
            },
        )
    };

    window.open_dialog(cx, move |dialog, _, cx| {
        let p = palette(cx);
        let trust = {
            let (decide, whole_parent) = (decide.clone(), whole_parent.clone());
            move |window: &mut Window, cx: &mut App| {
                let decision = if whole_parent.get() {
                    Decision::TrustParent
                } else {
                    Decision::Trust
                };
                decide(decision, window, cx);
            }
        };
        let trust_on_ok = trust.clone();
        let read_only = decide.clone();
        let whole = whole_parent.clone();
        let (path, parent) = (display_path(&project), parent.clone());
        let subtitle = if first_open {
            format!("You're opening {name} for the first time. Do you trust the code in this folder?")
        } else {
            format!("{name} was opened without trust, so Pi can only read it. Trust it to let Pi make changes.")
        };
        let bullet = |text: &'static str| {
            h_flex()
                .items_start()
                .gap_2()
                .child(div().flex_none().w(px(10.)).child("•"))
                .child(div().flex_1().child(text))
        };

        dialog
            .w(px(520.))
            .margin_top(px(150.))
            .p_0()
            .close_button(false)
            .on_ok(move |_, window, cx| {
                trust_on_ok(window, cx);
                true
            })
            .content(move |content, _, _| {
                let trust = trust.clone();
                let read_only = read_only.clone();
                let whole = whole.clone();
                content.child(
                    v_flex()
                        .text_size(px(13.5))
                        .child(
                            h_flex()
                                .items_start()
                                .gap_3()
                                .px_5()
                                .pt_5()
                                .child(
                                    div()
                                        .flex_none()
                                        .size(px(36.))
                                        .rounded(px(10.))
                                        .bg(mix(p.yellow, p.base, 0.14))
                                        .flex()
                                        .items_center()
                                        .justify_center()
                                        .child(icon(Lucide::ShieldAlert).size(px(18.)).text_color(p.yellow)),
                                )
                                .child(
                                    v_flex()
                                        .flex_1()
                                        .min_w_0()
                                        .gap(px(2.))
                                        .child(div().text_size(px(16.)).font_semibold().child("Trust this folder?"))
                                        .child(
                                            div()
                                                .text_size(px(13.))
                                                .text_color(p.subtext0)
                                                .child(SharedString::from(subtitle.clone())),
                                        ),
                                ),
                        )
                        .child(
                            v_flex()
                                .gap_3()
                                .px_5()
                                .pt(px(14.))
                                .pb_1()
                                .child(
                                    h_flex()
                                        .gap_2()
                                        .px(px(10.))
                                        .py_2()
                                        .rounded(px(6.))
                                        .bg(p.mantle)
                                        .border_1()
                                        .border_color(p.surface0)
                                        .font_family(MONO)
                                        .text_size(px(12.5))
                                        .child(icon(Lucide::Folder).text_color(p.subtext0))
                                        .child(div().flex_1().min_w_0().truncate().child(SharedString::from(path.clone()))),
                                )
                                .child(
                                    v_flex()
                                        .child("Trusting lets Pi:")
                                        .child(
                                            v_flex()
                                                .pl(px(6.))
                                                .gap(px(3.))
                                                .child(bullet("edit, create and delete files in its worktrees"))
                                                .child(bullet("run commands, including the project's own scripts"))
                                                .child(bullet("load the project's AGENTS.md, Pi settings and skills"))
                                                .child(bullet("run the worktree setup command")),
                                        ),
                                )
                                .child(
                                    div()
                                        .text_size(px(12.5))
                                        .text_color(p.subtext0)
                                        .child("Only trust code you know. Without trust, Pi can still read files and answer questions."),
                                )
                                .when_some(parent.clone(), |this, parent| {
                                    let whole = whole.clone();
                                    this.child(
                                        Checkbox::new("trust-parent")
                                            .checked(whole.get())
                                            .accessibility_label(format!("Trust every folder in {parent}"))
                                            .child(
                                                h_flex()
                                                    .min_w_0()
                                                    .gap_1()
                                                    .text_size(px(13.))
                                                    .child(div().flex_none().child("Trust every folder in"))
                                                    .child(
                                                        div()
                                                            .min_w_0()
                                                            .truncate()
                                                            .font_family(MONO)
                                                            .text_size(px(12.))
                                                            .child(SharedString::from(parent)),
                                                    ),
                                            )
                                            .on_click(move |checked, window, _| {
                                                whole.set(*checked);
                                                window.refresh();
                                            }),
                                    )
                                }),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .px_5()
                                .pt(px(14.))
                                .pb(px(18.))
                                .when(first_open, |this| {
                                    let read_only = read_only.clone();
                                    this.child(
                                        Button::new("trust-cancel")
                                            .ghost()
                                            .label("Cancel")
                                            .on_click(|_, window, cx| window.close_dialog(cx)),
                                    )
                                    .child(div().flex_1())
                                    .child(Button::new("trust-read-only").outline().label("Open read-only").on_click(
                                        move |_, window, cx| {
                                            read_only(Decision::ReadOnly, window, cx);
                                            window.close_dialog(cx);
                                        },
                                    ))
                                })
                                .when(!first_open, |this| {
                                    this.child(div().flex_1()).child(
                                        Button::new("trust-keep")
                                            .ghost()
                                            .label(kbd_label("Keep read-only", "esc"))
                                            .on_click(|_, window, cx| window.close_dialog(cx)),
                                    )
                                })
                                .child(
                                    Button::new("trust-ok")
                                        .primary()
                                        .label(kbd_label("Trust folder", "⏎"))
                                        .on_click(move |_, window, cx| {
                                            trust(window, cx);
                                            window.close_dialog(cx);
                                        }),
                                ),
                        ),
                )
            })
    });
}

fn kbd_label(label: &str, key: &str) -> SharedString {
    format!("{label}  {key}").into()
}
