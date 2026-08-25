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

    /// Slot into `DragState::last_focus` for this list.
    pub const fn slot(self) -> usize {
        match self {
            TrackListKind::Queue => 0,
            TrackListKind::Active => 1,
            TrackListKind::Recent => 2,
        }
    }

    pub const fn is_main(self) -> bool {
        matches!(self, TrackListKind::Active)
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
pub struct TrackListSearch {
    pub list: TrackListKind,
    pub query: String,
    pub matches: Vec<usize>,
}

/// Which submenu of the context menu is currently expanded by hover.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SubmenuKind {
    Play,
    Download,
    SongRadio,
    ArtistRadio,
    GoToArtist,
}

impl SubmenuKind {
    /// Message produced by activating the entry for `provider` in this
    /// submenu.
    pub fn entry_message(
        self,
        provider: crate::providers::ProviderId,
        menu: &ContextMenuState,
    ) -> super::message::Message {
        use super::message::Message;
        match self {
            SubmenuKind::Play => Message::ContextMenuPlayViaProvider(provider, menu.pos),
            SubmenuKind::Download => {
                Message::ContextMenuDownloadViaProvider(provider, menu.target_indices.clone())
            }
            SubmenuKind::SongRadio => Message::ContextMenuSongRadioProvider(provider),
            SubmenuKind::ArtistRadio => Message::ContextMenuArtistRadioProvider(provider),
            SubmenuKind::GoToArtist => Message::ContextMenuGoToArtistProvider(provider),
        }
    }

    /// Providers listed in this submenu, in display order.
    pub fn providers(self) -> Vec<crate::providers::ProviderId> {
        use crate::providers::ProviderId;
        match self {
            SubmenuKind::Play => ProviderId::searchable()
                .iter()
                .copied()
                .filter(|p| p.capabilities().stream)
                .collect(),
            SubmenuKind::Download => ProviderId::defaultable()
                .iter()
                .copied()
                .filter(|p| p.capabilities().download)
                .collect(),
            SubmenuKind::SongRadio | SubmenuKind::ArtistRadio => ProviderId::searchable()
                .iter()
                .copied()
                .filter(|p| p.capabilities().radio)
                .collect(),
            SubmenuKind::GoToArtist => ProviderId::searchable().to_vec(),
        }
    }

    /// Cheap capability probe so building the main-menu action list does not
    /// allocate a provider vec just to check that one is non-empty.
    pub fn available(self) -> bool {
        use crate::providers::ProviderId;
        let any = |list: &'static [ProviderId], cap: fn(ProviderId) -> bool| {
            list.iter().copied().any(cap)
        };
        match self {
            SubmenuKind::Play => any(ProviderId::searchable(), |p| p.capabilities().stream),
            SubmenuKind::Download => any(ProviderId::defaultable(), |p| p.capabilities().download),
            SubmenuKind::SongRadio | SubmenuKind::ArtistRadio => {
                any(ProviderId::searchable(), |p| p.capabilities().radio)
            }
            SubmenuKind::GoToArtist => !ProviderId::searchable().is_empty(),
        }
    }
}

/// Actions whose direct activation (clicking the submenu's parent row) needs
/// a default provider resolved from the track and config. A dedicated enum
/// keeps [`Message::ContextMenuDefault`] exhaustive over real cases.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DefaultCtxAction {
    Download,
    SongRadio,
    ArtistRadio,
}

/// A context-menu entry. The menu's contents are derived from this list so
/// the view and keyboard navigation always agree on indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CtxAction {
    Play,
    Edit,
    GoToArtist,
    AddToPlaylist,
    Download,
    SongRadio,
    ArtistRadio,
    RemoveFromQueue,
    RemoveFromPlaylist,
}

impl CtxAction {
    pub fn submenu(self) -> Option<SubmenuKind> {
        match self {
            CtxAction::Play => Some(SubmenuKind::Play),
            CtxAction::Download => Some(SubmenuKind::Download),
            CtxAction::SongRadio => Some(SubmenuKind::SongRadio),
            CtxAction::ArtistRadio => Some(SubmenuKind::ArtistRadio),
            CtxAction::GoToArtist => Some(SubmenuKind::GoToArtist),
            CtxAction::Edit
            | CtxAction::AddToPlaylist
            | CtxAction::RemoveFromQueue
            | CtxAction::RemoveFromPlaylist => None,
        }
    }

    pub fn to_message(self, menu: &ContextMenuState) -> super::message::Message {
        use super::message::Message;
        match self {
            CtxAction::Play => Message::ContextMenuPlayTrack(menu.pos),
            CtxAction::Edit => Message::ContextMenuEditTrack,
            CtxAction::GoToArtist => Message::ContextMenuGoToArtist,
            CtxAction::AddToPlaylist => Message::TogglePicker(menu.target_indices.clone()),
            CtxAction::Download => Message::ContextMenuDefault(DefaultCtxAction::Download),
            CtxAction::SongRadio => Message::ContextMenuDefault(DefaultCtxAction::SongRadio),
            CtxAction::ArtistRadio => Message::ContextMenuDefault(DefaultCtxAction::ArtistRadio),
            CtxAction::RemoveFromQueue => {
                Message::ContextMenuRemoveFromQueue(menu.target_indices.clone())
            }
            CtxAction::RemoveFromPlaylist => {
                Message::ContextMenuRemoveFromPlaylist(menu.target_indices.clone())
            }
        }
    }
}

