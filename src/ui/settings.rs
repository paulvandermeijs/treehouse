//! Settings (⌘,): gpui-kit's `Settings` in a dialog, one page per section.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::setting::{
    SettingField, SettingGroup, SettingItem, SettingPage, Settings,
};
use gpui_kit::component::{IconName, Sizable as _, WindowExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use crate::actions::SHORTCUTS;
use crate::approval::ApprovalMode;
use crate::config::{CompareBase, Config, THINKING_LEVELS, TOOLS, ThemeChoice};
use crate::pi::auth::{self, Credential, PROVIDERS};
use crate::theme::{MONO, palette};

/// The sections other views link to; the value is the page index.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Section {
    Providers,
    Defaults,
    Approvals,
}

pub fn open(section: Section, window: &mut Window, cx: &mut App) {
    window.open_dialog(cx, move |dialog, _, _| {
        dialog
            .w(px(860.))
            .margin_top(px(110.))
            .p_0()
            .content(move |content, _, _| content.child(div().h(px(588.)).child(settings(section))))
    });
}

fn opts(values: &[(&str, &str)]) -> Vec<(SharedString, SharedString)> {
    values
        .iter()
        .map(|(v, l)| {
            (
                SharedString::from(v.to_string()),
                SharedString::from(l.to_string()),
            )
        })
        .collect()
}

fn settings(section: Section) -> Settings {
    let index = section as usize;
    Settings::new("settings")
        .sidebar_width(px(200.))
        .default_selected_index(gpui_kit::component::setting::SelectIndex {
            page_ix: index,
            ..Default::default()
        })
        .pages(vec![
            providers_page(),
            defaults_page(),
            approvals_page(),
            worktrees_page(),
            appearance_page(),
            shortcuts_page(),
        ])
}

fn providers_page() -> SettingPage {
    let items = PROVIDERS.iter().map(|&(id, name, var)| {
        SettingItem::render(move |_, _, cx| provider_row(id, name, var, cx))
            .keywords([name, "API key", "provider"])
    });
    SettingPage::new("Providers")
        .icon(Lucide::KeyRound)
        .group(
            SettingGroup::new()
                .description(
                    "Keys are shared with Pi's own configuration (auth.json in Pi's agent folder), \
                     so a key added here also works when you run Pi in a terminal.",
                )
                .items(items.collect::<Vec<_>>()),
        )
        .group(
            SettingGroup::new().title("API keys").items(
                PROVIDERS
                    .iter()
                    .map(|&(id, name, _)| {
                        SettingItem::new(
                            format!("{name} API key"),
                            SettingField::input(
                                |_| SharedString::default(),
                                move |key: SharedString, cx| {
                                    if !key.trim().is_empty() {
                                        if let Err(err) = auth::save_key(id, &key) {
                                            eprintln!(
                                                "treehouse: couldn't save the {id} key: {err}"
                                            );
                                        }
                                        cx.refresh_windows();
                                    }
                                },
                            ),
                        )
                        .description("Paste a key and press Enter. It replaces the stored key.")
                    })
                    .collect::<Vec<_>>(),
            ),
        )
}

fn provider_row(
    id: &'static str,
    name: &'static str,
    var: &'static str,
    cx: &mut App,
) -> AnyElement {
    let p = palette(cx);
    let credential = auth::credential(id);
    let (detail, status, color): (String, String, Hsla) = match &credential {
        Credential::Stored(masked) => (masked.clone(), "Saved in Pi's auth.json".into(), p.green),
        Credential::OAuth => (
            "Signed in with Pi".into(),
            "OAuth credential".into(),
            p.green,
        ),
        Credential::Environment(var) => (
            format!("${var}"),
            "Read from the environment".into(),
            p.green,
        ),
        Credential::Missing => (
            "Not set up".into(),
            format!("Paste a key below, or set ${var}"),
            p.subtext0,
        ),
    };
    let removable = matches!(credential, Credential::Stored(_) | Credential::OAuth);
    h_flex()
        .w_full()
        .gap_4()
        .child(div().w(px(140.)).font_medium().child(name))
        .child(
            v_flex()
                .flex_1()
                .min_w_0()
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(12.5))
                        .text_color(p.subtext1)
                        .child(detail),
                )
                .child(
                    h_flex()
                        .gap(px(6.))
                        .text_size(px(12.))
                        .text_color(p.subtext0)
                        .child(div().size(px(7.)).rounded_full().bg(color))
                        .child(status),
                ),
        )
        .when(removable, |this| {
            this.child(
                Button::new(("remove-key", id.len()))
                    .ghost()
                    .small()
                    .label("Remove")
                    .on_click(move |_, _, cx| {
                        let _ = auth::remove(id);
                        cx.refresh_windows();
                    }),
            )
        })
        .into_any_element()
}

