use serde::{Deserialize, Serialize};

use super::{lyrics_state::LyricsState, view_data::ViewData};
use crate::providers::{ProviderId, SearchScope};

pub type PaneId = u64;

pub const MAX_PANES: usize = 4;

#[derive(Clone)]
pub struct Pane {
    pub id: PaneId,
    pub nav_history: Vec<ViewData>,
    pub nav_history_pos: usize,
    pub search_query: String,
    pub search_scope: SearchScope,
    pub search_provider: ProviderId,
    pub show_search_history: bool,
    pub last_filtered_history: Vec<String>,
    pub lyrics: Option<LyricsState>,
}

impl Pane {
    pub fn new(id: PaneId) -> Self {
        Self {
            id,
            nav_history: vec![ViewData::default()],
            nav_history_pos: 0,
            search_query: String::new(),
            search_scope: SearchScope::Songs,
            search_provider: if ProviderId::YouTube.capabilities().search {
                ProviderId::YouTube
            } else {
                ProviderId::searchable()
                    .iter()
                    .copied()
                    .find(|p| p.capabilities().search)
                    .unwrap_or(ProviderId::SoundCloud)
            },
            show_search_history: false,
            last_filtered_history: Vec::new(),
            lyrics: None,
        }
    }

    pub fn view_data(&self) -> &ViewData {
        &self.nav_history[self.nav_history_pos]
    }

    pub fn view_data_mut(&mut self) -> &mut ViewData {
        &mut self.nav_history[self.nav_history_pos]
    }

    pub const fn can_navigate_back(&self) -> bool {
        self.nav_history_pos > 0
    }

    pub const fn can_navigate_forward(&self) -> bool {
        self.nav_history_pos + 1 < self.nav_history.len()
    }

    pub fn from_data(data: PaneData) -> Self {
        let mut pane = Self {
            id: data.id,
            nav_history: vec![ViewData::default()],
            nav_history_pos: 0,
            search_query: String::new(),
            search_scope: SearchScope::Songs,
            search_provider: ProviderId::YouTube,
            show_search_history: false,
            last_filtered_history: Vec::new(),
            lyrics: None,
        };
        if !data.nav_history.is_empty() {
            pane.nav_history = data.nav_history;
            pane.nav_history_pos = data.nav_history_pos.min(pane.nav_history.len() - 1);
        }
        pane.search_query = data.search_query;
        pane.search_scope = data.search_scope;
        pane.search_provider = data.search_provider;
        pane
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SplitNode {
    Leaf(PaneId),
    Row {
        first: Box<SplitNode>,
        second: Box<SplitNode>,
    },
    Column {
        first: Box<SplitNode>,
        second: Box<SplitNode>,
    },
}

/// Split direction for a new pane. `Horizontal` places the new pane to the
/// right (side by side); `Vertical` stacks it below.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SplitDir {
    Horizontal,
    Vertical,
}

impl SplitNode {
    pub fn leaves(&self, out: &mut Vec<PaneId>) {
        match self {
            SplitNode::Leaf(id) => out.push(*id),
            SplitNode::Row { first, second, .. } | SplitNode::Column { first, second, .. } => {
                first.leaves(out);
                second.leaves(out);
            }
        }
    }

    pub fn leaf_count(&self) -> usize {
        match self {
            SplitNode::Leaf(_) => 1,
            SplitNode::Row { first, second, .. } | SplitNode::Column { first, second, .. } => {
                first.leaf_count() + second.leaf_count()
            }
        }
    }

    pub fn contains(&self, id: PaneId) -> bool {
        match self {
            SplitNode::Leaf(leaf) => *leaf == id,
            SplitNode::Row { first, second, .. } | SplitNode::Column { first, second, .. } => {
                first.contains(id) || second.contains(id)
            }
        }
    }

