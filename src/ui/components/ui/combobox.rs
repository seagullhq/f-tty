//! Single-choice dropdown whose trigger turns into a text input while open.
//!
//! The owner keeps the selected value and the item list. This type tracks
//! whether the popup is open, the highlighted row, and owns the [`TextInput`]
//! shown in the trigger. Navigation keys (up/down/enter/escape) reach it
//! through [`Combobox::on_key`]: the root view forwards them while open.
//! Typing and editing are handled by the input itself.

use super::{TextInput, icon};
use crate::terminal::DEFAULT_FG;
use crate::ui::theme::{self, color_of};
use gpui::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;

const POPUP_MAX_HEIGHT: Pixels = px(260.);
const ROW_HEIGHT: Pixels = px(26.);
/// Same width as the settings control column, so the popup lines up with the trigger.
const POPUP_WIDTH: Pixels = px(300.);

pub(crate) struct Combobox {
    open: bool,
    input: Entity<TextInput>,
    /// Text the input was prefilled with. While the input still equals it the
    /// list is not filtered, so the current name does not hide every other font.
    opened_with: String,
    /// Index into the filtered list, not into the full item list.
    highlight: Cell<usize>,
    /// Last input text seen, to reset the highlight when it changes.
    seen: RefCell<String>,
    scroll: UniformListScrollHandle,
    /// Free window space (below, above) the trigger, refreshed on every paint
    /// so the popup can pick a side and a height that fit.
    space: Rc<Cell<(Pixels, Pixels)>>,
}

impl Combobox {
    pub(crate) fn new(placeholder: &'static str, cx: &mut App) -> Self {
        Self {
            open: false,
            input: cx.new(|cx| TextInput::new(placeholder, cx)),
            opened_with: String::new(),
            highlight: Cell::new(0),
            seen: RefCell::new(String::new()),
            scroll: UniformListScrollHandle::new(),
            space: Rc::new(Cell::new((px(10_000.), px(0.)))),
        }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.input.read(cx).focus_handle()
    }

    /// Opens the popup with `current` in the input (all selected) and
    /// highlights it in the list.
    pub(crate) fn open(&mut self, current: &str, items: &[String], cx: &mut App) {
        self.open = true;
        self.opened_with = current.to_string();
        *self.seen.borrow_mut() = current.to_string();
        self.input
            .update(cx, |input, cx| input.set_text(current, cx));
        let at = filter(items, "")
            .iter()
            .position(|&i| items[i] == current)
            .unwrap_or(0);
        self.highlight.set(at);
        self.scroll.scroll_to_item(at, ScrollStrategy::Center);
    }

    pub(crate) fn close(&mut self) {
        self.open = false;
    }

    /// Text that filters the list: empty until the user edits the prefill.
    fn filter_text(&self, cx: &App) -> String {
        let text = self.input.read(cx).text();
        if text == self.opened_with {
            String::new()
        } else {
            text.to_string()
        }
    }

    /// Resets the highlight to the top when the typed text changed.
    fn sync_highlight(&self, text: &str) {
        let mut seen = self.seen.borrow_mut();
        if *seen != text {
            *seen = text.to_string();
            self.highlight.set(0);
            self.scroll.scroll_to_item(0, ScrollStrategy::Top);
        }
    }

    /// Handles a navigation key while open. Returns the item picked with Enter.
    pub(crate) fn on_key(&mut self, ks: &Keystroke, items: &[String], cx: &App) -> Option<String> {
        let query = self.filter_text(cx);
        self.sync_highlight(self.input.read(cx).text());
        let matches = filter(items, &query);
        match ks.key.as_str() {
            "escape" => self.close(),
            "enter" => {
                let pick = matches.get(self.highlight.get()).map(|&i| items[i].clone());
                if pick.is_some() {
                    self.close();
                }
                return pick;
            }
            "down" => self.move_highlight(1, matches.len()),
            "up" => self.move_highlight(-1, matches.len()),
            _ => {}
        }
        None
    }

    fn move_highlight(&self, step: isize, len: usize) {
        if len == 0 {
            self.highlight.set(0);
            return;
        }
        // Wraps around so the list can be walked with the arrow keys alone.
        let at = (self.highlight.get() as isize + step).rem_euclid(len as isize) as usize;
        self.highlight.set(at);
        self.scroll.scroll_to_item(at, ScrollStrategy::Top);
    }

