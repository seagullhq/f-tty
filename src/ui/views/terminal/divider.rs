//! Dividers between panes: the separator line, the grab area that resizes the
//! split, and the hover and drag feedback. Panes themselves live in `pane.rs`.

use crate::app::TerminalView;
use crate::ui::theme::{self, color_of};
use gpui::prelude::FluentBuilder;
use gpui::*;

/// One element per divider of the active tab; none while a pane is zoomed.
pub(crate) fn divider_elements(
    view: &TerminalView,
    cx: &mut Context<TerminalView>,
) -> Vec<AnyElement> {
    let tab = &view.tabs[view.active];
    let (base_x, base_y) = (px(tab.rect.0), px(tab.rect.1));
    let mut children: Vec<AnyElement> = Vec::new();
    // Grab strips between panes: drag to resize, double click to equalize.
    // The grab area is wider than the gap and centred on it.
    if !tab.zoomed {
        for (k, d) in tab
            .tree
            .layout(tab.rect, f32::from(theme::SPLIT_GAP))
            .dividers
            .into_iter()
            .enumerate()
        {
            let (x, y, w, h) = d.rect;
            let hit = f32::from(theme::SPLIT_HIT);
            let (hx, hy, hw, hh) = if d.axis == Axis::Horizontal {
                (x + (w - hit) / 2., y, hit, h)
            } else {
                (x, y + (h - hit) / 2., w, hit)
            };
            let dragging = view
                .split_drag
                .as_ref()
                .is_some_and(|s| s.is_on(&d.path, d.index));
            let (path, index) = (d.path, d.index);
            // A handle the width of the bar, running the whole length with square
            // ends, so it covers the junctions too.
            let group: SharedString = format!("divider-{k}").into();
            let bar = f32::from(theme::SPLIT_BAR);
            let (handle_x, handle_y, handle_w, handle_h) = if d.axis == Axis::Horizontal {
                ((hw - bar) / 2., 0., bar, hh)
            } else {
                (0., (hh - bar) / 2., hw, bar)
            };
            let handle = div()
                .absolute()
                .left(px(handle_x))
                .top(px(handle_y))
                .w(px(handle_w))
                .h(px(handle_h))
                // Always visible as the separator; accent on hover and drag.
                .when(dragging, |s| s.bg(color_of(theme::ACCENT)))
                .when(!dragging, |s| {
                    s.bg(color_of(theme::BORDER))
                        .group_hover(group.clone(), |s| {
                            s.bg(color_of(theme::ACCENT).opacity(0.6))
                        })
                });
            let strip = div()
                .id(("divider", k))
                .group(group)
                .absolute()
                .left(px(hx) - base_x)
                .top(px(hy) - base_y)
                .w(px(hw))
                .h(px(hh))
                .child(handle)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, ev: &MouseDownEvent, _, cx| {
                        this.start_divider_drag(path.clone(), index, ev, cx);
                    }),
                );
            children.push(
                if d.axis == Axis::Horizontal {
                    strip.cursor_col_resize()
                } else {
                    strip.cursor_row_resize()
                }
                .into_any_element(),
            );
        }
    }

    children
}
