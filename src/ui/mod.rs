//! The Treehouse window.

mod app;
mod chat;
mod code_block;
mod diff;
mod items;
mod palette;
mod settings;
mod sidebar;
mod trust;

pub use app::TreehouseApp;

use gpui_kit::component::spinner::Spinner;
use gpui_kit::component::{Icon, Sizable as _};
use gpui_kit::{App, IntoElement, ParentElement as _, Styled as _, div, px};

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

/// A 16px stroke icon, the design's default icon size.
pub fn icon(name: impl Into<Icon>) -> Icon {
    name.into().size(px(16.))
}
