//! The model picker: a filterable list of Pi's models, opened from the
//! chat's model button and from Settings' model dropdowns.

use std::collections::HashSet;
use std::rc::Rc;

use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::kbd::Kbd;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::component::{IconName, h_flex, v_flex};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;

use super::icon;
use crate::actions::{PickerAccept, PickerDismiss, PickerDown, PickerFavorite, PickerUp};
use crate::config::Config;
use crate::models::{self, ListOptions, ModelInfo, Row, Section};
use crate::theme::{mix, palette};

/// A row's height as drawn, so the list knows its own height without
/// measuring hundreds of rows. Models include the 2px gap under them.
fn row_height(ix: usize, row: &Row) -> Pixels {
    match row {
        Row::First | Row::Model { .. } => px(50.),
        Row::Header { .. } if ix == 0 => px(28.),
        // The 4px above the line is margin, outside this height.
        Row::Header { .. } => px(30.),
        Row::Maker(_) => px(24.),
    }
}

/// What was picked.
#[derive(Clone)]
pub enum Pick {
    /// Settings' first option, which clears the setting.
    First,
    Model(ModelInfo),
}

type Getter<T> = Rc<dyn Fn(&App) -> T>;
type OnPick = Rc<dyn Fn(Pick, &mut Window, &mut App)>;

pub struct ModelPicker {
    filter: Entity<InputState>,
    models: Getter<Vec<ModelInfo>>,
    /// `provider/id` of the current model; `None` when the first option is.
    current: Getter<Option<String>>,
    /// A message for an empty list while there are no models yet.
    waiting: Getter<Option<String>>,
    /// Settings' first option: its label and the line under it.
    first_option: Option<(SharedString, SharedString)>,
    on_pick: OnPick,
    toggled: HashSet<String>,
    /// The highlighted row, by index into `rows`.
    selected: Option<usize>,
    /// Opened, but the current model isn't highlighted yet.
    select_current: bool,
    /// Only the rows in view are drawn: OpenRouter alone lists hundreds.
    list: ListState,
    /// The rows the list was last drawn with.
    rows: Rc<Vec<Row>>,
    /// The star shows on the selected and the hovered row.
    hovered: Option<usize>,
    max_height: Pixels,
    _subscription: Subscription,
}

impl EventEmitter<DismissEvent> for ModelPicker {}

impl Focusable for ModelPicker {
    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.filter.focus_handle(cx)
    }
}

impl ModelPicker {
    pub fn new(
        models: impl Fn(&App) -> Vec<ModelInfo> + 'static,
        current: impl Fn(&App) -> Option<String> + 'static,
        waiting: impl Fn(&App) -> Option<String> + 'static,
        on_pick: impl Fn(Pick, &mut Window, &mut App) + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter models"));
        let subscription = cx.subscribe(&filter, |this, filter, event: &InputEvent, cx| {
            if matches!(event, InputEvent::Change) {
                // Opening clears the filter; that keeps the current model.
                if this.select_current && filter.read(cx).value().is_empty() {
                    return;
                }
                this.select_current = false;
                this.select_first_match(cx);
                cx.notify();
            }
        });
        Self {
            filter,
            models: Rc::new(models),
            current: Rc::new(current),
            waiting: Rc::new(waiting),
            first_option: None,
            on_pick: Rc::new(on_pick),
            toggled: HashSet::new(),
            selected: None,
            select_current: false,
            list: ListState::new(0, ListAlignment::Top, px(200.)),
            rows: Rc::new(Vec::new()),
            hovered: None,
            max_height: px(440.),
            _subscription: subscription,
        }
    }

    pub fn first_option(mut self, label: &str, detail: &str) -> Self {
        self.first_option = Some((label.to_string().into(), detail.to_string().into()));
        self
    }

    pub fn max_height(mut self, height: Pixels) -> Self {
        self.max_height = height;
        self
    }

    /// Fresh for each opening: no filter, the current model highlighted.
    pub fn reset(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.filter
            .update(cx, |state, cx| state.set_value("", window, cx));
        self.toggled.clear();
        self.selected = None;
        self.hovered = None;
        self.select_current = true;
        self.rows = Rc::new(Vec::new());
        self.list.reset(0);
        cx.notify();
    }

    /// Highlights the current model, once the list has rows: the models
    /// may arrive after the picker opens.
    fn highlight_current(&mut self, rows: &[Row], cx: &App) {
        if rows.is_empty() {
            return;
        }
        self.select_current = false;
        self.selected = rows
            .iter()
            .position(|row| match row {
                Row::Model { current, .. } => *current,
                Row::First => (self.current)(cx).is_none(),
                _ => false,
            })
            .or_else(|| rows.iter().position(Row::selectable));
        if let Some(ix) = self.selected {
            self.list.scroll_to_reveal_item(ix);
        }
    }

