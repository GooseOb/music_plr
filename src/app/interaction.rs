//! Mouse, drag, and context-menu interaction state.

use crate::{
    data::library::LibraryItem,
    types::{QueueTab, Track},
};
use iced::{widget::Id, Point};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackListKind {
    Queue,
    Active,
    Recent,
}

impl From<TrackListKind> for Id {
    fn from(list: TrackListKind) -> Self {
        match list {
            TrackListKind::Queue => crate::app::ui::QUEUE_LIST_ID,
            TrackListKind::Active => crate::app::ui::TRACK_LIST_ID,
            TrackListKind::Recent => crate::app::ui::QUEUE_RECENT_LIST_ID,
        }
    }
}

impl TrackListKind {
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

/// Stable `Id` for a track-list row `Container`, used to capture its measured
/// geometry via the bounds `Operation`. The tag distinguishes lists so ids
/// never collide across the tree. Cards (artists/albums/playlists) are not
/// track-list rows and intentionally carry no geometry-capturing id.
pub fn row_id(list: TrackListKind, index: usize) -> Id {
    let tag = match list {
        TrackListKind::Queue => "queue",
        TrackListKind::Active => "active",
        TrackListKind::Recent => "recent",
    };
    Id::from(format!("row:{tag}:{index}"))
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
pub struct FloatingSearch {
    pub list: TrackListKind,
    pub query: String,
    pub matches: Vec<usize>,
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DropTarget {
    Track(TrackPos),
    Playlist(usize),
    Library(usize),
    PlaylistAdd(usize),
    PlaylistReorder { from: usize, to: usize },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoverTarget {
    Track(TrackPos),
    Card(LibraryItem),
    LibraryCard(LibraryItem),
    Playlist(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pressed {
    Track(TrackPos),
    Card(LibraryItem),
    Playlist(usize),
}

/// Mouse and drag interaction state
#[derive(Debug, Clone, Default)]
pub struct DragState {
    pub cursor_pos: Point,
    pub pressed: Option<Pressed>,
    pub drag_origin: Option<Point>,
    pub drag_active: bool,
    pub drop_target: Option<DropTarget>,
    pub is_hover_controlled: bool,
    pub hovered: Option<HoverTarget>,
}

impl DragState {
    pub fn stop(&mut self) {
        self.drag_active = false;
        self.drag_origin = None;
        self.pressed = None;
        self.drop_target = None;
    }

    pub(crate) fn cleanup(&mut self) {
        self.stop();
        self.hovered = None;
    }

    /// The hovered track, if any — also the keyboard-navigation focus.
    pub fn hovered_track(&self) -> Option<TrackPos> {
        match self.hovered {
            Some(HoverTarget::Track(pos)) => Some(pos),
            _ => None,
        }
    }

    /// Set the hovered track (keyboard-navigation focus).
    pub fn set_hovered_track(&mut self, pos: TrackPos) {
        self.hovered = Some(HoverTarget::Track(pos));
    }

    /// Clear a hovered track without disturbing an unrelated card hover.
    pub fn clear_hovered_track(&mut self) {
        if matches!(self.hovered, Some(HoverTarget::Track(_))) {
            self.hovered = None;
        }
    }

    /// Whether the given search card is the hovered one.
    pub fn is_hovered_card(&self, item: &LibraryItem) -> bool {
        matches!(self.hovered, Some(HoverTarget::Card(ref c)) if c == item)
    }

    /// Whether the given library card is the hovered one.
    pub fn is_hovered_library_card(&self, item: &LibraryItem) -> bool {
        matches!(self.hovered, Some(HoverTarget::LibraryCard(ref c)) if c == item)
    }

    /// Whether a card (vs track) drag is active.
    pub fn is_pressed_card(&self) -> bool {
        matches!(self.pressed, Some(Pressed::Card(_)))
    }

    /// The hovered playlist row index, if any.
    pub fn hovered_playlist(&self) -> Option<usize> {
        match self.hovered {
            Some(HoverTarget::Playlist(i)) => Some(i),
            _ => None,
        }
    }

    pub fn cursor_interaction(&self) -> Option<iced::mouse::Interaction> {
        if self.drag_active && self.pressed.is_some() {
            Some(iced::mouse::Interaction::Grabbing)
        } else {
            None
        }
    }

    pub fn clickable_cursor_interaction(&self) -> iced::mouse::Interaction {
        self.cursor_interaction()
            .unwrap_or(iced::mouse::Interaction::Pointer)
    }
}

#[cfg(test)]
mod tests {
    use super::TrackListKind::{Active, Queue, Recent};
    use crate::app::ui::{QUEUE_LIST_ID, QUEUE_RECENT_LIST_ID, TRACK_LIST_ID};
    use iced::widget::Id;

    #[test]
    fn each_list_targets_a_distinct_scrollable() {
        assert_eq!(Id::from(Queue), QUEUE_LIST_ID);
        assert_eq!(Id::from(Active), TRACK_LIST_ID);
        assert_eq!(Id::from(Recent), QUEUE_RECENT_LIST_ID);
        assert_ne!(Id::from(Queue), Id::from(Recent));
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
