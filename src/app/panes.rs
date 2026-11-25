//! Pane management inside a tab: split, focus, close, resize, zoom, swap and
//! broadcast. The arrangement itself lives in `tree.rs`.

use crate::app::tree::{self, Dir, resize_weights};
use crate::app::{MAX_PANES, Pane, TerminalView};
use crate::ui::theme;
use gpui::*;

/// A divider being dragged: where it is in the tree and the numbers needed
/// to turn pointer movement into weights.
pub(crate) struct SplitDrag {
    path: Vec<usize>,
    index: usize,
    axis: Axis,
    /// Pointer position along the axis when the drag started.
    start: Pixels,
    weights: Vec<f32>,
    /// Room the split's children share, and the smallest each side may get.
    usable: f32,
    min_a: f32,
    min_b: f32,
}

/// Smallest pane: columns across, lines down.
const MIN_COLS: f32 = 8.;
const MIN_ROWS: f32 = 3.;
/// Keyboard resize moves a border this many cells per press.
const STEP_CELLS: f32 = 2.;

impl TerminalView {
    fn reset_pointer_state(&mut self) {
        self.selection = None;
        self.selecting = false;
    }

    fn gap() -> f32 {
        f32::from(theme::SPLIT_GAP)
    }

    /// Smallest width and height a pane may be squeezed to, in pixels.
    fn min_pane(&self) -> (f32, f32) {
        (
            f32::from(self.cell_width) * MIN_COLS,
            f32::from(self.line_height) * MIN_ROWS,
        )
    }

    /// Splits the focused pane: the new one opens right of it (`Horizontal`)
    /// or below it (`Vertical`), in the same directory, and takes focus.
    pub(crate) fn split_pane(&mut self, axis: Axis, cx: &mut Context<Self>) {
        let id = self.next_id;
        let shell = self.shell.clone();
        let (min_w, min_h) = self.min_pane();
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if tab.panes.len() >= MAX_PANES {
            return;
        }
        // Refuse a split that would leave either half under the minimum.
        let layout = tab.tree.layout(tab.rect, Self::gap());
        let Some((_, (_, _, w, h))) = layout.panes.iter().find(|(p, _)| *p == tab.focus_id())
        else {
            return;
        };
        let fits = if axis == Axis::Horizontal {
            *w >= min_w * 2. + Self::gap()
        } else {
            *h >= min_h * 2. + Self::gap()
        };
        if !fits {
            return;
        }
        let pane = Pane::spawn(id, &shell, tab.emu.term.cwd.as_deref());
        if tab.add_pane(pane, axis) {
            self.next_id += 1;
        }
        self.settings_open = false;
        self.reset_pointer_state();
        cx.notify();
    }

    pub(crate) fn focus_pane(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if index < tab.panes.len() && index != tab.focus {
            tab.focus = index;
            tab.zoomed = false;
            self.reset_pointer_state();
            cx.notify();
        }
    }

    /// Previous / next pane in tree order, wrapping.
    pub(crate) fn cycle_pane(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let order = tab.tree.leaves();
        let at = order
            .iter()
            .position(|id| *id == tab.focus_id())
            .unwrap_or(0);
        let id = order[(at as isize + delta).rem_euclid(order.len() as isize) as usize];
        if let Some(index) = tab.index_of(id) {
            self.focus_pane(index, cx);
        }
    }

    /// Focus the pane that lies `dir` of the focused one.
    pub(crate) fn focus_direction(&mut self, dir: Dir, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get(self.active) else {
            return;
        };
        let rects = tab.tree.layout(tab.rect, Self::gap()).panes;
        if let Some(index) = tree::neighbour(&rects, tab.focus_id(), dir, Self::gap())
            .and_then(|id| tab.index_of(id))
        {
            self.focus_pane(index, cx);
        }
    }

    /// Closes the focused pane, or the whole tab when it is the only one.
    pub(crate) fn close_pane(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if tab.panes.len() < 2 {
            return self.close_tab(self.active, window, cx);
        }
        tab.remove_pane(tab.focus);
        self.reset_pointer_state();
        cx.notify();
    }