/// Where keyboard/mouse focus currently sits inside the open context menu.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuFocus {
    /// An entry of the main menu (index into [`ContextMenuState::actions`]).
    Item(usize),
    /// An entry of the open submenu (index into `SubmenuKind::providers`).
    Sub(SubmenuKind, usize),
}

#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct ContextMenuState {
    pub pos: TrackPos,
    pub position: (f32, f32),
    /// Original cursor point; `position` may be flipped away from it, but
    /// every re-measure recomputes the flip relative to this.
    pub cursor: (f32, f32),
    pub in_playlist: bool,
    pub track: Track,
    pub target_indices: Vec<usize>,
    pub hovered: Option<ContextMenuFocus>,
}

impl ContextMenuState {
    /// The visible entries of the main menu, in order. The view renders one
    /// row per entry; keyboard navigation indexes into this list.
    pub fn actions(&self) -> Vec<CtxAction> {
        let mut v = vec![CtxAction::Play, CtxAction::Edit];
        if !self.track.artist.is_empty() {
            v.push(CtxAction::GoToArtist);
        }
        v.push(CtxAction::AddToPlaylist);
        v.push(CtxAction::Download);
        if SubmenuKind::SongRadio.available() {
            v.push(CtxAction::SongRadio);
            v.push(CtxAction::ArtistRadio);
        }
        if self.pos.list == TrackListKind::Queue {
            v.push(CtxAction::RemoveFromQueue);
        } else if self.in_playlist && self.pos.list != TrackListKind::Recent {
            v.push(CtxAction::RemoveFromPlaylist);
        }
        v
    }

    /// The submenu currently visible, derived from the hovered element: a
    /// main-menu parent opens its own submenu; hovering a submenu entry keeps
    /// that submenu open.
    pub fn open_submenu_kind(&self) -> Option<SubmenuKind> {
        match self.hovered? {
            ContextMenuFocus::Item(i) => self.actions().get(i).and_then(|a| a.submenu()),
            ContextMenuFocus::Sub(kind, _) => Some(kind),
        }
    }

    /// Provider used when a submenu-parent action is clicked directly: the
    /// track's source provider when capable of `action`, else the first
    /// capable search provider, else `fallback` (the configured default).
    pub fn default_provider(
        &self,
        action: DefaultCtxAction,
        fallback: crate::providers::ProviderId,
    ) -> crate::providers::ProviderId {
        use crate::providers::ProviderId;
        let capable = |p: ProviderId| match action {
            DefaultCtxAction::Download => p.capabilities().download,
            DefaultCtxAction::SongRadio | DefaultCtxAction::ArtistRadio => p.capabilities().radio,
        };
        if capable(self.track.source) {
            return self.track.source;
        }
        ProviderId::searchable()
            .iter()
            .copied()
            .find(|&p| action != DefaultCtxAction::Download && capable(p))
            .unwrap_or(fallback)
    }

    /// Provider used when "Go to artist" is clicked directly: the track's
    /// source provider when it carries that artist id, else `fallback`.
    pub fn default_go_to_artist_provider(
        &self,
        fallback: crate::providers::ProviderId,
    ) -> crate::providers::ProviderId {
        if self.track.provider_artist_id(self.track.source).is_some() {
            self.track.source
        } else {
            fallback
        }
    }
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
    SearchHistory(usize),
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
    /// Last focused row per list, so returning to a list restores focus.
    pub last_focus: [Option<usize>; 3],
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

    /// Set the hovered target, remembering track rows as the list's last
    /// focused row.
    pub fn set_hovered(&mut self, target: HoverTarget) {
        if let HoverTarget::Track(pos) = &target {
            self.last_focus[pos.list.slot()] = Some(pos.index);
        }
        self.hovered = Some(target);
    }

    /// The last focused row index of `list`, if still meaningful-ish.
    pub fn recall_focus(&self, list: TrackListKind) -> Option<usize> {
        self.last_focus[list.slot()]
    }

    /// Clear a hovered track without disturbing an unrelated card hover.
    pub fn clear_hovered_track(&mut self) {
        if matches!(self.hovered, Some(HoverTarget::Track(_))) {
            self.hovered = None;
        }
    }

    /// The hovered search-history entry index, if any — the keyboard-
    /// navigation focus while the search-history dropdown is open.
    pub fn hovered_search_history(&self) -> Option<usize> {
        match self.hovered {
            Some(HoverTarget::SearchHistory(i)) => Some(i),
            _ => None,
        }
    }

    /// Set the hovered search-history entry (keyboard-navigation focus).
    pub fn set_hovered_search_history(&mut self, index: usize) {
        self.hovered = Some(HoverTarget::SearchHistory(index));
    }

    /// Clear a hovered search-history entry without disturbing another hover.
    pub fn clear_hovered_search_history(&mut self) {
        if matches!(self.hovered, Some(HoverTarget::SearchHistory(_))) {
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
    fn recent_is_read_only() {
        assert!(Queue.is_interactive() && Active.is_interactive());
        assert!(!Recent.is_interactive());
    }
}