    /// Trigger plus, while open, the deferred popup under (or over) it.
    ///
    /// `on_toggle` is called on a left click on the trigger outside the input;
    /// `on_pick` with the chosen item. Both stop propagation so the root
    /// view's click-outside handler does not immediately close the popup.
    pub(crate) fn render(
        &self,
        id: impl Into<ElementId>,
        current: &str,
        items: &[String],
        cx: &App,
        on_toggle: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
        on_pick: impl Fn(String, &mut Window, &mut App) + 'static,
    ) -> impl IntoElement {
        let id = id.into();
        let trigger = div()
            .id(id.clone())
            .flex()
            .items_center()
            .justify_between()
            .gap_2()
            .h(px(30.))
            .px_2()
            .w_full()
            .rounded(theme::CONTROL_RADIUS)
            .border_1()
            .border_color(color_of(theme::BORDER))
            .bg(color_of(theme::CHROME))
            .text_color(color_of(DEFAULT_FG))
            .cursor_pointer()
            .hover(|s| s.bg(color_of(theme::HOVER)))
            .on_mouse_down(MouseButton::Left, move |ev, window, cx| {
                cx.stop_propagation();
                on_toggle(ev, window, cx);
            })
            .child({
                let space = self.space.clone();
                canvas(
                    move |bounds, window, _| {
                        let height = window.viewport_size().height;
                        space.set((height - bounds.bottom(), bounds.top()));
                    },
                    |_, _, _, _| {},
                )
                .absolute()
                .size_full()
            })
            .child(self.trigger_content(current))
            .child(
                div()
                    .flex_none()
                    .text_color(color_of(theme::MUTED))
                    .child(icon(
                        if self.open {
                            "icons/chevron-up.svg"
                        } else {
                            "icons/chevron-down.svg"
                        },
                        14.,
                        theme::MUTED,
                    )),
            );

        let mut wrapper = div().relative().w_full().child(trigger);
        if self.open {
            let query = self.filter_text(cx);
            self.sync_highlight(self.input.read(cx).text());
            // Deferred so the popup paints above the rest of the panel.
            let (below, above) = self.space.get();
            let margin = px(12.);
            let below_fits = below - margin >= px(160.) || below >= above;
            let room = if below_fits { below } else { above } - margin;
            let list_max = POPUP_MAX_HEIGHT.min(room - px(12.));
            let mut holder = div().absolute().left_0();
            holder = if below_fits {
                holder.top_full().mt(px(4.))
            } else {
                holder.bottom_full().mb(px(4.))
            };
            wrapper = wrapper.child(deferred(holder.child(self.popup(
                current,
                items,
                &query,
                list_max,
                Rc::new(on_pick),
            ))));
        }
        wrapper
    }

    /// Left side of the trigger: the current family when closed; while open,
    /// the text input.
    fn trigger_content(&self, current: &str) -> Div {
        let row = div().flex().flex_1().items_center().gap_2().min_w(px(0.));
        if !self.open {
            return row.child(
                div()
                    .truncate()
                    .font_family(current.to_string())
                    .child(current.to_string()),
            );
        }
        row.child(self.input.clone())
    }

    fn popup(
        &self,
        current: &str,
        items: &[String],
        query: &str,
        list_max: Pixels,
        on_pick: Rc<dyn Fn(String, &mut Window, &mut App)>,
    ) -> impl IntoElement {
        let matches = filter(items, query);
        let list = if matches.is_empty() {
            div()
                .px_2()
                .py_1()
                .text_color(color_of(theme::MUTED))
                .child("No fonts found")
                .into_any_element()
        } else {
            // Only the visible rows are built: shaping every font name in its
            // own font made opening the popup lag.
            let names: Vec<(usize, String)> =
                matches.iter().map(|&i| (i, items[i].clone())).collect();
            let height = (ROW_HEIGHT * names.len() as f32).min(list_max);
            let highlight = self.highlight.get();
            let current = current.to_string();
            uniform_list("combobox-list", names.len(), move |range, _, _| {
                range
                    .map(|row| {
                        let (index, name) = names[row].clone();
                        let picked = name.clone();
                        let on_pick = on_pick.clone();
                        let mut row_el = div()
                            .id(("combobox-option", index))
                            .flex()
                            .items_center()
                            .w_full()
                            .h(ROW_HEIGHT)
                            .px_2()
                            .rounded(theme::CHIP_RADIUS)
                            .truncate()
                            .font_family(name.clone());
                        if row == highlight {
                            row_el = row_el.bg(color_of(theme::SELECTION));
                        } else {
                            row_el = row_el.hover(|s| s.bg(color_of(theme::HOVER)));
                        }
                        row_el = row_el.cursor_pointer();
                        if name == current {
                            row_el = row_el.text_color(color_of(theme::ACCENT));
                        }
                        row_el
                            .on_mouse_down(MouseButton::Left, move |_, window, cx| {
                                cx.stop_propagation();
                                on_pick(picked.clone(), window, cx);
                            })
                            .child(name)
                    })
                    .collect()
            })
            .track_scroll(&self.scroll)
            .h(height)
            .into_any_element()
        };

        div()
            .id("combobox-popup")
            // Deferred content does not inherit the parent text color.
            .text_color(color_of(DEFAULT_FG))
            .w(POPUP_WIDTH)
            .occlude()
            .flex()
            .flex_col()
            .rounded(theme::SURFACE_RADIUS)
            .border_1()
            .border_color(color_of(theme::BORDER))
            .bg(color_of(theme::CHROME))
            .shadow_md()
            .p_1()
            .gap_1()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(list)
    }
}

/// Indices of `items` whose name contains `query` (case-insensitive).
/// Names starting with the query come first; the rest keep their order.
fn filter(items: &[String], query: &str) -> Vec<usize> {
    let needle = query.to_lowercase();
    let mut hits: Vec<usize> = items
        .iter()
        .enumerate()
        .filter(|(_, name)| name.to_lowercase().contains(&needle))
        .map(|(i, _)| i)
        .collect();
    hits.sort_by_key(|&i| !items[i].to_lowercase().starts_with(&needle));
    hits
}