    fn rows(&self, cx: &App) -> Vec<Row> {
        let config = Config::global(cx);
        let models = (self.models)(cx);
        let current = (self.current)(cx);
        models::rows(&ListOptions {
            models: &models,
            favorites: &config.favorite_models,
            recent: &config.recent_models,
            current: current.as_deref(),
            query: &self.filter.read(cx).value(),
            toggled: &self.toggled,
            first_option: self.first_option.is_some(),
        })
    }

    fn select_first_match(&mut self, cx: &App) {
        let rows = self.rows(cx);
        self.selected = rows
            .iter()
            .position(|row| matches!(row, Row::Model { .. }))
            .or_else(|| rows.iter().position(Row::selectable));
        self.list.scroll_to_reveal_item(0);
    }

    fn move_selection(&mut self, delta: isize, cx: &mut Context<Self>) {
        let rows = self.rows(cx);
        let selectable: Vec<usize> = (0..rows.len()).filter(|&i| rows[i].selectable()).collect();
        if selectable.is_empty() {
            return;
        }
        let at = self
            .selected
            .and_then(|s| selectable.iter().position(|&i| i == s));
        let next = match at {
            Some(at) => (at as isize + delta).rem_euclid(selectable.len() as isize) as usize,
            None if delta < 0 => selectable.len() - 1,
            None => 0,
        };
        self.selected = Some(selectable[next]);
        self.list.scroll_to_reveal_item(selectable[next]);
        cx.notify();
    }

    fn accept(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let rows = self.rows(cx);
        match rows.get(ix) {
            Some(Row::First) => self.pick(Pick::First, window, cx),
            Some(Row::Model { model, .. }) => self.pick(Pick::Model(model.clone()), window, cx),
            Some(Row::Header {
                section: Section::Provider(provider),
                open: Some(_),
                ..
            }) => {
                let provider = provider.clone();
                if !self.toggled.remove(&provider) {
                    self.toggled.insert(provider);
                }
                self.selected = Some(ix);
                cx.notify();
            }
            _ => {}
        }
    }

    fn pick(&mut self, pick: Pick, window: &mut Window, cx: &mut Context<Self>) {
        cx.emit(DismissEvent);
        (self.on_pick)(pick, window, cx);
    }

    fn toggle_favorite(&mut self, ix: usize, cx: &mut Context<Self>) {
        if let Some(Row::Model { model, .. }) = self.rows(cx).get(ix) {
            let full = model.full_name();
            Config::update(cx, |c| {
                if c.favorite_models.contains(&full) {
                    c.favorite_models.retain(|m| *m != full);
                } else {
                    c.favorite_models.push(full);
                }
            });
        }
        cx.notify();
    }

    /// Esc clears the filter first, then closes.
    fn dismiss(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.filter.read(cx).value().is_empty() {
            cx.emit(DismissEvent);
        } else {
            self.filter
                .update(cx, |state, cx| state.set_value("", window, cx));
            self.select_first_match(cx);
            cx.notify();
        }
    }