    pub(crate) fn toggle_zoom(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if tab.panes.len() > 1 {
                tab.zoomed = !tab.zoomed;
                self.reset_pointer_state();
                cx.notify();
            }
        }
    }

    pub(crate) fn toggle_broadcast(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            if tab.panes.len() > 1 || tab.broadcast {
                tab.broadcast = !tab.broadcast;
                cx.notify();
            }
        }
    }

    /// Swaps the focused pane with the previous / next one in tree order.
    /// Positions and sizes stay; the panes trade places and focus follows.
    pub(crate) fn swap_pane(&mut self, delta: isize, cx: &mut Context<Self>) {
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        let order = tab.tree.leaves();
        if order.len() < 2 {
            return;
        }
        let me = tab.focus_id();
        let at = order.iter().position(|id| *id == me).unwrap_or(0);
        let other = order[(at as isize + delta).rem_euclid(order.len() as isize) as usize];
        tab.tree.swap_leaves(me, other);
        tab.zoomed = false;
        self.reset_pointer_state();
        cx.notify();
    }

    pub(crate) fn equalize_panes(&mut self, cx: &mut Context<Self>) {
        if let Some(tab) = self.tabs.get_mut(self.active) {
            tab.tree.equalize();
            cx.notify();
        }
    }

    /// Ctrl+Alt+Shift+arrow: the border of the nearest split running that
    /// way moves one step in the arrow's direction (as WezTerm does).
    pub(crate) fn resize_pane_key(&mut self, dir: Dir, cx: &mut Context<Self>) {
        let (min_w, min_h) = self.min_pane();
        let step = f32::from(if dir.axis() == Axis::Horizontal {
            self.cell_width
        } else {
            self.line_height
        }) * STEP_CELLS;
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if tab.zoomed {
            return;
        }
        let Some((path, boundary)) = tab.tree.border_for(tab.focus_id(), dir.axis()) else {
            return;
        };
        let layout = tab.tree.layout(tab.rect, Self::gap());
        let Some(usable) = layout
            .splits
            .iter()
            .find(|s| s.path == path)
            .map(|s| s.usable)
        else {
            return;
        };
        let (Some(node), Some(mut weights)) = (
            tab.tree.node(&path),
            tab.tree.weights_at(&path).map(<[f32]>::to_vec),
        ) else {
            return;
        };
        let (min_a, min_b) = border_minimums(node, boundary, dir.axis(), min_w, min_h);
        resize_weights(
            &mut weights,
            boundary,
            dir.sign() * step / usable,
            min_a / usable,
            min_b / usable,
        );
        tab.tree.set_weights(&path, weights);
        cx.notify();
    }

    /// Mouse down on a divider; a double click equalizes that split instead.
    pub(crate) fn start_divider_drag(
        &mut self,
        path: Vec<usize>,
        index: usize,
        ev: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let (min_w, min_h) = self.min_pane();
        let Some(tab) = self.tabs.get_mut(self.active) else {
            return;
        };
        if ev.click_count >= 2 {
            tab.tree.equalize_at(&path);
            return cx.notify();
        }
        let layout = tab.tree.layout(tab.rect, Self::gap());
        let (Some(info), Some(node), Some(weights)) = (
            layout.splits.iter().find(|s| s.path == path),
            tab.tree.node(&path),
            tab.tree.weights_at(&path),
        ) else {
            return;
        };
        let axis = match node {
            tree::Tree::Split { axis, .. } => *axis,
            tree::Tree::Leaf(_) => return,
        };
        let (min_a, min_b) = border_minimums(node, index, axis, min_w, min_h);
        let start = if axis == Axis::Horizontal {
            ev.position.x
        } else {
            ev.position.y
        };
        self.split_drag = Some(SplitDrag {
            path,
            index,
            axis,
            start,
            weights: weights.to_vec(),
            usable: info.usable,
            min_a,
            min_b,
        });
    }

    /// Follows the pointer while a divider is dragged.
    pub(crate) fn drag_divider(&mut self, ev: &MouseMoveEvent, cx: &mut Context<Self>) {
        let Some(drag) = &self.split_drag else {
            return;
        };
        let zoomed = self.tabs.get(self.active).is_none_or(|t| t.zoomed);
        if ev.pressed_button != Some(MouseButton::Left) || zoomed {
            self.split_drag = None;
            return;
        }
        let pos = if drag.axis == Axis::Horizontal {
            ev.position.x
        } else {
            ev.position.y
        };
        let mut weights = drag.weights.clone();
        resize_weights(
            &mut weights,
            drag.index,
            f32::from(pos - drag.start) / drag.usable,
            drag.min_a / drag.usable,
            drag.min_b / drag.usable,
        );
        let path = drag.path.clone();
        let tab = &mut self.tabs[self.active];
        if tab.tree.weights_at(&path) != Some(&weights[..]) {
            tab.tree.set_weights(&path, weights);
            cx.notify();
        }
    }
}

/// Smallest extents of the two children on either side of `boundary`.
fn border_minimums(node: &tree::Tree, boundary: usize, axis: Axis, w: f32, h: f32) -> (f32, f32) {
    let gap = TerminalView::gap();
    match node {
        tree::Tree::Split { kids, .. } => (
            kids[boundary].min_extent(axis, w, h, gap),
            kids[boundary + 1].min_extent(axis, w, h, gap),
        ),
        tree::Tree::Leaf(_) => (0., 0.),
    }
}
