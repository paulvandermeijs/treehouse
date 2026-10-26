//! Application actions and their key bindings.

use gpui_kit::base::actions::{SelectDown, SelectUp};
use gpui_kit::{App, KeyBinding, Menu, MenuItem, OsAction, SystemMenuType, actions};

actions!(
    treehouse,
    [
        Quit,
        About,
        Hide,
        HideOthers,
        ShowAll,
        CommandPalette,
        OpenSettings,
        OpenProject,
        NewWorktree,
        SwitchWorktree,
        ShowDiff,
        ToggleDock,
        FullScreenDock,
        ToggleSidebar,
        ToggleFullScreen,
        ToggleTheme,
        ChangeModel,
        SetThinkingLevel,
        StopAgent,
        CommitWithPi,
        SyncWithBase,
        FinishBranch,
        SlashUp,
        SlashDown,
        SlashAccept,
        SlashDismiss,
        OpenInDock,
        ScrollPageUp,
        ScrollPageDown,
        ScrollHalfPageUp,
        ScrollHalfPageDown,
        ScrollToEnd,
    ]
);

/// Shortcuts as the design lists them, with Ctrl in place of ⌘ off macOS.
pub const SHORTCUTS: [(&str, &str); 9] = [
    ("Command palette", "secondary-k"),
    ("Settings", "secondary-,"),
    ("Open project", "secondary-o"),
    ("New worktree", "secondary-n"),
    ("Show diff", "secondary-d"),
    ("Toggle sidebar", "secondary-b"),
    ("Full-screen right dock", "secondary-shift-f"),
    ("Stop agent", "secondary-."),
    ("Go to worktree or sub-agent", "secondary-p"),
];

/// Jump to the end of the chat.
const END: &str = if cfg!(target_os = "macos") {
    "cmd-down"
} else {
    "ctrl-end"
};

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("cmd-h", Hide, None),
        KeyBinding::new("alt-cmd-h", HideOthers, None),
        KeyBinding::new("secondary-k", CommandPalette, None),
        KeyBinding::new("secondary-,", OpenSettings, None),
        KeyBinding::new("secondary-o", OpenProject, None),
        KeyBinding::new("secondary-n", NewWorktree, None),
        KeyBinding::new("secondary-p", SwitchWorktree, None),
        KeyBinding::new("secondary-d", ShowDiff, None),
        KeyBinding::new("secondary-\\", ToggleDock, None),
        KeyBinding::new("secondary-shift-f", FullScreenDock, None),
        KeyBinding::new("secondary-b", ToggleSidebar, None),
        // GPUI builds the menu bar itself, so macOS doesn't add its own
        // Enter Full Screen item or shortcut.
        KeyBinding::new(
            if cfg!(target_os = "macos") {
                "ctrl-cmd-f"
            } else {
                "f11"
            },
            ToggleFullScreen,
            None,
        ),
        KeyBinding::new("secondary-m", ChangeModel, None),
        KeyBinding::new("secondary-.", StopAgent, None),
        // While the slash menu is open these beat the message box's own
        // bindings: they are more specific and added after the kit's.
        KeyBinding::new("up", SlashUp, Some("SlashMenu > Input")),
        KeyBinding::new("down", SlashDown, Some("SlashMenu > Input")),
        KeyBinding::new("tab", SlashAccept, Some("SlashMenu > Input")),
        KeyBinding::new("enter", SlashAccept, Some("SlashMenu > Input")),
        KeyBinding::new("escape", SlashDismiss, Some("SlashMenu > Input")),
        // Ctrl-N and Ctrl-P move through lists, as in a terminal or Emacs.
        // On Linux and Windows they beat New worktree and the switcher while
        // a list has focus.
        KeyBinding::new("ctrl-p", SlashUp, Some("SlashMenu > Input")),
        KeyBinding::new("ctrl-n", SlashDown, Some("SlashMenu > Input")),
        KeyBinding::new("ctrl-p", SelectUp, Some("Command > Input")),
        KeyBinding::new("ctrl-n", SelectDown, Some("Command > Input")),
        KeyBinding::new("ctrl-p", SelectUp, Some("Command")),
        KeyBinding::new("ctrl-n", SelectDown, Some("Command")),
        // The chat scrolls from the keyboard while its message box has focus:
        // the box's own Page Up/Down and Ctrl-D give way.
        KeyBinding::new("pageup", ScrollPageUp, Some("ChatView > Input")),
        KeyBinding::new("pagedown", ScrollPageDown, Some("ChatView > Input")),
        KeyBinding::new("ctrl-u", ScrollHalfPageUp, Some("ChatView > Input")),
        KeyBinding::new("ctrl-d", ScrollHalfPageDown, Some("ChatView > Input")),
        KeyBinding::new(END, ScrollToEnd, Some("ChatView > Input")),
        KeyBinding::new(END, ScrollToEnd, Some("ChatView")),
        KeyBinding::new("pageup", ScrollPageUp, Some("ChatView")),
        KeyBinding::new("pagedown", ScrollPageDown, Some("ChatView")),
        KeyBinding::new("ctrl-u", ScrollHalfPageUp, Some("ChatView")),
        KeyBinding::new("ctrl-d", ScrollHalfPageDown, Some("ChatView")),
        // ⌘⏎ in the switcher opens the pick in the right dock, as ⌘-click
        // does in the sidebar. More specific than the search field's own.
        KeyBinding::new("secondary-enter", OpenInDock, Some("Command > Input")),
        KeyBinding::new("secondary-enter", OpenInDock, Some("Command")),
    ]);
    cx.on_action(|_: &Quit, cx| cx.quit());
    cx.on_action(|_: &About, cx| crate::ui::open_about(cx));
    cx.on_action(|_: &Hide, cx| cx.hide());
    cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
    cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
    cx.set_menus(menus());
}

