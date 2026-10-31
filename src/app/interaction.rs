//! Mouse, drag, and context-menu interaction state.

use std::time::{Duration, Instant};

use iced::{widget::Id, Point};

use super::{
    dialog::Dialog,
    pane::{is_main_pane, PaneId, MANUAL_SEARCH_PANE_ID, QUEUE_PANE_ID, RECENT_PANE_ID},
};
use crate::{
    data::{
        cache::StreamCache,
        library::{Card, LibraryItem, LibraryKind, LibraryStore},
    },
    types::Track,
};

/// Stable `Id` for a track-list row `Container`, used to capture its measured
/// geometry via the bounds `Operation`. The tag distinguishes lists so ids
/// never collide across the tree; `pane` further scopes `Active` rows so two
/// panes showing a track list never share an id. Cards (artists/albums/
/// playlists) are not track-list rows and intentionally carry no
/// geometry-capturing id.
pub fn row_id(pane: PaneId, index: usize) -> Id {
    let tag = match pane {
        QUEUE_PANE_ID => "queue",
        RECENT_PANE_ID => "recent",
        _ => "active",
    };
    Id::from(format!("row:{tag}:{pane}:{index}"))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TrackPos {
    pub index: usize,
    /// Owning pane for main-list positions; [`QUEUE_PANE_ID`] or
    /// [`RECENT_PANE_ID`] for the global queue panel lists.
    pub pane: PaneId,
}

impl TrackPos {
    pub const fn new(index: usize, pane: PaneId) -> Self {
        Self { index, pane }
    }
}

#[derive(Debug, Clone)]
pub struct TrackListSearch {
    /// Searched list: a main pane id, or [`QUEUE_PANE_ID`] /
    /// [`RECENT_PANE_ID`] for the global panel lists.
    pub pane: PaneId,
    pub query: String,
    /// Ascending track indices, so membership is a binary search.
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
    OpenInBrowser,
    AddToPlaylist,
    Download,
    SongRadio,
    ArtistRadio,
    ClearCache,
    RemoveFromQueue,
    RemoveFromPlaylist,
    RemoveFromRecent,
    AddToTrashbin,
    RemoveFromTrashbin,
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
            | CtxAction::OpenInBrowser
            | CtxAction::AddToQueue
            | CtxAction::AddToPlaylist
            | CtxAction::RemoveFromQueue
            | CtxAction::RemoveFromPlaylist
            | CtxAction::RemoveFromRecent
            | CtxAction::AddToTrashbin
            | CtxAction::RemoveFromTrashbin => None,
        }
    }

    pub fn to_message(self, menu: &ContextMenuState) -> super::message::Message {
        use super::message::Message;
        match self {
            CtxAction::Play => Message::ContextMenuPlayTrack(menu.pos),
            CtxAction::AddToQueue => {
                Message::ContextMenuAddToQueue(menu.pos.pane, menu.target_indices.clone())
            }
            CtxAction::Edit => Message::ContextMenuEditTrack,
            CtxAction::GoToArtist => Message::ContextMenuGoToArtist,
            CtxAction::OpenInBrowser => Message::ContextMenuOpenInBrowser,
            CtxAction::AddToPlaylist => Message::OpenPlaylistAdd(menu.target_indices.clone()),
            CtxAction::Download => Message::ContextMenuDefault(DefaultCtxAction::Download),
            CtxAction::SongRadio => Message::ContextMenuDefault(DefaultCtxAction::SongRadio),
            CtxAction::ArtistRadio => Message::ContextMenuDefault(DefaultCtxAction::ArtistRadio),
            CtxAction::ClearCache => Message::ContextMenuClearCache,
            CtxAction::RemoveFromQueue
            | CtxAction::RemoveFromPlaylist
            | CtxAction::RemoveFromRecent => {
                Message::ContextMenuRemoveFromList(menu.pos.pane, menu.target_indices.clone())
            }
            CtxAction::AddToTrashbin => {
                Message::ContextMenuAddToTrashbin(menu.pos.pane, menu.target_indices.clone())
            }
            CtxAction::RemoveFromTrashbin => {
                Message::ContextMenuRemoveFromTrashbin(menu.pos.pane, menu.target_indices.clone())
            }
        }
    }
}