    fn render_row(&self, ix: usize, row: &Row, cx: &mut Context<Self>) -> AnyElement {
        let p = palette(cx);
        let selected = self.selected == Some(ix);
        let hover = mix(p.surface0, p.mantle, 0.55);
        match row {
            Row::First => {
                let (label, detail) = self.first_option.clone().unwrap_or_default();
                let current = (self.current)(cx).is_none();
                self.model_shell(ix, selected, cx)
                    .child(
                        v_flex().flex_1().min_w_0().child(label).child(
                            div()
                                .text_size(px(11.5))
                                .text_color(p.subtext0)
                                .child(detail),
                        ),
                    )
                    .when(current, |this| {
                        this.child(icon(IconName::Check).size(px(14.)).text_color(p.blue))
                    })
                    .into_any_element()
            }
            Row::Header {
                label, count, open, ..
            } => h_flex()
                .id(("model-group", ix))
                .w_full()
                .h(row_height(ix, row))
                .items_end()
                .gap(px(6.))
                .px(px(14.))
                .pb(px(2.))
                .when(ix > 0, |this| {
                    this.mt(px(4.)).border_t_1().border_color(p.surface0)
                })
                .text_size(px(11.5))
                .text_color(p.subtext0)
                .font_weight(FontWeight::MEDIUM)
                .when(selected, |this| this.bg(p.surface0))
                .child(label.clone())
                .when_some(*count, |this, count| {
                    this.child(
                        div()
                            .font_weight(FontWeight::NORMAL)
                            .child(format!("· {count}")),
                    )
                })
                .when_some(*open, |this, open| {
                    this.cursor_pointer()
                        .child(div().flex_1())
                        .child(
                            h_flex()
                                .size(px(22.))
                                .justify_center()
                                .rounded(px(4.))
                                .hover(|s| s.bg(p.surface0).text_color(p.text))
                                .child(
                                    icon(if open {
                                        IconName::ChevronDown
                                    } else {
                                        IconName::ChevronRight
                                    })
                                    .size(px(14.)),
                                ),
                        )
                        .on_click(
                            cx.listener(move |this, _, window, cx| this.accept(ix, window, cx)),
                        )
                })
                .into_any_element(),
            Row::Maker(maker) => h_flex()
                .w_full()
                .h(row_height(ix, row))
                .items_end()
                .px(px(14.))
                .pb(px(2.))
                .text_size(px(11.))
                .text_color(p.subtext0)
                .child(maker.clone())
                .into_any_element(),
            Row::Model {
                model,
                label,
                label_marks,
                id_marks,
                favorite,
                current,
            } => {
                let marked = HighlightStyle {
                    background_color: Some(p.blue.opacity(0.3)),
                    ..Default::default()
                };
                let marks = |ranges: &[std::ops::Range<usize>]| {
                    ranges
                        .iter()
                        .map(|r| (r.clone(), marked))
                        .collect::<Vec<_>>()
                };
                let favorite = *favorite;
                let starred = selected || self.hovered == Some(ix);
                self.model_shell(ix, selected, cx)
                    .when(!selected, |this| this.hover(|s| s.bg(hover)))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered {
                            this.hovered = Some(ix);
                        } else if this.hovered == Some(ix) {
                            this.hovered = None;
                        }
                        cx.notify();
                    }))
                    .child(
                        v_flex()
                            .flex_1()
                            .min_w_0()
                            .gap(px(1.))
                            .child(
                                h_flex()
                                    .gap(px(6.))
                                    .min_w_0()
                                    .child(
                                        div()
                                            .min_w_0()
                                            .overflow_hidden()
                                            .whitespace_nowrap()
                                            .child(
                                                StyledText::new(label.clone())
                                                    .with_highlights(marks(label_marks)),
                                            ),
                                    )
                                    .when(model.is_latest(), |this| {
                                        this.child(
                                            div()
                                                .flex_none()
                                                .h(px(18.))
                                                .px(px(6.))
                                                .rounded(px(9.))
                                                .bg(p.surface0)
                                                .text_size(px(11.))
                                                .text_color(p.subtext1)
                                                .child("latest"),
                                        )
                                    }),
                            )
                            .child(
                                div()
                                    .overflow_hidden()
                                    .whitespace_nowrap()
                                    .text_ellipsis()
                                    .font_family(crate::theme::mono())
                                    .text_size(px(11.5))
                                    .text_color(p.subtext0)
                                    .child(
                                        StyledText::new(model.full_name())
                                            .with_highlights(marks(id_marks)),
                                    ),
                            ),
                    )
                    .child(
                        h_flex()
                            .flex_none()
                            .gap(px(8.))
                            .when(*current, |this| {
                                this.child(icon(IconName::Check).size(px(14.)).text_color(p.blue))
                            })
                            .when(starred, |this| {
                                this.child(
                                    h_flex()
                                        .id(("model-star", ix))
                                        .size(px(20.))
                                        .justify_center()
                                        .cursor_pointer()
                                        .child(
                                            icon(if favorite {
                                                IconName::StarFill
                                            } else {
                                                IconName::Star
                                            })
                                            .size(px(14.))
                                            .text_color(if favorite { p.blue } else { p.subtext0 }),
                                        )
                                        .tooltip(move |window, cx| {
                                            Tooltip::new(if favorite {
                                                "Remove from favorites"
                                            } else {
                                                "Add to favorites"
                                            })
                                            .build(window, cx)
                                        })
                                        .on_click(cx.listener(move |this, _, _, cx| {
                                            cx.stop_propagation();
                                            this.toggle_favorite(ix, cx)
                                        })),
                                )
                            }),
                    )
                    .into_any_element()
            }
        }
    }

    /// A clickable row of the list: a model or the first option.
    fn model_shell(&self, ix: usize, selected: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = palette(cx);
        h_flex()
            .id(("model", ix))
            .flex_none()
            .flex_1()
            .mb(px(2.))
            .h(px(48.))
            .gap(px(8.))
            .px(px(8.))
            .py(px(7.))
            .rounded(px(6.))
            .cursor_pointer()
            .when(selected, |this| this.bg(p.surface0))
            .on_click(cx.listener(move |this, _, window, cx| this.accept(ix, window, cx)))
    }
}

