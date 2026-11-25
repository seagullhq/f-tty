//! Split tree of a tab: panes are leaves, splits are rows or columns of
//! children that share their room by weight. Pure geometry (f32 pixels), so
//! it is testable without a window.
//!
//! Modeled on tmux's layout tree: splitting a pane whose parent already runs
//! the same way adds a sibling; otherwise the pane becomes a new split of two.
//! Closing a pane gives its share to the previous sibling (else the next).

use gpui::Axis;

/// x, y, width, height.
pub(crate) type Rect = (f32, f32, f32, f32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Dir {
    Left,
    Right,
    Up,
    Down,
}

impl Dir {
    pub(crate) fn parse(key: &str) -> Option<Self> {
        Some(match key {
            "left" | "arrowleft" => Self::Left,
            "right" | "arrowright" => Self::Right,
            "up" | "arrowup" => Self::Up,
            "down" | "arrowdown" => Self::Down,
            _ => return None,
        })
    }

    pub(crate) fn axis(self) -> Axis {
        match self {
            Self::Left | Self::Right => Axis::Horizontal,
            Self::Up | Self::Down => Axis::Vertical,
        }
    }

    pub(crate) fn sign(self) -> f32 {
        match self {
            Self::Left | Self::Up => -1.,
            Self::Right | Self::Down => 1.,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Tree {
    /// A pane, by id.
    Leaf(usize),
    /// `Horizontal` puts children side by side, `Vertical` stacks them.
    Split {
        axis: Axis,
        kids: Vec<Tree>,
        weights: Vec<f32>,
    },
}

/// The strip between two children of a split.
pub(crate) struct Divider {
    /// Child indices from the root to the split that owns this divider.
    pub(crate) path: Vec<usize>,
    /// Sits between children `index` and `index + 1`.
    pub(crate) index: usize,
    pub(crate) axis: Axis,
    pub(crate) rect: Rect,
}

/// A split and the room its children share (its extent minus the gaps).
pub(crate) struct SplitInfo {
    pub(crate) path: Vec<usize>,
    pub(crate) usable: f32,
}

#[derive(Default)]
pub(crate) struct Layout {
    pub(crate) panes: Vec<(usize, Rect)>,
    pub(crate) dividers: Vec<Divider>,
    pub(crate) splits: Vec<SplitInfo>,
}

impl Tree {
    /// Pane ids in tree order (left to right, top to bottom).
    pub(crate) fn leaves(&self) -> Vec<usize> {
        match self {
            Self::Leaf(id) => vec![*id],
            Self::Split { kids, .. } => kids.iter().flat_map(Self::leaves).collect(),
        }
    }

    pub(crate) fn path_to(&self, id: usize) -> Option<Vec<usize>> {
        match self {
            Self::Leaf(leaf) => (*leaf == id).then(Vec::new),
            Self::Split { kids, .. } => kids.iter().enumerate().find_map(|(i, kid)| {
                let mut path = kid.path_to(id)?;
                path.insert(0, i);
                Some(path)
            }),
        }
    }

    pub(crate) fn node(&self, path: &[usize]) -> Option<&Self> {
        match (path.split_first(), self) {
            (None, _) => Some(self),
            (Some((i, rest)), Self::Split { kids, .. }) => kids.get(*i)?.node(rest),
            _ => None,
        }
    }

    fn node_mut(&mut self, path: &[usize]) -> Option<&mut Self> {
        match (path.split_first(), self) {
            (None, node) => Some(node),
            (Some((i, rest)), Self::Split { kids, .. }) => kids.get_mut(*i)?.node_mut(rest),
            _ => None,
        }
    }

    /// Opens `new` next to `target` (right or below it, by `axis`).
    pub(crate) fn split_leaf(&mut self, target: usize, new: usize, axis: Axis) -> bool {
        match self {
            Self::Leaf(id) if *id == target => {
                *self = Self::Split {
                    axis,
                    kids: vec![Self::Leaf(target), Self::Leaf(new)],
                    weights: vec![0.5, 0.5],
                };
                true
            }
            Self::Leaf(_) => false,
            Self::Split {
                axis: own,
                kids,
                weights,
            } => {
                if *own == axis {
                    if let Some(i) = kids.iter().position(|k| *k == Self::Leaf(target)) {
                        let half = weights[i] / 2.;
                        weights[i] = half;
                        weights.insert(i + 1, half);
                        kids.insert(i + 1, Self::Leaf(new));
                        return true;
                    }
                }
                kids.iter_mut().any(|k| k.split_leaf(target, new, axis))
            }
        }
    }

    /// Removes a pane (never the only one); false if it is not here.
    pub(crate) fn remove_leaf(&mut self, id: usize) -> bool {
        let Self::Split { kids, weights, .. } = self else {
            return false;
        };
        if let Some(i) = kids.iter().position(|k| *k == Self::Leaf(id)) {
            kids.remove(i);
            let share = weights.remove(i);
            weights[i.saturating_sub(1)] += share;
        } else if !kids.iter_mut().any(|k| k.remove_leaf(id)) {
            return false;
        }
        self.tidy();
        true
    }

    /// Keeps the tree canonical: a one-child split becomes its child, and a
    /// child split that runs the same way as its parent is merged into it.
    fn tidy(&mut self) {
        let Self::Split {
            axis,
            kids,
            weights,
        } = self
        else {
            return;
        };
        let mut i = 0;
        while i < kids.len() {
            match &kids[i] {
                Self::Split { axis: inner, .. } if inner == axis => {
                    let share = weights.remove(i);
                    let Self::Split {
                        kids: inner_kids,
                        weights: inner_weights,
                        ..
                    } = kids.remove(i)
                    else {
                        unreachable!()
                    };
                    let count = inner_kids.len();
                    for (k, (kid, w)) in inner_kids.into_iter().zip(inner_weights).enumerate() {
                        kids.insert(i + k, kid);
                        weights.insert(i + k, w * share);
                    }
                    i += count;
                }
                _ => i += 1,
            }
        }
        if kids.len() == 1 {
            *self = kids.pop().unwrap();
        }
    }

    pub(crate) fn swap_leaves(&mut self, a: usize, b: usize) {
        match self {
            Self::Leaf(id) if *id == a => *id = b,
            Self::Leaf(id) if *id == b => *id = a,
            Self::Leaf(_) => {}
            Self::Split { kids, .. } => kids.iter_mut().for_each(|k| k.swap_leaves(a, b)),
        }
    }

    /// Every split gives its children equal shares.
    pub(crate) fn equalize(&mut self) {
        if let Self::Split { kids, weights, .. } = self {
            *weights = vec![1. / kids.len() as f32; kids.len()];
            kids.iter_mut().for_each(Self::equalize);
        }
    }

    /// Only the split at `path` gets equal shares.
    pub(crate) fn equalize_at(&mut self, path: &[usize]) {
        if let Some(Self::Split { kids, weights, .. }) = self.node_mut(path) {
            *weights = vec![1. / kids.len() as f32; kids.len()];
        }
    }

    pub(crate) fn weights_at(&self, path: &[usize]) -> Option<&[f32]> {
        match self.node(path)? {
            Self::Split { weights, .. } => Some(weights),
            Self::Leaf(_) => None,
        }
    }

    pub(crate) fn set_weights(&mut self, path: &[usize], new: Vec<f32>) {
        if let Some(Self::Split { weights, .. }) = self.node_mut(path) {
            if weights.len() == new.len() {
                *weights = new;
            }
        }
    }

    /// Smallest extent along `axis` this subtree can be squeezed to: panes
    /// need `min_w` x `min_h`, and splits add up their children's minimums
    /// plus gaps (along the split) or take the largest (across it).
    pub(crate) fn min_extent(&self, axis: Axis, min_w: f32, min_h: f32, gap: f32) -> f32 {
        match self {
            Self::Leaf(_) => {
                if axis == Axis::Horizontal {
                    min_w
                } else {
                    min_h
                }
            }
            Self::Split {
                axis: own, kids, ..
            } => {
                let sizes = kids.iter().map(|k| k.min_extent(axis, min_w, min_h, gap));
                if *own == axis {
                    sizes.sum::<f32>() + gap * (kids.len() - 1) as f32
                } else {
                    sizes.fold(0., f32::max)
                }
            }
        }
    }

    /// Nearest ancestor of `id` that splits along `axis`: its path and the
    /// border to move (the one after the pane's branch, or the one before it
    /// for the last branch).
    pub(crate) fn border_for(&self, id: usize, axis: Axis) -> Option<(Vec<usize>, usize)> {
        let path = self.path_to(id)?;
        (0..path.len()).rev().find_map(|depth| {
            let Some(Self::Split {
                axis: own, kids, ..
            }) = self.node(&path[..depth])
            else {
                return None;
            };
            (*own == axis).then(|| {
                let i = path[depth];
                (
                    path[..depth].to_vec(),
                    if i + 1 < kids.len() { i } else { i - 1 },
                )
            })
        })
    }

    /// Places every pane and divider inside `rect`, `gap` apart.
    pub(crate) fn layout(&self, rect: Rect, gap: f32) -> Layout {
        let mut out = Layout::default();
        self.place(&mut Vec::new(), rect, gap, &mut out);
        out
    }

    fn place(&self, path: &mut Vec<usize>, rect: Rect, gap: f32, out: &mut Layout) {
        let Self::Split {
            axis,
            kids,
            weights,
        } = self
        else {
            if let Self::Leaf(id) = self {
                out.panes.push((*id, rect));
            }
            return;
        };
        let (x, y, w, h) = rect;
        let horizontal = *axis == Axis::Horizontal;
        let along = if horizontal { w } else { h };
        let usable = (along - gap * (kids.len() - 1) as f32).max(kids.len() as f32);
        out.splits.push(SplitInfo {
            path: path.clone(),
            usable,
        });
        let mut offset = 0.;
        for (i, (kid, weight)) in kids.iter().zip(weights).enumerate() {
            let extent = (usable * weight).max(1.);
            let child = if horizontal {
                (x + offset, y, extent, h)
            } else {
                (x, y + offset, w, extent)
            };
            path.push(i);
            kid.place(path, child, gap, out);
            path.pop();
            offset += extent;
            if i + 1 < kids.len() {
                out.dividers.push(Divider {
                    path: path.clone(),
                    index: i,
                    axis: *axis,
                    rect: if horizontal {
                        (x + offset, y, gap, h)
                    } else {
                        (x, y + offset, w, gap)
                    },
                });
                offset += gap;
            }
        }
    }
}

/// Moves the border between children `boundary` and `boundary + 1` by
/// `delta` (a fraction of the split), keeping them at least `min_a` and
/// `min_b` wide. Squeezed beyond what fits, it splits the difference.
pub(crate) fn resize_weights(
    weights: &mut [f32],
    boundary: usize,
    delta: f32,
    min_a: f32,
    min_b: f32,
) {
    let total = weights[boundary] + weights[boundary + 1];
    let (low, high) = (min_a, total - min_b);
    let first = if low > high {
        total * min_a / (min_a + min_b)
    } else {
        (weights[boundary] + delta).clamp(low, high)
    };
    weights[boundary] = first;
    weights[boundary + 1] = total - first;
}

/// Pane to focus when moving `dir` from pane `from`: the adjacent pane that
/// shares the most edge with it. With nothing that way it wraps to the far
/// side, like tmux does.
pub(crate) fn neighbour(rects: &[(usize, Rect)], from: usize, dir: Dir, gap: f32) -> Option<usize> {
    const EPS: f32 = 0.5;
    let (_, me) = rects.iter().find(|(id, _)| *id == from)?;
    let horizontal = dir.axis() == Axis::Horizontal;
    let span = |r: &Rect| {
        if horizontal {
            (r.0, r.0 + r.2)
        } else {
            (r.1, r.1 + r.3)
        }
    };
    let cross = |r: &Rect| {
        if horizontal {
            (r.1, r.1 + r.3)
        } else {
            (r.0, r.0 + r.2)
        }
    };
    let (mine, my_cross) = (span(me), cross(me));
    let overlap = |r: &Rect| {
        let c = cross(r);
        my_cross.1.min(c.1) - my_cross.0.max(c.0)
    };

    let mut best: Option<(usize, f32)> = None;
    for (id, r) in rects.iter().filter(|(id, _)| *id != from) {
        let (s, ov) = (span(r), overlap(r));
        let distance = if dir.sign() > 0. {
            s.0 - mine.1
        } else {
            mine.0 - s.1
        };
        if ov > EPS
            && (-EPS..=gap + EPS).contains(&distance)
            && best.is_none_or(|(_, b)| ov > b + EPS)
        {
            best = Some((*id, ov));
        }
    }
    if let Some((id, _)) = best {
        return Some(id);
    }

    // Wrap: among panes across from this one, the farthest in the opposite
    // direction (most overlap on a tie).
    let mut far: Option<(usize, f32, f32)> = None;
    for (id, r) in rects.iter().filter(|(id, _)| *id != from) {
        let ov = overlap(r);
        let s = span(r);
        let pos = if dir.sign() > 0. { s.0 } else { -s.1 };
        if ov > EPS
            && far.is_none_or(|(_, p, o)| pos < p - EPS || ((pos - p).abs() <= EPS && ov > o))
        {
            far = Some((*id, pos, ov));
        }
    }
    far.map(|(id, _, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: Axis = Axis::Horizontal;
    const V: Axis = Axis::Vertical;

    /// 1 | (2 over 3): split 1 right, then 2 down.
    fn sample() -> Tree {
        let mut t = Tree::Leaf(1);
        assert!(t.split_leaf(1, 2, H));
        assert!(t.split_leaf(2, 3, V));
        t
    }

    #[::core::prelude::v1::test]
    fn same_direction_adds_a_sibling_other_direction_nests() {
        let mut t = Tree::Leaf(1);
        t.split_leaf(1, 2, H);
        t.split_leaf(2, 3, H);
        // Three side by side, not a tree of pairs; 2 gave half to 3.
        assert_eq!(
            t,
            Tree::Split {
                axis: H,
                kids: vec![Tree::Leaf(1), Tree::Leaf(2), Tree::Leaf(3)],
                weights: vec![0.5, 0.25, 0.25],
            }
        );
        // Splitting 2 downwards makes a column inside the row.
        t.split_leaf(2, 4, V);
        assert_eq!(t.leaves(), vec![1, 2, 4, 3]);
        assert_eq!(t.path_to(4), Some(vec![1, 1]));
    }

    #[::core::prelude::v1::test]
    fn layout_fills_the_rect_with_gaps_between() {
        let l = sample().layout((10., 20., 205., 100.), 5.);
        // Row of two: (205 - 5) shared 50/50, then the right half stacked.
        let rect = |id| l.panes.iter().find(|(i, _)| *i == id).unwrap().1;
        assert_eq!(rect(1), (10., 20., 100., 100.));
        assert_eq!(rect(2), (115., 20., 100., 47.5));
        assert_eq!(rect(3), (115., 72.5, 100., 47.5));
        assert_eq!(l.dividers.len(), 2);
        assert_eq!(l.dividers[0].rect, (110., 20., 5., 100.)); // vertical bar
        assert_eq!(l.dividers[1].path, vec![1]); // the one inside the column
        assert_eq!(
            l.splits.iter().find(|s| s.path.is_empty()).unwrap().usable,
            200.
        );
    }

    #[::core::prelude::v1::test]
    fn closing_gives_space_to_the_previous_sibling_and_collapses() {
        let mut t = sample();
        assert!(t.remove_leaf(3));
        // 2 is alone in its column: the column collapses into the row.
        assert_eq!(
            t,
            Tree::Split {
                axis: H,
                kids: vec![Tree::Leaf(1), Tree::Leaf(2)],
                weights: vec![0.5, 0.5],
            }
        );
        assert!(t.remove_leaf(1));
        assert_eq!(t, Tree::Leaf(2));
        assert!(!t.remove_leaf(2), "the last pane stays");

        // Removing the first child hands its share to the next one.
        let mut t = Tree::Leaf(1);
        t.split_leaf(1, 2, H);
        t.split_leaf(2, 3, H);
        t.remove_leaf(1);
        assert_eq!(t.weights_at(&[]), Some(&[0.75, 0.25][..]));
    }

    #[::core::prelude::v1::test]
    fn closing_merges_a_same_direction_child_into_its_parent() {
        // Row [1, column[2, row[3, 4]]]: closing 2 leaves the inner row to
        // take the column's place, and it must merge into the outer row.
        let mut t = Tree::Leaf(1);
        t.split_leaf(1, 2, H);
        t.split_leaf(2, 5, V);
        t.split_leaf(5, 3, H);
        t.split_leaf(3, 4, H);
        assert!(t.remove_leaf(2));
        assert_eq!(t.leaves(), vec![1, 5, 3, 4]);
        let Tree::Split {
            axis,
            kids,
            weights,
        } = &t
        else {
            panic!()
        };
        assert_eq!((*axis, kids.len()), (H, 4));
        assert!((weights.iter().sum::<f32>() - 1.).abs() < 1e-6);
    }

    #[::core::prelude::v1::test]
    fn resizing_respects_both_minimums() {
        let mut w = vec![0.5, 0.25, 0.25];
        resize_weights(&mut w, 0, 0.1, 0.1, 0.1);
        assert!((w[0] - 0.6).abs() < 1e-6 && (w[1] - 0.15).abs() < 1e-6 && w[2] == 0.25);
        resize_weights(&mut w, 0, 9., 0.1, 0.1);
        assert!((w[1] - 0.1).abs() < 1e-6, "neighbour stops at its minimum");
        resize_weights(&mut w, 0, -9., 0.1, 0.1);
        assert!((w[0] - 0.1).abs() < 1e-6);
        // Not enough room for both minimums: shared in proportion.
        let mut tight = vec![0.1, 0.1];
        resize_weights(&mut tight, 0, 0.05, 0.15, 0.05);
        assert!((tight[0] - 0.15).abs() < 1e-6 && (tight[1] - 0.05).abs() < 1e-6);
    }

    #[::core::prelude::v1::test]
    fn border_and_minimum_follow_the_nearest_split_that_matches() {
        let t = sample();
        // Pane 3 sits in a column: Vertical resize moves that column's border
        // (before it, as it is last); Horizontal resize reaches the root.
        assert_eq!(t.border_for(3, V), Some((vec![1], 0)));
        assert_eq!(t.border_for(3, H), Some((vec![], 0)));
        assert_eq!(t.border_for(1, V), None);
        // Min width: the column needs one pane's width, the row adds two.
        assert_eq!(t.min_extent(H, 8., 3., 1.), 8. + 1. + 8.);
        assert_eq!(t.min_extent(V, 8., 3., 1.), 3. + 1. + 3.);
    }

    #[::core::prelude::v1::test]
    fn equalize_and_swap() {
        let mut t = sample();
        t.set_weights(&[], vec![0.8, 0.2]);
        t.set_weights(&[1], vec![0.1, 0.9]);
        t.equalize_at(&[1]);
        assert_eq!(t.weights_at(&[]), Some(&[0.8, 0.2][..]));
        assert_eq!(t.weights_at(&[1]), Some(&[0.5, 0.5][..]));
        t.equalize();
        assert_eq!(t.weights_at(&[]), Some(&[0.5, 0.5][..]));
        t.swap_leaves(1, 3);
        assert_eq!(t.leaves(), vec![3, 2, 1]);
    }

    #[::core::prelude::v1::test]
    fn neighbours_by_geometry_with_wrap() {
        let l = sample().layout((0., 0., 205., 100.), 5.);
        let go = |from, dir| neighbour(&l.panes, from, dir, 5.);
        assert_eq!(go(1, Dir::Right), Some(2), "ties go to the first pane");
        assert_eq!(go(2, Dir::Left), Some(1));
        assert_eq!(go(3, Dir::Left), Some(1));
        assert_eq!(go(2, Dir::Down), Some(3));
        assert_eq!(go(3, Dir::Up), Some(2));
        // Wrap: right of the rightmost goes back to the left side; down from
        // the bottom pane returns to the top one of the same column.
        assert_eq!(go(3, Dir::Right), Some(1));
        assert_eq!(go(3, Dir::Down), Some(2));
        // Nothing shares an edge across a single pane.
        assert_eq!(
            neighbour(&[(1, (0., 0., 10., 10.))], 1, Dir::Left, 5.),
            None
        );
    }

    #[::core::prelude::v1::test]
    fn most_shared_edge_wins() {
        // Left: one tall pane. Right: a short pane on top and a taller below.
        let mut t = Tree::Leaf(1);
        t.split_leaf(1, 2, H);
        t.split_leaf(2, 3, V);
        t.set_weights(&[1], vec![0.2, 0.8]);
        let l = t.layout((0., 0., 205., 100.), 5.);
        assert_eq!(neighbour(&l.panes, 1, Dir::Right, 5.), Some(3));
    }
}