fn defaults_page() -> SettingPage {
    let thinking: Vec<(SharedString, SharedString)> = THINKING_LEVELS
        .iter()
        .map(|l| (SharedString::from(*l), SharedString::from(capitalize(l))))
        .collect();
    let approvals: Vec<(SharedString, SharedString)> = ApprovalMode::ALL
        .iter()
        .map(|m| {
            (
                SharedString::from(format!("{m:?}")),
                SharedString::from(m.label()),
            )
        })
        .collect();
    let tools = TOOLS.iter().map(|&tool| {
        SettingItem::new(
            tool,
            SettingField::checkbox(
                move |cx| Config::global(cx).tools.iter().any(|t| t == tool),
                move |on, cx| {
                    Config::update(cx, |c| {
                        c.tools.retain(|t| t != tool);
                        if on {
                            c.tools.push(tool.to_string());
                        }
                    })
                },
            ),
        )
    });
    SettingPage::new("Agent defaults")
        .icon(Lucide::Bot)
        .group(
            SettingGroup::new()
                .description("Used for new worktrees and sub-agents. You can still change them per chat in the toolbar under the message box.")
                .item(
                    SettingItem::new(
                        "Model",
                        SettingField::input(
                            |cx| Config::global(cx).model.clone().unwrap_or_default().into(),
                            |v: SharedString, cx| Config::update(cx, |c| c.model = Some(v.to_string()).filter(|m| !m.trim().is_empty())),
                        ),
                    )
                    .description("provider/model for new chats, e.g. anthropic/claude-opus-4-5. Empty uses Pi's default."),
                )
                .item(
                    SettingItem::new(
                        "Sub-agent model",
                        SettingField::input(
                            |cx| Config::global(cx).subagent_model.clone().unwrap_or_default().into(),
                            |v: SharedString, cx| {
                                Config::update(cx, |c| c.subagent_model = Some(v.to_string()).filter(|m| !m.trim().is_empty()))
                            },
                        ),
                    )
                    .description("Cheaper models are often enough. Empty means the same as the chat."),
                )
                .item(SettingItem::new(
                    "Thinking level",
                    SettingField::dropdown(
                        thinking,
                        |cx| Config::global(cx).thinking.clone().into(),
                        |v: SharedString, cx| Config::update(cx, |c| c.thinking = v.to_string()),
                    ),
                ))
                .item(
                    SettingItem::new(
                        "Approval",
                        SettingField::dropdown(
                            approvals,
                            |cx| format!("{:?}", Config::global(cx).approval).into(),
                            |v: SharedString, cx| {
                                let mode = ApprovalMode::ALL.into_iter().find(|m| format!("{m:?}") == v.as_ref());
                                Config::update(cx, |c| c.approval = mode.unwrap_or_default())
                            },
                        ),
                    )
                    .description("When Pi has to ask first"),
                )
                .item(
                    SettingItem::new(
                        "Pi command",
                        SettingField::input(
                            |cx| Config::global(cx).pi_command.clone().into(),
                            |v: SharedString, cx| Config::update(cx, |c| c.pi_command = v.to_string()),
                        ),
                    )
                    .description("The pi executable. Install it with npm install -g @earendil-works/pi-coding-agent."),
                ),
        )
        .group(SettingGroup::new().title("Tools").description("What Pi may use").items(tools.collect::<Vec<_>>()))
}

