//! Mouse, drag, and context-menu interaction state.

use iced::{widget::Id, Point};

use super::pane::PaneId;
use crate::{
    data::{cache::StreamCache, library::LibraryItem},
    types::{QueueTab, Track},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum TrackListKind {
    Queue,
    Active,
    Recent,
}

impl TrackListKind {
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
/// never collide across the tree; `pane` further scopes `Active` rows so two
/// panes showing a track list never share an id. Cards (artists/albums/
/// playlists) are not track-list rows and intentionally carry no
/// geometry-capturing id.
pub fn row_id(list: TrackListKind, index: usize, pane: PaneId) -> Id {
    let tag = match list {
        TrackListKind::Queue => "queue",
        TrackListKind::Active => "active",
        TrackListKind::Recent => "recent",
    };
    Id::from(format!("row:{tag}:{pane}:{index}"))
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
    /// Owning pane for `Active` positions; ignored for `Queue`/`Recent`,
    /// which live in the global queue panel.
    pub pane: PaneId,
}

impl TrackPos {
    /// `pane` is only meaningful for the main (`Active`) list; `Queue` and
    /// `Recent` live in the global panel, so their pane is normalized to `0`
    /// and ignored by equality-sensitive uses (e.g. double-click detection).
    pub const fn new(index: usize, list: TrackListKind, pane: PaneId) -> Self {
        if list.is_main() {
            Self { index, list, pane }
        } else {
            Self {
                index,
                list,
                pane: 0,
            }
        }
    }
}

#[derive(Debug, Clone)]
pub struct TrackListSearch {
    pub list: TrackListKind,
    /// Owning pane for `Active` searches; ignored for `Queue`/`Recent`,
    /// which live in the global queue panel.
    pub pane: PaneId,
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
    ClearCache,
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
            SubmenuKind::Download => Message::ContextMenuDownloadViaProvider(provider),
            SubmenuKind::SongRadio => Message::ContextMenuSongRadioProvider(provider),
            SubmenuKind::ArtistRadio => Message::ContextMenuArtistRadioProvider(provider),
            SubmenuKind::GoToArtist => Message::ContextMenuGoToArtistProvider(provider),
            SubmenuKind::ClearCache => Message::ContextMenuClearCacheProvider(provider),
        }
    }

    /// Providers listed in this submenu, in display order.
    pub fn providers(self, track: &Track) -> Vec<crate::providers::ProviderId> {
        use crate::providers::ProviderId;
        match self {
            SubmenuKind::Play => {
                let mut v: Vec<ProviderId> = ProviderId::searchable()
                    .iter()
                    .copied()
                    .filter(|p| p.capabilities().stream)
                    .collect();
                // A local file (imported or downloaded) is playable directly,
                // so it appears in the "Play" submenu alongside the stream
                // providers when one is available.
                if track.local_path().is_some() {
                    v.push(ProviderId::Local);
                }
                v
            }
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
            SubmenuKind::GoToArtist => ProviderId::searchable()
                .iter()
                .copied()
                .filter(|p| p.capabilities().search)
                .collect(),
            SubmenuKind::ClearCache => track
                .providers
                .keys()
                .copied()
                .filter(|p| *p != ProviderId::Local)
                .collect(),
        }
    }

    /// Cheap capability probe so building the main-menu action list does not
    /// allocate a provider vec just to check that one is non-empty.
    /// Takes the track because `Play` also covers a local file with no
    /// stream-capable provider.
    pub fn available(self, track: &Track) -> bool {
        use crate::providers::ProviderId;
        let any = |list: &'static [ProviderId], cap: fn(ProviderId) -> bool| {
            list.iter().copied().any(cap)
        };
        match self {
            SubmenuKind::Play => {
                track.local_path().is_some()
                    || any(ProviderId::searchable(), |p| p.capabilities().stream)
            }
            SubmenuKind::Download => any(ProviderId::defaultable(), |p| p.capabilities().download),
            SubmenuKind::SongRadio | SubmenuKind::ArtistRadio => {
                any(ProviderId::searchable(), |p| p.capabilities().radio)
            }
            SubmenuKind::GoToArtist => any(ProviderId::searchable(), |p| p.capabilities().search),
            SubmenuKind::ClearCache => track.providers.keys().any(|p| *p != ProviderId::Local),
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
    AddToQueue,
    Edit,
    GoToArtist,
    AddToPlaylist,
    Download,
    SongRadio,
    ArtistRadio,
    ClearCache,
    RemoveFromQueue,
    RemoveFromPlaylist,
    RemoveFromRecent,
}

impl CtxAction {
    pub fn submenu(self) -> Option<SubmenuKind> {
        match self {
            CtxAction::Play => Some(SubmenuKind::Play),
            CtxAction::Download => Some(SubmenuKind::Download),
            CtxAction::SongRadio => Some(SubmenuKind::SongRadio),
            CtxAction::ArtistRadio => Some(SubmenuKind::ArtistRadio),
            CtxAction::GoToArtist => Some(SubmenuKind::GoToArtist),
            CtxAction::ClearCache => Some(SubmenuKind::ClearCache),
            CtxAction::Edit
            | CtxAction::AddToQueue
            | CtxAction::AddToPlaylist
            | CtxAction::RemoveFromQueue
            | CtxAction::RemoveFromPlaylist
            | CtxAction::RemoveFromRecent => None,
        }
    }

    pub fn to_message(self, menu: &ContextMenuState) -> super::message::Message {
        use super::message::Message;
        match self {
            CtxAction::Play => Message::ContextMenuPlayTrack(menu.pos),
            CtxAction::AddToQueue => {
                Message::ContextMenuAddToQueue(menu.pos.list, menu.target_indices.clone())
            }
            CtxAction::Edit => Message::ContextMenuEditTrack,
            CtxAction::GoToArtist => Message::ContextMenuGoToArtist,
            CtxAction::AddToPlaylist => Message::TogglePicker(menu.target_indices.clone()),
            CtxAction::Download => Message::ContextMenuDefault(DefaultCtxAction::Download),
            CtxAction::SongRadio => Message::ContextMenuDefault(DefaultCtxAction::SongRadio),
            CtxAction::ArtistRadio => Message::ContextMenuDefault(DefaultCtxAction::ArtistRadio),
            CtxAction::ClearCache => Message::ContextMenuClearCache,
            CtxAction::RemoveFromQueue
            | CtxAction::RemoveFromPlaylist
            | CtxAction::RemoveFromRecent => {
                Message::ContextMenuRemoveFromList(menu.pos.list, menu.target_indices.clone())
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
    /// Providers of [`SubmenuKind::ClearCache`]: only those the track carries
    /// an id for that actually have a stream-cache entry.
    pub fn cached_providers(&self, cache: &StreamCache) -> Vec<crate::providers::ProviderId> {
        cache.cached_providers_for(&self.track)
    }

    /// Providers listed in `kind`'s submenu for this menu. `ClearCache` is
    /// filtered down to the cached providers; every other submenu lists its
    /// static candidate set.
    pub fn submenu_providers(
        &self,
        kind: SubmenuKind,
        cache: &StreamCache,
    ) -> Vec<crate::providers::ProviderId> {
        match kind {
            SubmenuKind::ClearCache => self.cached_providers(cache),
            _ => kind.providers(&self.track),
        }
    }

    /// The visible entries of the main menu, in order. The view renders one
    /// row per entry; keyboard navigation indexes into this list.
    pub fn actions(&self, cache: &StreamCache) -> Vec<CtxAction> {
        let mut v = vec![CtxAction::Play, CtxAction::AddToQueue, CtxAction::Edit];
        if !self.track.artist.is_empty() {
            v.push(CtxAction::GoToArtist);
        }
        v.push(CtxAction::AddToPlaylist);
        v.push(CtxAction::Download);
        if SubmenuKind::SongRadio.available(&self.track) {
            v.push(CtxAction::SongRadio);
            v.push(CtxAction::ArtistRadio);
        }
        if !self.cached_providers(cache).is_empty() {
            v.push(CtxAction::ClearCache);
        }
        if self.pos.list == TrackListKind::Queue {
            v.push(CtxAction::RemoveFromQueue);
        } else if self.pos.list == TrackListKind::Recent {
            v.push(CtxAction::RemoveFromRecent);
        } else if self.in_playlist {
            v.push(CtxAction::RemoveFromPlaylist);
        }
        v
    }

    /// The submenu currently visible, derived from the hovered element: a
    /// main-menu parent opens its own submenu; hovering a submenu entry keeps
    /// that submenu open.
    pub fn open_submenu_kind(&self, cache: &StreamCache) -> Option<SubmenuKind> {
        match self.hovered? {
            ContextMenuFocus::Item(i) => self.actions(cache).get(i).and_then(|a| a.submenu()),
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

    /// Recompute the flip from the original cursor point using the latest
    /// measurement. Returns whether the position moved. A panel flush with
    /// the window edge counts as overflow (its measurement was clipped).
    pub fn flip_position(&mut self, panel: iced::Rectangle, window: iced::Size) -> bool {
        const EDGE_EPSILON: f32 = 1.0;
        let (cx, cy) = self.cursor;
        let nx = if cx + panel.width > window.width - EDGE_EPSILON {
            (cx - panel.width).max(0.0)
        } else {
            cx
        };
        let ny = if cy + panel.height > window.height - EDGE_EPSILON {
            (cy - panel.height).max(0.0)
        } else {
            cy
        };
        let moved = (nx, ny) != self.position;
        self.position = (nx, ny);
        moved
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
    /// A drill-down card. `Some(pane)` when pressed inside a main-view pane,
    /// `None` for sidebar library rows (which target the focused pane).
    Card(LibraryItem, Option<PaneId>),
    Playlist(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PressedDrag {
    pub what: Pressed,
    pub origin: Point,
}

/// Mouse and drag interaction state
#[derive(Debug, Clone, Default)]
pub struct DragState {
    pub cursor_pos: Point,
    pub pressed: Option<PressedDrag>,
    pub drag_active: bool,
    pub drop_target: Option<DropTarget>,
    /// Track indices carried by the current drag, resolved at press time:
    /// the whole selection if the pressed track is selected, else just it.
    /// The owning pane and list are stored alongside because `pressed` is
    /// taken before the drop handler runs.
    pub dragged: Option<(PaneId, TrackListKind, Vec<usize>)>,
    pub is_hover_controlled: bool,
    pub hovered: Option<HoverTarget>,
    /// Last focused row per list, so returning to a list restores focus.
    /// Queue/Recent live in the global panel; the main (`Active`) list is
    /// remembered per pane instead (see `pane_focus`).
    pub last_focus: [usize; 3],
    /// Last focused row of each pane's main track list.
    pub pane_focus: std::collections::HashMap<PaneId, usize>,
}

impl DragState {
    pub fn stop(&mut self) {
        self.drag_active = false;
        self.pressed = None;
        self.drop_target = None;
        self.dragged = None;
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
    /// focused row (per pane for the main list).
    pub fn set_hovered(&mut self, target: HoverTarget) {
        if let HoverTarget::Track(pos) = &target {
            if pos.list.is_main() {
                self.pane_focus.insert(pos.pane, pos.index);
            } else {
                self.last_focus[pos.list.slot()] = pos.index;
            }
        }
        self.hovered = Some(target);
    }

    /// The last focused row index of `list` in `pane`, if still
    /// meaningful-ish. Queue/Recent ignore the pane.
    pub fn recall_focus(&self, pane: PaneId, list: TrackListKind) -> usize {
        if list.is_main() {
            self.pane_focus.get(&pane).copied().unwrap_or(0)
        } else {
            self.last_focus[list.slot()]
        }
    }

    pub fn forget_pane(&mut self, pane: PaneId) {
        self.pane_focus.remove(&pane);
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
        matches!(
            self.pressed,
            Some(PressedDrag {
                what: Pressed::Card(_, _),
                ..
            })
        )
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
    use crate::app::ui::{track_list_id, QUEUE_LIST_ID, QUEUE_RECENT_LIST_ID};

    #[test]
    fn each_list_targets_a_distinct_scrollable() {
        assert_eq!(track_list_id(7), track_list_id(7));
        assert_ne!(track_list_id(7), track_list_id(8));
        assert_ne!(track_list_id(7), QUEUE_LIST_ID);
        assert_ne!(track_list_id(7), QUEUE_RECENT_LIST_ID);
        assert_ne!(QUEUE_LIST_ID, QUEUE_RECENT_LIST_ID);
    }

    #[test]
    fn only_queue_offsets_its_first_row() {
        assert_eq!(Queue.first_index(), 1);
        assert_eq!(Active.first_index(), 0);
        assert_eq!(Recent.first_index(), 0);
    }

    #[test]
    fn main_list_focus_is_remembered_per_pane() {
        use super::{DragState, HoverTarget, TrackPos};
        let mut drag = DragState::default();
        drag.set_hovered(HoverTarget::Track(TrackPos::new(7, Active, 0)));
        drag.set_hovered(HoverTarget::Track(TrackPos::new(2, Active, 1)));
        assert_eq!(drag.recall_focus(0, Active), 7);
        assert_eq!(drag.recall_focus(1, Active), 2);
        assert_eq!(drag.recall_focus(9, Active), 0);

        drag.set_hovered(HoverTarget::Track(TrackPos::new(3, Queue, 0)));
        assert_eq!(drag.recall_focus(0, Queue), 3);
        assert_eq!(drag.recall_focus(1, Queue), 3);

        drag.forget_pane(0);
        assert_eq!(drag.recall_focus(0, Active), 0);
        assert_eq!(drag.recall_focus(1, Active), 2);
    }
}
