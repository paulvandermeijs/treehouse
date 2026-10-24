//! Application actions and their key bindings.

use gpui_kit::{App, KeyBinding, actions};

actions!(
    treehouse,
    [
        Quit,
        CommandPalette,
        OpenSettings,
        OpenProject,
        NewWorktree,
        SwitchWorktree,
        ShowDiff,
        ToggleDock,
        FullScreenDock,
        ToggleSidebar,
        ToggleTheme,
        ChangeModel,
        SetThinkingLevel,
        StopAgent,
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
    ("Switch worktree", "secondary-p"),
];

pub fn init(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("secondary-q", Quit, None),
        KeyBinding::new("secondary-k", CommandPalette, None),
        KeyBinding::new("secondary-,", OpenSettings, None),
        KeyBinding::new("secondary-o", OpenProject, None),
        KeyBinding::new("secondary-n", NewWorktree, None),
        KeyBinding::new("secondary-p", SwitchWorktree, None),
        KeyBinding::new("secondary-d", ShowDiff, None),
        KeyBinding::new("secondary-\\", ToggleDock, None),
        KeyBinding::new("secondary-shift-f", FullScreenDock, None),
        KeyBinding::new("secondary-b", ToggleSidebar, None),
        KeyBinding::new("secondary-m", ChangeModel, None),
        KeyBinding::new("secondary-.", StopAgent, None),
    ]);
    cx.on_action(|_: &Quit, cx| cx.quit());
}