    fn replace_leaf(&mut self, id: PaneId, node: SplitNode) -> bool {
        match self {
            SplitNode::Leaf(leaf) if *leaf == id => {
                *self = node;
                true
            }
            SplitNode::Leaf(_) => false,
            SplitNode::Row { first, second, .. } | SplitNode::Column { first, second, .. } => {
                first.replace_leaf(id, node.clone()) || second.replace_leaf(id, node)
            }
        }
    }

    pub fn split_leaf(&mut self, id: PaneId, new_id: PaneId, dir: SplitDir) -> bool {
        let node = match dir {
            SplitDir::Vertical => SplitNode::Column {
                first: Box::new(SplitNode::Leaf(id)),
                second: Box::new(SplitNode::Leaf(new_id)),
            },
            SplitDir::Horizontal => SplitNode::Row {
                first: Box::new(SplitNode::Leaf(id)),
                second: Box::new(SplitNode::Leaf(new_id)),
            },
        };
        self.replace_leaf(id, node)
    }

    pub fn remove_leaf(&mut self, id: PaneId) -> Option<PaneId> {
        match self {
            SplitNode::Leaf(_) => None,
            SplitNode::Row { first, second, .. } | SplitNode::Column { first, second, .. } => {
                if matches!(first.as_ref(), SplitNode::Leaf(leaf) if *leaf == id) {
                    let survivor = second.first_leaf();
                    *self = (**second).clone();
                    Some(survivor)
                } else if matches!(second.as_ref(), SplitNode::Leaf(leaf) if *leaf == id) {
                    let survivor = first.first_leaf();
                    *self = (**first).clone();
                    Some(survivor)
                } else {
                    first.remove_leaf(id).or_else(|| second.remove_leaf(id))
                }
            }
        }
    }

    fn first_leaf(&self) -> PaneId {
        match self {
            SplitNode::Leaf(id) => *id,
            SplitNode::Row { first, .. } | SplitNode::Column { first, .. } => first.first_leaf(),
        }
    }

    /// The nearest pane adjacent to `focused` in `dir`, if any. Traces the
    /// full root-to-leaf path, crosses at the deepest split along the
    /// movement axis whose near side holds the focus, then descends into the
    /// sibling subtree — replaying the focused pane's perpendicular choices
    /// depth-aligned to hold the relative position. `None` at the edge.
    pub fn neighbor(&self, focused: PaneId, dir: PaneDir) -> Option<PaneId> {
        let path = Self::path_to(self, focused)?;
        let horizontal = dir.is_horizontal();
        let from_first = matches!(dir, PaneDir::Right | PaneDir::Down);
        let crossing = path.iter().rposition(|(node, went_first)| {
            let is_row = matches!(node, SplitNode::Row { .. });
            is_row == horizontal && *went_first == from_first
        })?;
        let (node, went_first) = path[crossing];
        let sibling = match node {
            SplitNode::Row { first, second, .. } | SplitNode::Column { first, second, .. } => {
                if went_first {
                    second
                } else {
                    first
                }
            }
            SplitNode::Leaf(_) => return None,
        };
        Some(Self::descend(sibling, dir, &path, crossing + 1))
    }

    /// The extreme pane opposite the direction of travel, for wrapping focus
    /// past the edge (like track navigation wraps at list ends). Stays in
    /// the focused pane's column/row by replaying its recorded path
    /// depth-aligned at perpendicular splits.
    pub fn wrap_edge(&self, focused: PaneId, dir: PaneDir) -> PaneId {
        match Self::path_to(self, focused) {
            Some(path) => Self::descend(self, dir, &path, 0),
            None => self.first_leaf(),
        }
    }

    /// Root-to-leaf trail of `(split, went_first)` for `target`, or `None`
    /// when the pane isn't in the tree.
    fn path_to(node: &SplitNode, target: PaneId) -> Option<Vec<(&SplitNode, bool)>> {
        let mut trail = Vec::new();
        if !Self::trace(node, target, &mut trail) {
            return None;
        }
        trail.reverse();
        Some(trail)
    }

