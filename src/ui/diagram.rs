//! A Mermaid diagram in the right dock, where it can be zoomed and panned.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::assets::IconName as Lucide;
use gpui_kit::component::button::{Button, ButtonVariants as _};
use gpui_kit::component::{Sizable as _, StyledExt as _, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::{BAR_HEIGHT, icon};
use crate::actions::FullScreenDock;
use crate::theme::palette;

const MIN_ZOOM: f32 = 0.1;
const MAX_ZOOM: f32 = 8.;
/// Room around a diagram fitted to the panel.
const FIT_MARGIN: f32 = 24.;

pub enum DiagramEvent {
    Close,
}

pub struct DiagramPanel {
    source: SharedString,
    zoom: f32,
    /// Where the diagram's top left sits in the panel's body.
    offset: Point<Pixels>,
    /// Fitted to the body, until zoomed or moved by hand.
    fit: bool,
    /// The last mouse position of a drag.
    drag: Option<Point<Pixels>>,
    /// The body's bounds when last painted, for fitting and zooming at the
    /// pointer.
    body: Rc<Cell<Bounds<Pixels>>>,
    pub full_screen: bool,
    /// See `ChatView::sidebar_lead`.
    pub sidebar_lead: Option<Pixels>,
}

impl EventEmitter<DiagramEvent> for DiagramPanel {}

impl DiagramPanel {
    pub fn new(source: SharedString) -> Self {
        Self {
            source,
            zoom: 1.,
            offset: Point::default(),
            fit: true,
            drag: None,
            body: Rc::new(Cell::new(Bounds::default())),
            full_screen: false,
            sidebar_lead: None,
        }
    }

    pub fn set_full_screen(&mut self, full_screen: bool, cx: &mut Context<Self>) {
        self.full_screen = full_screen;
        // The body changes size; fit the diagram again if it was fitted.
        cx.notify();
    }

    /// Zooms by `factor`, keeping the point `at` (in the window) in place.
    fn zoom_by(&mut self, factor: f32, at: Option<Point<Pixels>>, cx: &mut Context<Self>) {
        let body = self.body.get();
        let new = (self.zoom * factor).clamp(MIN_ZOOM, MAX_ZOOM);
        let at = at.map_or(body.size.center(), |at| at - body.origin);
        let ratio = new / self.zoom;
        self.offset = at - (at - self.offset) * ratio;
        self.zoom = new;
        self.fit = false;
        cx.notify();
    }

    fn fit_to(&mut self, width: f32, height: f32) {
        let body = self.body.get().size;
        if body.width <= px(0.) || width <= 0. || height <= 0. {
            return;
        }
        let room_w = f32::from(body.width) - FIT_MARGIN * 2.;
        let room_h = f32::from(body.height) - FIT_MARGIN * 2.;
        self.zoom = (room_w / width).min(room_h / height).clamp(MIN_ZOOM, 2.);
        self.offset = point(
            (body.width - px(width * self.zoom)) / 2.,
            (body.height - px(height * self.zoom)) / 2.,
        );
    }
}

impl Render for DiagramPanel {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        // Drawn at twice its size once, so it stays sharp up to 200% and
        // zooming never swaps the image, which shows a frame late.
        let rendered = crate::mermaid::render(&self.source, &p, p.base, window.scale_factor() * 2.);
        if let (true, Ok(diagram)) = (self.fit, &rendered) {
            self.fit_to(diagram.width, diagram.height);
        }
        let kind = crate::mermaid::kind(&self.source).map(|k| SharedString::from(k.to_string()));
        let copied = self.source.clone();

        // The body's size is known once painted; a change asks for another
        // frame, so the diagram is fitted to the new size.
        let body = self.body.clone();
        let measure = canvas(
            move |bounds, window, _| {
                if body.replace(bounds) != bounds {
                    window.on_next_frame(|window, _| window.refresh());
                }
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();

        let zoom_button = |id: &'static str, glyph: Lucide, tip: &'static str, factor: f32| {
            Button::new(id)
                .ghost()
                .small()
                .icon(icon(glyph))
                .tooltip(tip)
                .on_click(cx.listener(move |this, _, _, cx| this.zoom_by(factor, None, cx)))
        };

        v_flex()
            .size_full()
            .min_w_0()
            .bg(p.base)
            .child(
                super::drags_window(h_flex().id("diagram-top"))
                    .h(px(BAR_HEIGHT))
                    .flex_none()
                    .gap_2()
                    .px_2()
                    .when_some(self.sidebar_lead, |this, lead| {
                        this.pl(lead)
                            .child(super::sidebar_toggle("diagram-sidebar"))
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
                    .child(div().font_medium().child("Diagram"))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .text_color(p.subtext0)
                            .whitespace_nowrap()
                            .overflow_hidden()
                            .text_ellipsis()
                            .children(kind),
                    )
                    .child(zoom_button(
                        "zoom-out",
                        Lucide::ZoomOut,
                        "Zoom out",
                        1. / 1.25,
                    ))
                    .child(
                        Button::new("zoom-fit")
                            .ghost()
                            .small()
                            .label(format!("{:.0}%", self.zoom * 100.))
                            .tooltip("Fit to the panel")
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.fit = true;
                                cx.notify();
                            })),
                    )
                    .child(zoom_button("zoom-in", Lucide::ZoomIn, "Zoom in", 1.25))
                    .child(
                        Button::new("diagram-copy")
                            .ghost()
                            .small()
                            .icon(icon(Lucide::Copy))
                            .tooltip("Copy source")
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(copied.to_string()))
                            }),
                    )
                    .child(
                        Button::new("dock-close")
                            .ghost()
                            .small()
                            .icon(icon(Lucide::X))
                            .tooltip("Close diagram")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(DiagramEvent::Close))),
                    ),
            )
            .child(
                div()
                    .id("diagram-body")
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .overflow_hidden()
                    .when(self.drag.is_some(), |this| this.cursor_grabbing())
                    .when(self.drag.is_none(), |this| this.cursor_grab())
                    .child(measure)
                    // Scrolling pans; with ⌘ or Ctrl, or pinching on a
                    // trackpad's scroll, it zooms at the pointer.
                    .on_scroll_wheel(cx.listener(|this, event: &ScrollWheelEvent, _, cx| {
                        let delta = event.delta.pixel_delta(px(20.));
                        if event.modifiers.platform || event.modifiers.control {
                            // Scrolling up, or spreading on a trackpad, zooms in.
                            let factor = (f32::from(delta.y) * 0.004).exp();
                            this.zoom_by(factor, Some(event.position), cx);
                        } else {
                            this.offset += delta;
                            this.fit = false;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            this.drag = Some(event.position);
                            cx.notify();
                        }),
                    )
                    .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                        let Some(last) = this.drag else { return };
                        if event.pressed_button != Some(MouseButton::Left) {
                            this.drag = None;
                        } else {
                            this.offset += event.position - last;
                            this.drag = Some(event.position);
                            this.fit = false;
                        }
                        cx.notify();
                    }))
                    .on_mouse_up(
                        MouseButton::Left,
                        cx.listener(|this, _: &MouseUpEvent, _, cx| {
                            this.drag = None;
                            cx.notify();
                        }),
                    )
                    .map(|this| match rendered {
                        Ok(diagram) => this.child(
                            img(diagram.image.clone())
                                .absolute()
                                .left(self.offset.x)
                                .top(self.offset.y)
                                .w(px(diagram.width * self.zoom))
                                .h(px(diagram.height * self.zoom)),
                        ),
                        Err(error) => this.child(
                            div()
                                .p_4()
                                .text_size(px(13.))
                                .text_color(p.red)
                                .child(format!("Couldn't render: {error}")),
                        ),
                    }),
            )
    }
}
