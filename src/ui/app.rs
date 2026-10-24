use gpui_kit::*;
use crate::workspace::Workspace;

pub struct TreehouseApp {
    workspace: Entity<Workspace>,
}

impl TreehouseApp {
    pub fn new(_window: &mut Window, cx: &mut Context<Self>) -> Self {
        Self { workspace: cx.new(Workspace::new) }
    }
}

impl Render for TreehouseApp {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        let _ = &self.workspace;
        div().size_full()
    }
}