fn approvals_page() -> SettingPage {
    SettingPage::new("Approvals")
        .icon(Lucide::Shield)
        .group(
            SettingGroup::new()
                .title("Always allowed")
                .item(
                    SettingItem::render(|_, _, cx| allowed_list(cx))
                        .keywords(["pattern", "allow", "bash"]),
                )
                .footer(
                    |_, _| "Added with \"Always allow\" on a bash approval. Use * as a wildcard.",
                ),
        )
        .group(
            SettingGroup::new()
                .title("Always ask, whatever the approval setting")
                .item(SettingItem::new(
                    "Deleting files (rm, git clean)",
                    SettingField::switch(
                        |cx| Config::global(cx).always_ask.deleting,
                        |v, cx| Config::update(cx, |c| c.always_ask.deleting = v),
                    ),
                ))
                .item(SettingItem::new(
                    "Pushing (git push, deploys)",
                    SettingField::switch(
                        |cx| Config::global(cx).always_ask.pushing,
                        |v, cx| Config::update(cx, |c| c.always_ask.pushing = v),
                    ),
                ))
                .item(SettingItem::new(
                    "Touching files outside the worktree",
                    SettingField::switch(
                        |cx| Config::global(cx).always_ask.outside_worktree,
                        |v, cx| Config::update(cx, |c| c.always_ask.outside_worktree = v),
                    ),
                )),
        )
}

fn allowed_list(cx: &mut App) -> AnyElement {
    let p = palette(cx);
    let allowed = Config::global(cx).allowed.clone();
    v_flex()
        .w_full()
        .when(allowed.is_empty(), |this| {
            this.child(
                div()
                    .text_size(px(13.))
                    .text_color(p.subtext0)
                    .child("Nothing yet."),
            )
        })
        .children(allowed.into_iter().enumerate().map(|(ix, allowed)| {
            let scope = allowed
                .project
                .as_ref()
                .and_then(|p| p.file_name())
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| "all projects".into());
            let pattern = allowed.pattern.clone();
            h_flex()
                .h(px(38.))
                .gap_3()
                .child(
                    div()
                        .font_family(MONO)
                        .text_size(px(12.5))
                        .child(pattern.clone()),
                )
                .child(div().flex_1())
                .child(div().text_size(px(12.)).text_color(p.subtext0).child(scope))
                .child(
                    Button::new(("remove-pattern", ix))
                        .ghost()
                        .xsmall()
                        .icon(IconName::Close)
                        .tooltip(format!("Remove {pattern}"))
                        .on_click(move |_, _, cx| {
                            Config::update(cx, |c| {
                                if ix < c.allowed.len() {
                                    c.allowed.remove(ix);
                                }
                            })
                        }),
                )
        }))
        .into_any_element()
}

fn worktrees_page() -> SettingPage {
    SettingPage::new("Worktrees").icon(Lucide::GitBranch).group(
        SettingGroup::new()
            .item(
                SettingItem::new(
                    "Location",
                    SettingField::input(
                        |cx| Config::global(cx).worktree_location.clone().into(),
                        |v: SharedString, cx| {
                            Config::update(cx, |c| c.worktree_location = v.to_string())
                        },
                    ),
                )
                .description("Where new worktrees go. {project} and {branch} are filled in."),
            )
            .item(
                SettingItem::new(
                    "Compare against",
                    SettingField::dropdown(
                        opts(&[
                            ("created_from", "The branch it was created from"),
                            ("origin_main", "origin/main"),
                        ]),
                        |cx| match Config::global(cx).compare_base {
                            CompareBase::CreatedFrom => "created_from".into(),
                            CompareBase::OriginMain => "origin_main".into(),
                        },
                        |v: SharedString, cx| {
                            Config::update(cx, |c| {
                                c.compare_base = if v == "origin_main" {
                                    CompareBase::OriginMain
                                } else {
                                    CompareBase::CreatedFrom
                                }
                            })
                        },
                    ),
                )
                .description("Base for the diff and \"+/−\" counts"),
            )
            .item(
                SettingItem::new(
                    "Setup command",
                    SettingField::input(
                        |cx| Config::global(cx).setup_command.clone().into(),
                        |v: SharedString, cx| {
                            Config::update(cx, |c| c.setup_command = v.to_string())
                        },
                    ),
                )
                .description("Runs once after a worktree is created, e.g. npm install"),
            )
            .item(
                SettingItem::new(
                    "Also delete its folder and branch",
                    SettingField::switch(
                        |cx| Config::global(cx).delete_on_remove,
                        |v, cx| Config::update(cx, |c| c.delete_on_remove = v),
                    ),
                )
                .description("When removing a worktree"),
            )
            .item(
                SettingItem::render(|_, _, cx| {
                    let p = palette(cx);
                    let name = Config::global(cx)
                        .projects
                        .first()
                        .and_then(|p| crate::git::user_name(p))
                        .unwrap_or_else(|| "Not set".into());
                    h_flex()
                        .w_full()
                        .justify_between()
                        .child(
                            v_flex()
                                .child(div().font_medium().child("Commit author"))
                                .child(div().text_size(px(12.)).text_color(p.subtext0).child(
                                    "Read from git config user.name; can differ per project",
                                )),
                        )
                        .child(name)
                })
                .keywords(["git", "author", "name"]),
            ),
    )
}