/// Where keyboard/mouse focus currently sits inside the open context menu.
/// Indices address the menu's own entry list, so both the track menu and the
/// card menu use this.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContextMenuFocus {
    /// An entry of the main menu (index into the menu's `actions()`).
    Item(usize),
    /// An entry of the open submenu (index into `SubmenuKind::providers`).
    Sub(SubmenuKind, usize),
}

/// Where a popup menu sits and which entry has focus. Shared by the track
/// context menu and the card context menu: both anchor at the cursor, both
/// flip back inside the window as their measured size changes, and both drive
/// focus from the same hover/arrow keys.
#[derive(Debug, Clone)]
pub struct MenuAnchor {
    /// Menu top-left in window coordinates (may be flipped away from the
    /// cursor).
    pub position: (f32, f32),
    /// Original cursor point; `position` may be flipped away from it, but
    /// every re-measure recomputes the flip relative to this.
    pub cursor: (f32, f32),
    pub hovered: Option<ContextMenuFocus>,
}

impl MenuAnchor {
    pub fn at(point: iced::Point) -> Self {
        Self {
            position: (point.x, point.y),
            cursor: (point.x, point.y),
            hovered: None,
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

/// A context-menu entry for a track row. The menu's contents are derived from
/// this list so the view and keyboard navigation always agree on indices.
#[derive(Debug, Clone)]
#[allow(clippy::struct_excessive_bools)]
pub struct ContextMenuState {
    pub anchor: MenuAnchor,
    pub pos: TrackPos,
    pub in_playlist: bool,
    pub is_trashed: bool,
    pub track: Track,
    pub target_indices: Vec<usize>,
    /// The dialog the menu was opened from, restored when it closes. Only the
    /// Edit popup's manual search right-clicks from inside a dialog, which a
    /// single-overlay `Dialog` slot would otherwise destroy. Boxed to break
    /// the `Dialog::ContextMenu` cycle.
    pub return_to: Option<Box<Dialog>>,
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
    /// row per entry; keyboard navigation indexes into this list. Trashed
    /// tracks cannot be played, so `Play` is hidden for them; trashed tracks
    /// offer removal from the trashbin instead of adding. `OpenInBrowser` is
    /// shown only when some provider identity on the track addresses a page.
    pub fn actions(&self, cache: &StreamCache) -> Vec<CtxAction> {
        let mut v = Vec::with_capacity(12);
        if !self.is_trashed {
            v.push(CtxAction::Play);
        }
        v.push(CtxAction::AddToQueue);
        // A manual-search result is a transient row inside the Edit popup's
        // own editor; editing it there would replace the editor itself.
        if self.pos.pane != MANUAL_SEARCH_PANE_ID {
            v.push(CtxAction::Edit);
        }
        if !self.track.artist.is_empty() {
            v.push(CtxAction::GoToArtist);
        }
        if self.track.web_url().is_some() {
            v.push(CtxAction::OpenInBrowser);
        }
        v.push(CtxAction::AddToPlaylist);
        if self.is_trashed {
            v.push(CtxAction::RemoveFromTrashbin);
        } else {
            v.push(CtxAction::AddToTrashbin);
        }
        v.push(CtxAction::Download);
        if SubmenuKind::SongRadio.available(&self.track) {
            v.push(CtxAction::SongRadio);
            v.push(CtxAction::ArtistRadio);
        }
        if !self.cached_providers(cache).is_empty() {
            v.push(CtxAction::ClearCache);
        }
        if self.pos.pane == QUEUE_PANE_ID {
            v.push(CtxAction::RemoveFromQueue);
        } else if self.pos.pane == RECENT_PANE_ID {
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
        match self.anchor.hovered? {
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
        let src = self.track.source;
        let src_cap = src.capabilities();
        match action {
            DefaultCtxAction::Download if src_cap.download => src,
            DefaultCtxAction::Download => fallback,
            DefaultCtxAction::SongRadio | DefaultCtxAction::ArtistRadio if src_cap.radio => src,
            DefaultCtxAction::SongRadio | DefaultCtxAction::ArtistRadio => ProviderId::searchable()
                .iter()
                .copied()
                .find(|&p| p.capabilities().radio)
                .unwrap_or(fallback),
        }
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

/// An entry of the card context menu. Like [`CtxAction`], the visible rows are
/// derived from this list so the view and keyboard navigation agree on indices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardAction {
    Open,
    OpenInBrowser,
    SaveAsPlaylist,
    ToggleLibrary,
}

impl CardAction {
    pub fn to_message(self) -> super::message::Message {
        use super::message::Message;
        match self {
            CardAction::Open => Message::CardMenuOpen,
            CardAction::OpenInBrowser => Message::CardMenuOpenInBrowser,
            CardAction::SaveAsPlaylist => Message::CardMenuSaveAsPlaylist,
            CardAction::ToggleLibrary => Message::CardMenuToggleLibrary,
        }
    }
}

/// The open card context menu. Cards (artists, albums, playlists) have a flat
/// menu with no provider submenus or multi-selection: `target` is the exact
/// view the card's click opens, so "Open" lands on the same page and every
/// other entry reads the card's identity straight off it
/// ([`ViewKind::library_item`](crate::app::ViewKind::library_item)) — there is
/// no second copy to fall out of step.
#[derive(Debug, Clone)]
pub struct CardMenuState {
    pub anchor: MenuAnchor,
    /// Pane the card was right-clicked in (sidebar cards use the focused one).
    pub pane: PaneId,
    pub target: super::view_data::ViewKind,
}

impl CardMenuState {
    /// The card this menu is for.
    pub fn card(&self) -> crate::data::library::CardRef<'_> {
        self.target
            .card_ref()
            .expect("a card menu is only opened on an artist/album/playlist view")
    }

    /// Whether this card is already in the library, which decides how
    /// `ToggleLibrary` reads ("Add" vs "Remove").
    pub fn saved(&self, library: &LibraryStore) -> bool {
        let card = self.card();
        library.contains(card.kind, card.id)
    }

    /// The visible entries of the card menu, in order. "Open in browser" needs
    /// a public page, which not every provider advertises for every kind;
    /// "Save as playlist" browses the card, which only albums and playlists
    /// support as a browsable unit (an artist's tracks live on its page).
    pub fn actions(&self) -> Vec<CardAction> {
        let card = self.card();
        let mut v = vec![CardAction::Open];
        if card.web_url().is_some() {
            v.push(CardAction::OpenInBrowser);
        }
        if card.kind != LibraryKind::Artist {
            v.push(CardAction::SaveAsPlaylist);
        }
        v.push(CardAction::ToggleLibrary);
        v
    }
}

/// What a right-clicked card identifies, and the one place that turns it into
/// a message: every card host (search card tab, artist-page shelf, sidebar
/// library row) reports through `RightClickArea::new(_, card, CardTarget::into_message)`
/// so the field list isn't written out per host.
#[derive(Debug, Clone)]
pub struct CardTarget {
    /// The scrollable the card sits in, which shifts the reported cursor
    /// position and has to be undone when anchoring the menu.
    pub host: crate::app::update::operation::ScrollTarget,
    /// Pane the card belongs to (a sidebar row resolves to the focused pane).
    pub pane: PaneId,
    /// The exact view the card's click opens.
    pub target: super::view_data::ViewKind,
    /// The artist-page shelf inside `host`, when the card sits in one (the
    /// shelf scrolls sideways, so it shifts the cursor too).
    pub row: Option<crate::providers::ArtistSectionKind>,
}

impl CardTarget {
    pub fn into_message(self, point: iced::Point) -> super::message::Message {
        super::message::Message::CardRightClicked {
            host: self.host,
            pane: self.pane,
            target: self.target,
            row: self.row,
            point,
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
    Card(Card),
    Playlist(usize),
    SearchHistory(usize),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pressed {
    Track(TrackPos),
    /// A drill-down card. `Some(pane)` when pressed inside a main-view pane,
    /// `None` for sidebar library rows (which target the focused pane).
    Card(Card, Option<PaneId>),
    Playlist(usize),
}

#[derive(Debug, Clone, PartialEq)]
pub struct PressedDrag {
    pub what: Pressed,
    pub origin: Point,
}

/// How long widget hover reports are ignored after keyboard navigation took
/// over the hover. Iced re-evaluates `MouseArea` hover whenever row bounds
/// change, so the scroll that follows keyboard navigation emits enter/exit
/// reports for the stationary cursor; the grace window lets those echoes die
/// out while genuine later mouse movement still reclaims the hover.
pub const KEYBOARD_HOVER_GRACE: Duration = Duration::from_millis(300);

/// Mouse and drag interaction state
#[derive(Debug, Clone, Default)]
pub struct DragState {
    pub cursor_pos: Point,
    pub pressed: Option<PressedDrag>,
    /// Whether `pressed.origin` was captured from a tracked cursor position.
    /// Presses arrive via widget messages that carry no position, so the
    /// first `CursorMoved` after a press re-anchors the origin instead of
    /// measuring the drag threshold against a stale cursor.
    pub origin_fresh: bool,
    pub drag_active: bool,
    pub drop_target: Option<DropTarget>,
    /// Track indices carried by the current drag, resolved at press time:
    /// the whole selection if the pressed track is selected, else just it.
    /// The owning pane (a main pane id, or [`QUEUE_PANE_ID`] /
    /// [`RECENT_PANE_ID`]) is stored alongside because `pressed` is
    /// taken before the drop handler runs.
    pub dragged: Option<(PaneId, Vec<usize>)>,
    pub is_hover_controlled: bool,
    pub hovered: Option<HoverTarget>,
    /// Until when widget hover reports are ignored while keyboard navigation
    /// owns the hover (see [`KEYBOARD_HOVER_GRACE`]).
    pub hover_guard_until: Option<Instant>,
    /// Last focused row per list, so returning to a list restores focus.
    /// The queue/recent/manual-search slots are global; the main-list slot is
    /// remembered per pane instead (see `pane_focus`).
    pub last_focus: [usize; 4],
    /// Last focused row of each pane's main track list.
    pub pane_focus: std::collections::HashMap<PaneId, usize>,
}

impl DragState {
    pub fn stop(&mut self) {
        self.drag_active = false;
        self.pressed = None;
        self.origin_fresh = true;
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
            if is_main_pane(pos.pane) {
                self.pane_focus.insert(pos.pane, pos.index);
            } else {
                self.last_focus[super::pane::pane_slot(pos.pane)] = pos.index;
            }
        }
        self.hovered = Some(target);
    }

    pub fn claim_keyboard_hover(&mut self) {
        self.is_hover_controlled = true;
        self.hover_guard_until = Some(Instant::now() + KEYBOARD_HOVER_GRACE);
    }

    pub fn claim_mouse(&mut self) {
        self.is_hover_controlled = false;
        self.hover_guard_until = None;
    }

    fn mouse_hover_allowed(&self) -> bool {
        if !self.is_hover_controlled {
            return true;
        }
        self.hover_guard_until
            .is_none_or(|until| Instant::now() >= until)
    }

    /// Claim the hover for the mouse when keyboard navigation isn't owning it
    /// (or its grace window has passed). Returns whether the report applies.
    pub fn accept_mouse_hover(&mut self) -> bool {
        let allowed = self.mouse_hover_allowed();
        if allowed {
            self.claim_mouse();
        }
        allowed
    }

    pub fn hover_started(&mut self, target: HoverTarget) {
        if self.accept_mouse_hover() && self.hovered.as_ref() != Some(&target) {
            self.set_hovered(target);
        }
    }

    pub fn hover_ended(&mut self, target: &HoverTarget) {
        if self.accept_mouse_hover() && self.hovered.as_ref() == Some(target) {
            self.hovered = None;
        }
    }

    /// The last focused row index of the list addressed by `pane` (a main
    /// pane id, or [`QUEUE_PANE_ID`] / [`RECENT_PANE_ID`)).
    pub fn recall_focus(&self, pane: PaneId) -> usize {
        if is_main_pane(pane) {
            self.pane_focus.get(&pane).copied().unwrap_or(0)
        } else {
            self.last_focus[super::pane::pane_slot(pane)]
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

    /// Clear a hovered track belonging to `pane`, leaving any other list's
    /// hover alone.
    pub fn clear_hovered_track_in(&mut self, pane: PaneId) {
        if self.hovered_track().is_some_and(|pos| pos.pane == pane) {
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
        matches!(self.hovered, Some(HoverTarget::Card(ref c)) if c.as_ref() == item)
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
    use super::{
        super::pane::{pane_first_index, QUEUE_PANE_ID, RECENT_PANE_ID},
        CardMenuState, MenuAnchor,
    };
    use crate::{
        app::ui::{track_list_id, QUEUE_LIST_ID, QUEUE_RECENT_LIST_ID},
        data::library::{Card, LibraryKind},
    };

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
        assert_eq!(pane_first_index(QUEUE_PANE_ID), 1);
        assert_eq!(pane_first_index(7), 0);
        assert_eq!(pane_first_index(RECENT_PANE_ID), 0);
    }

    #[test]
    fn main_list_focus_is_remembered_per_pane() {
        use super::{DragState, HoverTarget, TrackPos};
        let mut drag = DragState::default();
        drag.set_hovered(HoverTarget::Track(TrackPos::new(7, 0)));
        drag.set_hovered(HoverTarget::Track(TrackPos::new(2, 1)));
        assert_eq!(drag.recall_focus(0), 7);
        assert_eq!(drag.recall_focus(1), 2);
        assert_eq!(drag.recall_focus(9), 0);

        drag.set_hovered(HoverTarget::Track(TrackPos::new(3, QUEUE_PANE_ID)));
        assert_eq!(drag.recall_focus(QUEUE_PANE_ID), 3);

        drag.forget_pane(0);
        assert_eq!(drag.recall_focus(0), 0);
        assert_eq!(drag.recall_focus(1), 2);
    }

    /// A card menu over a library item's view, which is how every host builds
    /// it (`ViewKind::from(&LibraryItem)` is the sidebar/search-card shape).
    fn card_menu(kind: LibraryKind, url: &str) -> CardMenuState {
        use crate::{app::ViewKind, data::library::LibraryItem, providers::ProviderId};
        CardMenuState {
            anchor: MenuAnchor::at(iced::Point::ORIGIN),
            pane: 0,
            target: ViewKind::from(&LibraryItem {
                kind,
                id: "id".into(),
                title: "T".into(),
                thumbnail: String::new(),
                provider: ProviderId::YouTube,
                url: url.into(),
            }),
        }
    }

    #[test]
    fn card_menu_offers_only_what_the_card_supports() {
        use super::CardAction;
        // An album with a page: everything the card supports.
        assert_eq!(
            card_menu(LibraryKind::Album, "https://x.test/a").actions(),
            vec![
                CardAction::Open,
                CardAction::OpenInBrowser,
                CardAction::SaveAsPlaylist,
                CardAction::ToggleLibrary,
            ]
        );
        // Without a page, no browser entry.
        assert_eq!(
            card_menu(LibraryKind::Album, "").actions(),
            vec![
                CardAction::Open,
                CardAction::SaveAsPlaylist,
                CardAction::ToggleLibrary,
            ]
        );
        // Artists are not browsable as a unit, so no playlist entry.
        assert_eq!(
            card_menu(LibraryKind::Artist, "https://x.test/ar").actions(),
            vec![
                CardAction::Open,
                CardAction::OpenInBrowser,
                CardAction::ToggleLibrary
            ]
        );
    }

    #[test]
    fn card_menu_knows_whether_the_card_is_saved() {
        use crate::data::library::LibraryStore;

        let menu = card_menu(LibraryKind::Album, "");
        let mut store = LibraryStore::default();
        assert!(!menu.saved(&store));
        store.add(Card::new(menu.target.library_item().expect("album view")));
        assert!(menu.saved(&store));
    }

    #[test]
    fn keyboard_hover_ignores_scroll_echoes_but_yields_to_mouse() {
        use std::time::{Duration, Instant};

        use super::{DragState, HoverTarget, TrackPos};
        let mut drag = DragState::default();
        let keyboard = HoverTarget::Track(TrackPos::new(2, 0));
        let echo = HoverTarget::Track(TrackPos::new(5, 0));
        drag.claim_keyboard_hover();
        drag.set_hovered(keyboard.clone());
        drag.hover_started(echo.clone());
        assert_eq!(drag.hovered, Some(keyboard.clone()));
        assert!(drag.is_hover_controlled);
        drag.hover_ended(&keyboard);
        assert_eq!(drag.hovered, Some(keyboard.clone()));

        drag.claim_mouse();
        drag.hover_started(echo.clone());
        assert_eq!(drag.hovered, Some(echo.clone()));
        assert!(!drag.is_hover_controlled);

        drag.claim_keyboard_hover();
        drag.hover_guard_until = Some(
            Instant::now()
                .checked_sub(Duration::from_millis(1))
                .unwrap(),
        );
        drag.hover_started(echo.clone());
        assert_eq!(drag.hovered, Some(echo));
        assert!(!drag.is_hover_controlled);
    }
}
