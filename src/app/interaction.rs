//! Mouse, drag, and context-menu interaction state.

use crate::types::{QueueTab, Track};
use iced::{widget::Id, Point};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrackListKind {
    Queue,
    Active,
    Recent,
}

impl TrackListKind {
    pub fn scrollable_id(self) -> Id {
        match self {
            TrackListKind::Queue => crate::app::ui::QUEUE_LIST_ID,
            TrackListKind::Active => crate::app::ui::TRACK_LIST_ID,
            TrackListKind::Recent => crate::app::ui::QUEUE_RECENT_LIST_ID,
        }
    }

    pub const fn is_interactive(self) -> bool {
        !matches!(self, TrackListKind::Recent)
    }

    pub const fn first_index(self) -> usize {
        match self {
            TrackListKind::Queue => 1,
            _ => 0,
        }
    }

    pub const fn in_queue_panel(self) -> bool {
        matches!(self, TrackListKind::Queue | TrackListKind::Recent)
    }
}

impl From<QueueTab> for TrackListKind {
    fn from(tab: QueueTab) -> Self {
        match tab {
            QueueTab::Queue => TrackListKind::Queue,
            QueueTab::RecentlyPlayed => TrackListKind::Recent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackPos {
    pub index: usize,
    pub list: TrackListKind,
}

impl TrackPos {
    pub const fn new(index: usize, list: TrackListKind) -> Self {
        Self { index, list }
    }
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct ContextMenuState {
    pub pos: TrackPos,
    pub position: (f32, f32),
    pub is_youtube: bool,
    pub is_downloaded: bool,
    pub in_playlist: bool,
    pub track: Track,
    pub target_indices: Vec<usize>,
}

/// Mouse and drag interaction state
#[derive(Debug, Clone, Default)]
pub struct DragState {
    pub cursor_pos: Point,
    pub pressed_track: Option<TrackPos>,
    pub hovered_track: Option<TrackPos>,
    pub drag_origin: Option<Point>,
    pub drag_active: bool,
    pub drag_drop_target: Option<usize>,
    pub drag_target_list: Option<TrackListKind>,
    pub sidebar_hover_playlist: Option<usize>,
}

impl DragState {
    pub(crate) const fn cleanup(&mut self) {
        self.drag_active = false;
        self.drag_origin = None;
        self.pressed_track = None;
        self.drag_drop_target = None;
        self.drag_target_list = None;
        self.sidebar_hover_playlist = None;
    }
}

#[cfg(test)]
mod tests {
    use super::TrackListKind::{Active, Queue, Recent};
    use crate::app::ui::{QUEUE_LIST_ID, QUEUE_RECENT_LIST_ID, TRACK_LIST_ID};

    #[test]
    fn each_list_targets_a_distinct_scrollable() {
        assert_eq!(Queue.scrollable_id(), QUEUE_LIST_ID);
        assert_eq!(Active.scrollable_id(), TRACK_LIST_ID);
        assert_eq!(Recent.scrollable_id(), QUEUE_RECENT_LIST_ID);
        assert_ne!(Queue.scrollable_id(), Recent.scrollable_id());
    }

    #[test]
    fn only_queue_offsets_its_first_row() {
        assert_eq!(Queue.first_index(), 1);
        assert_eq!(Active.first_index(), 0);
        assert_eq!(Recent.first_index(), 0);
    }

    #[test]
    fn recent_is_read_only_and_panel_membership_is_by_tab() {
        assert!(Queue.is_interactive() && Active.is_interactive());
        assert!(!Recent.is_interactive());
        assert!(Queue.in_queue_panel() && Recent.in_queue_panel());
        assert!(!Active.in_queue_panel());
    }
}