impl Render for ModelPicker {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let p = palette(cx);
        let rows = self.rows(cx);
        if *self.rows != rows {
            // Splicing keeps the scroll position, so starring a row
            // deep in the list doesn't jump to the top.
            self.list.splice(0..self.rows.len(), rows.len());
            self.rows = Rc::new(rows.clone());
        }
        if self.select_current {
            self.highlight_current(&rows, cx);
        }
        if self.selected.is_some_and(|s| s >= rows.len()) {
            self.selected = rows.iter().position(Row::selectable);
        }
        let query = self.filter.read(cx).value();
        let waiting = (self.waiting)(cx);
        let open_settings = |_: &ClickEvent, window: &mut Window, cx: &mut App| {
            super::settings::open(super::settings::Section::Providers, window, cx)
        };
        v_flex()
            .key_context("ModelPicker")
            .on_action(cx.listener(|this, _: &PickerUp, _, cx| this.move_selection(-1, cx)))
            .on_action(cx.listener(|this, _: &PickerDown, _, cx| this.move_selection(1, cx)))
            .on_action(cx.listener(|this, _: &PickerAccept, window, cx| {
                if let Some(ix) = this.selected {
                    this.accept(ix, window, cx)
                }
            }))
            .on_action(cx.listener(|this, _: &PickerDismiss, window, cx| this.dismiss(window, cx)))
            .on_action(cx.listener(|this, _: &PickerFavorite, _, cx| {
                if let Some(ix) = this.selected {
                    this.toggle_favorite(ix, cx)
                }
            }))
            .w_full()
            .text_size(px(13.5))
            .text_color(p.text)
            .child(
                h_flex()
                    .gap(px(8.))
                    .px(px(12.))
                    .py(px(4.))
                    .border_b_1()
                    .border_color(p.surface0)
                    .text_color(p.subtext0)
                    .child(icon(IconName::Search).size(px(14.)))
                    .child(
                        div().flex_1().child(
                            Input::new(&self.filter)
                                .appearance(false)
                                .aria_label("Filter models"),
                        ),
                    )
                    .child(Kbd::new(Keystroke::parse("escape").unwrap())),
            )
            .map(|this| {
                if rows.is_empty() {
                    let (title, detail): (SharedString, SharedString) = match waiting {
                        Some(waiting) if query.is_empty() => (waiting.into(), "".into()),
                        _ if query.is_empty() => (
                            "No models".into(),
                            "Models come from the providers you have keys for.".into(),
                        ),
                        _ => (
                            format!("No models match \"{query}\"").into(),
                            "Models come from the providers you have keys for.".into(),
                        ),
                    };
                    this.child(
                        v_flex()
                            .items_center()
                            .gap(px(4.))
                            .px(px(16.))
                            .py(px(28.))
                            .text_size(px(13.))
                            .text_color(p.subtext0)
                            .child(
                                div()
                                    .text_color(p.text)
                                    .font_weight(FontWeight::MEDIUM)
                                    .child(title),
                            )
                            .when(!detail.is_empty(), |this| {
                                this.child(
                                    h_flex().gap(px(4.)).child(detail).child(
                                        div()
                                            .id("model-providers")
                                            .text_color(p.blue)
                                            .cursor_pointer()
                                            .child("Providers…")
                                            .on_click(open_settings),
                                    ),
                                )
                            }),
                    )
                } else {
                    let height = rows
                        .iter()
                        .enumerate()
                        .map(|(ix, row)| row_height(ix, row))
                        .sum::<Pixels>()
                        .min(self.max_height);
                    this.child(
                        div().py(px(6.)).child(
                            list(
                                self.list.clone(),
                                cx.processor(|this, ix: usize, _, cx| {
                                    let rows = this.rows.clone();
                                    // Without a full-width wrapper the list
                                    // lays rows out at their own width.
                                    let row = match rows.get(ix) {
                                        Some(row @ (Row::First | Row::Model { .. })) => {
                                            h_flex().px(px(6.)).child(this.render_row(ix, row, cx))
                                        }
                                        Some(row) => h_flex().child(this.render_row(ix, row, cx)),
                                        None => h_flex(),
                                    };
                                    row.w_full().into_any_element()
                                }),
                            )
                            .h(height),
                        ),
                    )
                }
            })
            .child(
                h_flex()
                    .gap(px(12.))
                    .px(px(12.))
                    .py(px(8.))
                    .border_t_1()
                    .border_color(p.surface0)
                    .text_size(px(12.))
                    .text_color(p.subtext0)
                    .child(format!(
                        "↑↓ choose · ⏎ use · {} favorite",
                        if cfg!(target_os = "macos") {
                            "⌘D"
                        } else {
                            "Ctrl-D"
                        }
                    ))
                    .child(div().flex_1())
                    .child(
                        div()
                            .id("model-footer-providers")
                            .text_color(p.blue)
                            .cursor_pointer()
                            .child("Providers…")
                            .on_click(open_settings),
                    ),
            )
    }
}
