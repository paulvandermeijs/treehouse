//! The Treehouse window.

mod about;
mod app;
mod chat;
mod code_block;
mod diagram;
mod diff;
mod items;
mod model_picker;
mod palette;
mod settings;
mod sidebar;
mod state;
mod status_card;
mod trust;

pub use about::open as open_about;
pub use app::TreehouseApp;

use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::{App, IntoElement, ParentElement as _, Pixels, Styled as _, div, px};

use crate::session::AgentStatus;
use crate::theme::palette;

/// Heights shared by every top bar and the composer toolbar.
pub const BAR_HEIGHT: f32 = 40.;
pub const SIDEBAR_WIDTH: f32 = 272.;

/// The small status indicator used in the sidebar and on cards.
pub fn status_dot(status: AgentStatus, cx: &App) -> gpui_kit::AnyElement {
    let p = palette(cx);
    let dot = div().flex_none().size(px(7.)).rounded_full();
    match status {
        AgentStatus::Working => div()
            .flex_none()
            .size(px(10.))
            .mx(px(-1.5))
            .child(Spinner::new().with_size(px(10.)).color(p.subtext1))
            .into_any_element(),
        AgentStatus::NeedsInput => dot.bg(p.blue).into_any_element(),
        AgentStatus::Finished => dot.bg(p.green).into_any_element(),
        AgentStatus::Failed => dot.bg(p.red).into_any_element(),
        AgentStatus::Idle => dot
            .border(px(1.5))
            .border_color(p.subtext0)
            .into_any_element(),
    }
}

/// Room for macOS's traffic lights at the left of the first top bar. The
/// windows hide the system title bar so the app's own bars run to the top.
/// In full screen macOS hides them, and the bar keeps its usual padding.
pub fn traffic_lights(window: &gpui_kit::Window) -> f32 {
    if cfg!(target_os = "macos") && !window.is_fullscreen() {
        68.
    } else {
        0.
    }
}

/// The collapsed sidebar's width.
pub const RAIL_WIDTH: f32 = 48.;

/// With the sidebar collapsed on macOS, the traffic lights don't fit in the
/// rail: the pane next to it starts its top bar with the sidebar toggle,
/// after this much padding, so the toggle sits where the sidebar has it.
pub fn rail_lead(window: &gpui_kit::Window) -> Option<Pixels> {
    let lights = traffic_lights(window);
    (lights > 0.).then(|| px(8. + lights - RAIL_WIDTH))
}

/// The top bar's show/hide sidebar button.
pub fn sidebar_toggle(id: &'static str) -> gpui_kit::component::button::Button {
    use gpui_kit::component::IconName;
    use gpui_kit::component::button::{Button, ButtonVariants as _};
    Button::new(id)
        .ghost()
        .small()
        .icon(icon(IconName::PanelLeft))
        .tooltip_with_action("Toggle sidebar", &crate::actions::ToggleSidebar, None)
        .on_click(|_, window, cx| {
            window.dispatch_action(Box::new(crate::actions::ToggleSidebar), cx)
        })
}

/// The title bar on macOS: hidden, with the traffic lights centred in the
/// first 40px bar.
pub fn titlebar() -> gpui_kit::TitlebarOptions {
    gpui_kit::TitlebarOptions {
        title: None,
        // Windows would lose its caption buttons; Linux ignores it.
        appears_transparent: cfg!(target_os = "macos"),
        traffic_light_position: Some(gpui_kit::point(px(14.), px(13.))),
    }
}

/// Makes a top bar move the window like a title bar: drag to move, double
/// click to zoom. Presses that a button inside already handled are left
/// alone. Only macOS needs this; elsewhere the system draws the title bar.
pub fn drags_window<E: gpui_kit::InteractiveElement>(element: E) -> E {
    if !cfg!(target_os = "macos") {
        return element;
    }
    element.on_mouse_down(gpui_kit::MouseButton::Left, |event, window, _| {
        if window.default_prevented() {
            return;
        }
        if event.click_count == 2 {
            window.titlebar_double_click();
        } else {
            window.start_window_move();
        }
    })
}

/// A 16px stroke icon, the design's default icon size.
pub fn icon(name: impl Into<Icon>) -> Icon {
    name.into().size(px(16.))
}