/// The menu bar. macOS shows it; other platforms reach the same actions
/// through shortcuts and the command palette.
fn menus() -> Vec<Menu> {
    use gpui_kit::component::input::{Copy, Cut, Paste, Redo, SelectAll, Undo};
    vec![
        Menu::new("Treehouse").items([
            MenuItem::action("About Treehouse", About),
            MenuItem::separator(),
            MenuItem::action("Settings…", OpenSettings),
            MenuItem::separator(),
            MenuItem::os_submenu("Services", SystemMenuType::Services),
            MenuItem::separator(),
            MenuItem::action("Hide Treehouse", Hide),
            MenuItem::action("Hide Others", HideOthers),
            MenuItem::action("Show All", ShowAll),
            MenuItem::separator(),
            MenuItem::action("Quit Treehouse", Quit),
        ]),
        Menu::new("File").items([
            MenuItem::action("Open Project…", OpenProject),
            MenuItem::action("New Worktree", NewWorktree),
            MenuItem::action("Go to Worktree or Sub-agent…", SwitchWorktree),
        ]),
        Menu::new("Edit").items([
            MenuItem::os_action("Undo", Undo, OsAction::Undo),
            MenuItem::os_action("Redo", Redo, OsAction::Redo),
            MenuItem::separator(),
            MenuItem::os_action("Cut", Cut, OsAction::Cut),
            MenuItem::os_action("Copy", Copy, OsAction::Copy),
            MenuItem::os_action("Paste", Paste, OsAction::Paste),
            MenuItem::os_action("Select All", SelectAll, OsAction::SelectAll),
        ]),
        Menu::new("View").items([
            MenuItem::action("Command Palette", CommandPalette),
            MenuItem::separator(),
            MenuItem::action("Show Diff", ShowDiff),
            MenuItem::action("Full-Screen Right Dock", FullScreenDock),
            MenuItem::action("Toggle Sidebar", ToggleSidebar),
            MenuItem::separator(),
            MenuItem::action("Toggle Full Screen", ToggleFullScreen),
            MenuItem::separator(),
            MenuItem::action("Toggle Theme", ToggleTheme),
        ]),
    ]
}
