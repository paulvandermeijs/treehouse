//! Settings (⌘,): gpui-kit's `Settings` in a dialog, one page per section.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::Disableable as _;
use gpui_kit::component::StyledExt as _;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::input::{Input, InputEvent, InputState};
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
            // The dialog's own close button sits 8px from the top whatever
            // the header; ours is centred on the 52px page heading instead.
            .close_button(false)
            // Settings has no OK button: Enter in a field, such as saving an
            // API key, mustn't close the dialog.
            .on_ok(|_, _, _| false)
            .content(move |content, _, _| {
                content.child(
                    div().relative().h(px(588.)).child(settings(section)).child(
                        div()
                            .absolute()
                            .top_0()
                            .right(px(10.))
                            .h(px(HEADER_HEIGHT))
                            .flex()
                            .items_center()
                            .child(
                                Button::new("close-settings")
                                    .ghost()
                                    .small()
                                    .icon(IconName::Close)
                                    .tooltip("Close · Esc")
                                    .on_click(|_, window, cx| window.close_dialog(cx)),
                            ),
                    ),
                )
            })
    });
}

/// The page heading's height, as the design's settings dialog has it.
const HEADER_HEIGHT: f32 = 52.;

/// A settings page with the design's heading: 52px high, title centred.
fn page(title: &'static str) -> SettingPage {
    let style = StyleRefinement::default()
        .h(px(HEADER_HEIGHT))
        .py_0()
        .pl(px(24.))
        .pr(px(48.))
        .justify_center();
    SettingPage::new(title).header_style(&style)
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
    let mut keywords = vec!["API key", "provider", "key"];
    keywords.extend(PROVIDERS.iter().map(|&(_, name, _)| name));
    page("Providers").icon(Lucide::KeyRound).group(
        SettingGroup::new()
            .description(
                "Keys are shared with Pi's own configuration (auth.json in Pi's agent folder), \
                 so a key added here also works when you run Pi in a terminal.",
            )
            .item(
                SettingItem::render(|_, window, cx| providers_list(window, cx)).keywords(keywords),
            ),
    )
}

/// What one provider's row holds between renders: the key being typed, and
/// whether it is open for a new key, confirming a sign-out or showing an
/// error from the last save.
struct KeyRow {
    input: Entity<InputState>,
    /// Replace or Add key was pressed on a provider that already has a key.
    editing: bool,
    confirm_sign_out: bool,
    error: Option<String>,
    _subscription: Subscription,
}

impl KeyRow {
    fn new(id: &'static str, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| {
            InputState::new(window, cx)
                .masked(true)
                .placeholder(new_key_placeholder(id))
        });
        let _subscription =
            cx.subscribe_in(
                &input,
                window,
                move |this, _, event, window, cx| match event {
                    InputEvent::PressEnter { .. } => this.save(id, window, cx),
                    InputEvent::Change => cx.notify(),
                    _ => {}
                },
            );
        Self {
            input,
            editing: false,
            confirm_sign_out: false,
            error: None,
            _subscription,
        }
    }

    fn typed(&self, cx: &App) -> String {
        self.input.read(cx).value().trim().to_string()
    }

    /// Saves what was typed; nothing saves on blur, so a half-pasted key
    /// is never stored and a failure has a place to show.
    fn save(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let key = self.typed(cx);
        if key.is_empty() {
            return;
        }
        match auth::save_key(id, &key) {
            Ok(()) => {
                self.input
                    .update(cx, |input, cx| input.set_value("", window, cx));
                self.editing = false;
                self.error = None;
            }
            Err(err) => self.error = Some(format!("Couldn't write auth.json: {err}")),
        }
        cx.refresh_windows();
    }

    /// Opens the input for a provider that already has a key.
    fn edit(&mut self, placeholder: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = true;
        self.error = None;
        let placeholder = SharedString::from(placeholder.to_string());
        self.input.update(cx, |input, cx| {
            input.set_placeholder(placeholder, window, cx);
            input.focus(window, cx);
        });
        cx.notify();
    }

    fn cancel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.editing = false;
        self.error = None;
        self.input
            .update(cx, |input, cx| input.set_value("", window, cx));
        cx.notify();
    }
}

/// The input's placeholder shows the shape of the provider's keys.
fn new_key_placeholder(id: &str) -> &'static str {
    match id {
        "anthropic" => "Paste an API key (sk-ant-…)",
        "openai" => "Paste an API key (sk-…)",
        "google" => "Paste an API key (AIza…)",
        "openrouter" => "Paste an API key (sk-or-…)",
        _ => "Paste an API key",
    }
}