    fn trace<'a>(
        node: &'a SplitNode,
        target: PaneId,
        trail: &mut Vec<(&'a SplitNode, bool)>,
    ) -> bool {
        match node {
            SplitNode::Leaf(id) => *id == target,
            SplitNode::Row { first, second, .. } | SplitNode::Column { first, second, .. } => {
                if Self::trace(first, target, trail) {
                    trail.push((node, true));
                    true
                } else if Self::trace(second, target, trail) {
                    trail.push((node, false));
                    true
                } else {
                    false
                }
            }
        }
    }

    /// Descend `node` (at `depth` in the recorded `path`) toward the edge in
    /// `dir`: splits along the movement axis take the adjacent side, splits
    /// across it replay the focused pane's recorded choice at that depth
    /// (defaulting to the first child past the path's end) to hold the
    /// relative row/column.
    fn descend(
        node: &SplitNode,
        dir: PaneDir,
        path: &[(&SplitNode, bool)],
        depth: usize,
    ) -> PaneId {
        match node {
            SplitNode::Leaf(id) => *id,
            SplitNode::Row { first, second, .. } | SplitNode::Column { first, second, .. } => {
                let is_row = matches!(node, SplitNode::Row { .. });
                if is_row == dir.is_horizontal() {
                    let side = match dir {
                        PaneDir::Right | PaneDir::Down => first,
                        _ => second,
                    };
                    Self::descend(side, dir, path, depth + 1)
                } else {
                    let first_side = path.get(depth).is_none_or(|(_, went_first)| *went_first);
                    let next = if first_side { first } else { second };
                    Self::descend(next, dir, path, depth + 1)
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaneDir {
    Left,
    Right,
    Up,
    Down,
}

impl PaneDir {
    const fn is_horizontal(self) -> bool {
        matches!(self, PaneDir::Left | PaneDir::Right)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PaneData {
    pub id: PaneId,
    pub nav_history: Vec<ViewData>,
    pub nav_history_pos: usize,
    pub search_query: String,
    pub search_scope: SearchScope,
    pub search_provider: ProviderId,
}

impl From<&Pane> for PaneData {
    fn from(pane: &Pane) -> Self {
        Self {
            id: pane.id,
            nav_history: pane.nav_history.clone(),
            nav_history_pos: pane.nav_history_pos,
            search_query: pane.search_query.clone(),
            search_scope: pane.search_scope,
            search_provider: pane.search_provider,
        }
    }
}

impl PaneData {
    pub fn into_pane(self) -> Pane {
        Pane::from_data(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_and_remove_leaf() {
        let mut root = SplitNode::Leaf(0);
        assert!(root.split_leaf(0, 1, SplitDir::Horizontal));
        assert_eq!(root.leaf_count(), 2);
        assert!(root.contains(1));

        assert!(root.split_leaf(1, 2, SplitDir::Vertical));
        assert_eq!(root.leaf_count(), 3);

        let survivor = root.remove_leaf(2);
        assert_eq!(root.leaf_count(), 2);
        assert!(survivor.is_some());

        let survivor = root.remove_leaf(0);
        assert_eq!(root.leaf_count(), 1);
        assert_eq!(survivor, Some(1));
        assert!(root.contains(1));

        assert!(root.remove_leaf(1).is_none());
    }

    fn row(a: PaneId, b: PaneId) -> SplitNode {
        SplitNode::Row {
            first: Box::new(SplitNode::Leaf(a)),
            second: Box::new(SplitNode::Leaf(b)),
        }
    }

    fn column(a: PaneId, b: PaneId) -> SplitNode {
        SplitNode::Column {
            first: Box::new(SplitNode::Leaf(a)),
            second: Box::new(SplitNode::Leaf(b)),
        }
    }

    #[test]
    fn neighbor_moves_across_splits() {
        let root = row(0, 1);
        assert_eq!(root.neighbor(0, PaneDir::Right), Some(1));
        assert_eq!(root.neighbor(1, PaneDir::Left), Some(0));
        assert_eq!(root.neighbor(0, PaneDir::Left), None);
        assert_eq!(root.neighbor(1, PaneDir::Right), None);
        assert_eq!(root.neighbor(0, PaneDir::Up), None);
        assert_eq!(root.neighbor(0, PaneDir::Down), None);

        let root = column(0, 1);
        assert_eq!(root.neighbor(0, PaneDir::Down), Some(1));
        assert_eq!(root.neighbor(1, PaneDir::Up), Some(0));
        assert_eq!(root.neighbor(0, PaneDir::Up), None);
        assert_eq!(root.neighbor(0, PaneDir::Right), None);
    }

    #[test]
    fn neighbor_climbs_past_perpendicular_splits() {
        // 0 | (1 over 2): moving right from 1 must cross the outer row.
        let root = SplitNode::Row {
            first: Box::new(SplitNode::Leaf(0)),
            second: Box::new(column(1, 2)),
        };
        assert_eq!(root.neighbor(1, PaneDir::Left), Some(0));
        assert_eq!(root.neighbor(2, PaneDir::Left), Some(0));
        assert_eq!(root.neighbor(0, PaneDir::Right), Some(1));
        assert_eq!(root.neighbor(1, PaneDir::Down), Some(2));
        assert_eq!(root.neighbor(2, PaneDir::Up), Some(1));
        assert_eq!(root.neighbor(0, PaneDir::Down), None);
    }

    #[test]
    fn wrap_edge_stays_in_column() {
        let root = row(0, 1);
        assert_eq!(root.wrap_edge(0, PaneDir::Right), 0);
        assert_eq!(root.wrap_edge(1, PaneDir::Left), 1);
        assert_eq!(root.wrap_edge(0, PaneDir::Down), 0);
        assert_eq!(root.wrap_edge(1, PaneDir::Up), 1);

        let root = column(0, 1);
        assert_eq!(root.wrap_edge(0, PaneDir::Down), 0);
        assert_eq!(root.wrap_edge(1, PaneDir::Up), 1);
        assert_eq!(root.wrap_edge(0, PaneDir::Right), 0);
        assert_eq!(root.wrap_edge(1, PaneDir::Right), 1);

        // 0 | (1 over 2): vertical wraps hold the column.
        let root = SplitNode::Row {
            first: Box::new(SplitNode::Leaf(0)),
            second: Box::new(column(1, 2)),
        };
        assert_eq!(root.wrap_edge(2, PaneDir::Down), 1);
        assert_eq!(root.wrap_edge(1, PaneDir::Up), 2);
        assert_eq!(root.wrap_edge(0, PaneDir::Down), 0);
        assert_eq!(root.wrap_edge(1, PaneDir::Right), 0);
        assert_eq!(root.wrap_edge(0, PaneDir::Left), 1);

        // (0 over 1) | (2 over 3): wraps hold row and column.
        let root = SplitNode::Row {
            first: Box::new(column(0, 1)),
            second: Box::new(column(2, 3)),
        };
        assert_eq!(root.wrap_edge(3, PaneDir::Down), 2);
        assert_eq!(root.wrap_edge(0, PaneDir::Up), 1);
        assert_eq!(root.wrap_edge(3, PaneDir::Right), 1);
        assert_eq!(root.wrap_edge(0, PaneDir::Left), 2);
    }

    #[test]
    fn neighbor_holds_relative_position() {
        // (0 over 1) | (2 over 3): moving right from 1 lands on 3.
        let root = SplitNode::Row {
            first: Box::new(column(0, 1)),
            second: Box::new(column(2, 3)),
        };
        assert_eq!(root.neighbor(1, PaneDir::Right), Some(3));
        assert_eq!(root.neighbor(0, PaneDir::Right), Some(2));
        assert_eq!(root.neighbor(3, PaneDir::Left), Some(1));
        assert_eq!(root.neighbor(2, PaneDir::Left), Some(0));
    }
}
