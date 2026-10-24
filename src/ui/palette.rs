//! The command palette (⌘K), built on gpui-kit's `Command` in a dialog.

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::command::{Command, CommandGroup, CommandItem, CommandState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::{ActiveTheme as _, Icon, IconName, WindowExt as _, h_flex};
use gpui_kit::*;
use std::rc::Rc;

use super::icon;
use crate::actions::*;
use crate::theme::{palette, toggle_label};

struct Entry {
    label: SharedString,
    icon: Icon,
    action: Box<dyn Action>,
}

fn entry(label: impl Into<SharedString>, icon_name: impl Into<Icon>, action: impl Action) -> Entry {
    Entry {
        label: label.into(),
        icon: icon(icon_name),
        action: Box::new(action),
    }
}

fn groups(cx: &App) -> Vec<(&'static str, Vec<Entry>)> {
    vec![
        (
            "Workspace",
            vec![
                entry("Open project…", IconName::FolderOpen, OpenProject),
                entry("New worktree", Lucide::GitBranch, NewWorktree),
                entry("Switch worktree…", IconName::ChevronsUpDown, SwitchWorktree),
            ],
        ),
        (
            "View",
            vec![
                entry("Show diff", Lucide::PanelRight, ShowDiff),
                entry("Open right dock", Lucide::Columns2, ToggleDock),
                entry("Full-screen right dock", Lucide::Maximize2, FullScreenDock),
                entry("Toggle sidebar", IconName::PanelLeft, ToggleSidebar),
                entry(
                    toggle_label(cx),
                    if cx.theme().is_dark() {
                        IconName::Sun
                    } else {
                        IconName::Moon
                    },
                    ToggleTheme,
                ),
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
            vec![entry("Settings", IconName::Settings, OpenSettings)],
        ),
    ]
}

/// Opens the palette over the window.
pub fn open(state: &Entity<CommandState>, window: &mut Window, cx: &mut App) {
    let state = state.clone();
    window.open_dialog(cx, move |dialog, _, _| {
        let state = state.clone();
        dialog
            .close_button(false)
            .w(px(560.))
            .p_0()
            .margin_top(px(96.))
            .content(move |content, window, cx| content.child(build(&state, window, cx)))
    });
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
            group = group.item(CommandItem::new().label(entry.label).child(move |_, _| {
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