/// Every provider in one bordered list, a row each: its state and the place
/// to change it, so saving a key changes the row in place.
fn providers_list(window: &mut Window, cx: &mut App) -> AnyElement {
    let p = palette(cx);
    let rows: Vec<AnyElement> = PROVIDERS
        .iter()
        .enumerate()
        .map(|(ix, &(id, name, var))| provider_row(ix, id, name, var, window, cx))
        .collect();
    v_flex()
        .w_full()
        .rounded(px(8.))
        .border_1()
        .border_color(p.surface0)
        .bg(p.mantle)
        .overflow_hidden()
        .children(rows)
        .into_any_element()
}

fn provider_row(
    ix: usize,
    id: &'static str,
    name: &'static str,
    var: &'static str,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let p = palette(cx);
    let row = window.use_keyed_state(
        SharedString::from(format!("key-row-{id}")),
        cx,
        move |window, cx| KeyRow::new(id, window, cx),
    );
    let credential = auth::credential(id);
    let (editing, confirm_sign_out, error, typed, input) = {
        let r = row.read(cx);
        (
            r.editing,
            r.confirm_sign_out,
            r.error.clone(),
            r.typed(cx),
            r.input.clone(),
        )
    };
    let in_environment = std::env::var_os(var).is_some();
    let open = editing || credential == Credential::Missing;

    // The row's state under the name, and the line under the key or input.
    let state = if error.is_some() {
        "Not saved"
    } else if open && !typed.is_empty() {
        "Not saved yet"
    } else {
        match &credential {
            Credential::Stored(_) => "Saved",
            Credential::OAuth => "Signed in",
            Credential::Environment(_) => "From the environment",
            Credential::Missing => "Not set up",
        }
    };
    let status = |color: Option<Hsla>, text: String| {
        h_flex()
            .gap(px(6.))
            .text_size(px(12.))
            .text_color(if color == Some(p.red) {
                p.red
            } else {
                p.subtext0
            })
            .when_some(color, |this, color| {
                this.child(div().flex_none().size(px(7.)).rounded_full().bg(color))
            })
            .child(text)
    };

    let middle = if open {
        let hint = match (&error, &credential) {
            (Some(error), _) => status(Some(p.red), error.clone()),
            _ if !typed.is_empty() => status(None, "Press Enter to save".into()),
            (_, Credential::Stored(masked)) => status(None, format!("Replaces {masked}")),
            (_, Credential::Environment(var)) => {
                status(None, format!("Used instead of ${var} once saved"))
            }
            _ => status(None, format!("or set ${var}")),
        };
        v_flex()
            .gap_1()
            .child(
                Input::new(&input)
                    .small()
                    .mask_toggle()
                    .aria_label(format!("{name} API key"))
                    .w_full(),
            )
            .child(hint)
    } else {
        let (detail, line) = match &credential {
            Credential::Stored(masked) => (
                masked.clone(),
                if in_environment {
                    format!("Saved · used instead of ${var}")
                } else {
                    "Saved in Pi's auth.json".to_string()
                },
            ),
            Credential::OAuth => (
                "Signed in with Pi".to_string(),
                "OAuth credential in Pi's auth.json".to_string(),
            ),
            Credential::Environment(var) => (format!("${var}"), "Read from the environment".into()),
            Credential::Missing => (String::new(), String::new()),
        };
        v_flex()
            .gap_1()
            .child(
                div()
                    .font_family(MONO)
                    .text_size(px(12.5))
                    .text_color(p.subtext1)
                    .child(detail),
            )
            .child(if confirm_sign_out {
                status(
                    None,
                    "Sign out? Signing in again needs pi in a terminal.".into(),
                )
            } else {
                status(Some(p.green), line)
            })
    };

    let actions = h_flex().flex_none().gap_2();
    let actions = if open {
        actions
            .child(
                Button::new(("save-key", ix))
                    .small()
                    .label("Save")
                    .when(!typed.is_empty(), |this| this.primary())
                    .disabled(typed.is_empty())
                    .on_click({
                        let row = row.clone();
                        move |_, window, cx| row.update(cx, |r, cx| r.save(id, window, cx))
                    }),
            )
            .when(editing, |this| {
                this.child(
                    Button::new(("cancel-key", ix))
                        .ghost()
                        .small()
                        .label("Cancel")
                        .on_click({
                            let row = row.clone();
                            move |_, window, cx| row.update(cx, |r, cx| r.cancel(window, cx))
                        }),
                )
            })
    } else {
        match &credential {
            Credential::Stored(_) => actions
                .child(
                    Button::new(("replace-key", ix))
                        .small()
                        .label("Replace")
                        .on_click({
                            let row = row.clone();
                            move |_, window, cx| {
                                row.update(cx, |r, cx| r.edit("Paste the new key", window, cx))
                            }
                        }),
                )
                .child(
                    Button::new(("remove-key", ix))
                        .ghost()
                        .small()
                        .label("Remove")
                        .on_click(move |_, _, cx| {
                            let _ = auth::remove(id);
                            cx.refresh_windows();
                        }),
                ),
            Credential::Environment(_) => actions.child(
                Button::new(("add-key", ix))
                    .small()
                    .label("Add key")
                    .on_click({
                        let row = row.clone();
                        move |_, window, cx| {
                            row.update(cx, |r, cx| r.edit(new_key_placeholder(id), window, cx))
                        }
                    }),
            ),
            // Signing in again needs pi in a terminal, so signing out asks.
            Credential::OAuth if confirm_sign_out => actions
                .child(
                    Button::new(("sign-out", ix))
                        .small()
                        .danger()
                        .label("Sign out")
                        .on_click({
                            let row = row.clone();
                            move |_, _, cx| {
                                let _ = auth::remove(id);
                                row.update(cx, |r, _| r.confirm_sign_out = false);
                                cx.refresh_windows();
                            }
                        }),
                )
                .child(
                    Button::new(("keep-sign-in", ix))
                        .ghost()
                        .small()
                        .label("Cancel")
                        .on_click({
                            let row = row.clone();
                            move |_, _, cx| {
                                row.update(cx, |r, cx| {
                                    r.confirm_sign_out = false;
                                    cx.notify();
                                })
                            }
                        }),
                ),
            Credential::OAuth => actions.child(
                Button::new(("sign-out", ix))
                    .ghost()
                    .small()
                    .label("Sign out")
                    .tooltip("Signing in again needs pi in a terminal")
                    .on_click({
                        let row = row.clone();
                        move |_, _, cx| {
                            row.update(cx, |r, cx| {
                                r.confirm_sign_out = true;
                                cx.notify();
                            })
                        }
                    }),
            ),
            Credential::Missing => actions,
        }
    };

    h_flex()
        .w_full()
        .gap_4()
        .px_4()
        .py(px(14.))
        .when(ix > 0, |this| this.border_t_1().border_color(p.surface0))
        .child(
            v_flex()
                .w(px(150.))
                .flex_none()
                .child(div().font_medium().text_size(px(13.5)).child(name))
                .child(div().text_size(px(12.)).text_color(p.subtext0).child(state)),
        )
        .child(div().flex_1().min_w_0().child(middle))
        .child(actions)
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
    page("Agent defaults")
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
                    .description("provider/model for new chats, e.g. anthropic/claude-opus-4-5. Empty uses Pi's default.")
                    // The value can be long, so the input gets the whole row.
                    .layout(Axis::Vertical),
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
                    .description("Cheaper models are often enough. Empty means the same as the chat.")
                    // The value can be long, so the input gets the whole row.
                    .layout(Axis::Vertical),
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
                    .description("The pi executable. Install it with npm install -g @earendil-works/pi-coding-agent.")
                    // The value can be long, so the input gets the whole row.
                    .layout(Axis::Vertical),
                ),
        )
        .group(SettingGroup::new().title("Tools").description("What Pi may use").items(tools.collect::<Vec<_>>()))
}

fn approvals_page() -> SettingPage {
    page("Approvals")
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
    page("Worktrees").icon(Lucide::GitBranch).group(
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
                .description("Where new worktrees go. {project} and {branch} are filled in.")
                .layout(Axis::Vertical),
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
    page("Appearance").icon(Lucide::Palette).group(
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
            ))
            .item(SettingItem::new(
                "Collapse finished steps",
                SettingField::switch(
                    |cx| Config::global(cx).collapse_steps,
                    |v, cx| Config::update(cx, |c| c.collapse_steps = v),
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
    page("Shortcuts").icon(Lucide::Keyboard).group(
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