fn appearance_page() -> SettingPage {
    SettingPage::new("Appearance").icon(Lucide::Palette).group(
        SettingGroup::new()
            .item(
                SettingItem::new(
                    "Theme",
                    SettingField::dropdown(
                        opts(&[
                            ("mocha", "Mocha"),
                            ("latte", "Latte"),
                            ("system", "Match system"),
                        ]),
                        |cx| match Config::global(cx).theme {
                            ThemeChoice::Mocha => "mocha".into(),
                            ThemeChoice::Latte => "latte".into(),
                            ThemeChoice::System => "system".into(),
                        },
                        |v: SharedString, cx| {
                            let choice = match v.as_ref() {
                                "latte" => ThemeChoice::Latte,
                                "system" => ThemeChoice::System,
                                _ => ThemeChoice::Mocha,
                            };
                            Config::update(cx, |c| c.theme = choice);
                            crate::theme::apply(choice, None, cx);
                        },
                    ),
                )
                .description("Catppuccin"),
            )
            .item(SettingItem::new(
                "Text size",
                SettingField::dropdown(
                    opts(&[("13", "13"), ("14", "14"), ("15", "15"), ("16", "16")]),
                    |cx| format!("{}", Config::global(cx).text_size as u32).into(),
                    |v: SharedString, cx| {
                        let size = v.parse().unwrap_or(14.0);
                        Config::update(cx, |c| c.text_size = size);
                        let theme = Config::global(cx).theme;
                        crate::theme::apply(theme, None, cx);
                    },
                ),
            ))
            .item(SettingItem::new(
                "Keep diffs expanded after a turn",
                SettingField::switch(
                    |cx| Config::global(cx).keep_diffs_expanded,
                    |v, cx| Config::update(cx, |c| c.keep_diffs_expanded = v),
                ),
            ))
            .item(SettingItem::new(
                "Always expand thinking",
                SettingField::switch(
                    |cx| Config::global(cx).always_expand_thinking,
                    |v, cx| Config::update(cx, |c| c.always_expand_thinking = v),
                ),
            )),
    )
}

fn shortcuts_page() -> SettingPage {
    let items = SHORTCUTS.iter().map(|&(label, keys)| {
        SettingItem::render(move |_, _, _| {
            h_flex()
                .w_full()
                .justify_between()
                .child(label)
                .children(Keystroke::parse(keys).ok().map(Kbd::new))
        })
        .keywords([label])
    });
    SettingPage::new("Shortcuts").icon(Lucide::Keyboard).group(
        SettingGroup::new()
            .items(items.collect::<Vec<_>>())
            .footer(|_, _| "Hold Ctrl (⌘ on macOS) and click a worktree or sub-agent to open it in the right dock."),
    )
}

fn capitalize(s: &str) -> String {
    let mut chars = s.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}
