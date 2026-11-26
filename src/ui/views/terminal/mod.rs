//! Terminal view: the container of the active tab, its pane boxes with their
//! text, and the dividers between them. The layout itself lives in `app::tree`;
//! the grid text helpers live in `text.rs`.

use crate::app::TerminalView;
use crate::terminal::DEFAULT_BG;
use crate::ui::theme::color_of;
use gpui::*;

mod divider;
mod pane;
mod text;

pub(crate) use divider::divider_elements;
pub(crate) use pane::pane_elements;

pub(crate) fn terminal_screen(view: &TerminalView, cx: &mut Context<TerminalView>) -> Div {
    let mut children = pane_elements(view, cx);
    children.extend(divider_elements(view, cx));
    // Not clipped: dividers that end on the edge run into the card padding
    // so they meet the card border. Panes are laid out inside the area.
    div()
        .relative()
        .flex_1()
        .w_full()
        .min_w(px(0.))
        .min_h(px(0.))
        .bg(color_of(DEFAULT_BG))
        .font_family(view.font.clone())
        .text_sm()
        .on_mouse_move(cx.listener(TerminalView::on_terminal_mouse_move))
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(TerminalView::on_terminal_mouse_up),
        )
        .children(children)
}
