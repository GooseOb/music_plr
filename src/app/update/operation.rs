//! A custom widget `Operation` that captures scrollable geometry during the
//! layout/operate pass — without relying on `on_scroll` events.
//!
//! `on_scroll` only fires on actual scroll interaction, so it never reports
//! geometry on first layout or when content fits (no scrollbar). The
//! `Operation::scrollable` callback, by contrast, is visited for every
//! scrollable on every operate pass and hands us the viewport `bounds`, the
//! content `bounds` (total height, for max-scroll math), and the current
//! `translation` — even when the list doesn't overflow.
//!
//! For drop-target hit-testing we also need each *row's* measured geometry,
//! not a hard-coded row height. `Operation::container` is visited for every
//! `Container` tagged with an `Id`; we set `current` to the list whose
//! scrollable is being visited and record each row `Container`'s absolute
//! `bounds` into that list's `ListGeometry.rows`. Because a scrollable's rows
//! are visited contiguously right after its own `scrollable` callback, the
//! `current` flag is correctly scoped — and the only id'd `Container`s in the
//! tree are list rows (sidebar playlist, library, and search-history rows), so
//! nothing else is captured. The track/queue/recent lists are virtualized and
//! uniform-height, so they use `ROW_HEIGHT` for drop math instead and don't
//! need `rows`.
//! The global search `text_input` (id `SEARCH_INPUT_ID`) is captured via the
//! `text_input` callback so its bounds can hit-test the search-history
//! dropdown.
//!
//! The search-history dropdown rows are captured by a *separate* operation,
//! `CaptureSearchHistoryRows`, so they can be refreshed on every search-input
//! message without re-walking the whole widget tree (sidebar/library/queue/
//! track/recent + search input). See that struct for details.

use std::collections::HashMap;

use iced::{widget::Id, Rectangle, Task};
use iced_core::widget::operation::{Operation, Outcome, Scrollable};

use crate::{
    app::{
        pane::{PaneId, MANUAL_SEARCH_PANE_ID, QUEUE_PANE_ID, RECENT_PANE_ID},
        ui::{
            card_list_id, card_row_id, search_history_list_id, track_list_id,
            MANUAL_SEARCH_LIST_ID, QUEUE_LIST_ID, QUEUE_RECENT_LIST_ID,
        },
        Message,
    },
    theme,
};

#[derive(Debug, Clone, Default)]
pub struct ListGeometry {
    pub bounds: Rectangle,
    pub translation_y: f32,
    pub rows: Vec<Rectangle>,
}

pub const SIDEBAR_LIST_ID: Id = Id::new("sidebar_playlist_list");
pub const LIBRARY_LIST_ID: Id = Id::new("sidebar_library_list");

/// Which scrollable a scroll belongs to. The inverse of [`CaptureBounds`]'s
/// id classification, so every scrollable reports its offset against the same
/// key the capture pass files it under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScrollTarget {
    /// A track list: a pane's own, or the global queue/recent panel (whose
    /// virtual `PaneId` is not a real pane).
    Tracks(PaneId),
    /// A pane's card host: its search card tab or artist page.
    Cards(PaneId),
    /// One shelf of an artist page, scrolled sideways.
    CardRow(PaneId, crate::providers::ArtistSectionKind),
    /// The sidebar's saved-library list.
    Library,
}

/// The slot an id'd scrollable fills, i.e. the reverse of the id functions in
/// `ui` (`track_list_id`, `card_list_id`, `card_row_id`). Classifying an id
/// once per scrollable keeps the capture pass from re-deriving every id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Slot {
    Sidebar,
    Library,
    Queue,
    Recent,
    /// The Edit popup's manual-search results, inside a dialog.
    ManualSearch,
    Tracks(PaneId),
    Cards(PaneId),
    /// Only a horizontal offset — a shelf has no rows of its own to capture.
    CardRow(PaneId, crate::providers::ArtistSectionKind),
}

impl From<ScrollTarget> for Slot {
    fn from(target: ScrollTarget) -> Self {
        match target {
            ScrollTarget::Tracks(pane) => match pane {
                QUEUE_PANE_ID => Slot::Queue,
                RECENT_PANE_ID => Slot::Recent,
                MANUAL_SEARCH_PANE_ID => Slot::ManualSearch,
                _ => Slot::Tracks(pane),
            },
            ScrollTarget::Cards(pane) => Slot::Cards(pane),
            ScrollTarget::CardRow(pane, section) => Slot::CardRow(pane, section),
            ScrollTarget::Library => Slot::Library,
        }
    }
}

/// Which scrollable list contains a point. `Track` carries its owning pane
/// because every pane renders its own main track list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ContainingList {
    Queue,
    Recent,
    Track(PaneId),
    Sidebar,
    Library,
}

impl ContainingList {
    /// The list for track-containing lists: the virtual queue/recent pane
    /// for the global panel, or the owning pane for main lists. `None` for
    /// sidebar/library hits.
    pub fn track_target(self) -> Option<PaneId> {
        match self {
            ContainingList::Queue => Some(QUEUE_PANE_ID),
            ContainingList::Track(pane) => Some(pane),
            ContainingList::Recent => Some(RECENT_PANE_ID),
            ContainingList::Sidebar | ContainingList::Library => None,
        }
    }
}

#[derive(Default, Clone, Debug)]
pub struct CaptureBounds {
    pub sidebar: Option<ListGeometry>,
    pub library: Option<ListGeometry>,
    pub queue: Option<ListGeometry>,
    /// The Edit popup's manual-search list, captured while that popup is open.
    pub manual_search: Option<ListGeometry>,
    pub tracks: HashMap<PaneId, ListGeometry>,
    /// Scroll offsets of the per-pane scrollables that host cards (a search
    /// card tab, an artist page). Kept apart from `tracks` because card rows
    /// are not drop targets and never take part in hit-testing — only in
    /// converting a card right-click to window coordinates.
    pub cards: HashMap<PaneId, ListGeometry>,
    /// Horizontal scroll offsets of artist-page card shelves, keyed by pane *and*
    /// section (every section lays its shelf out on each pass, so one key per
    /// pane would collide). A card right-click inside a shelf is reported with
    /// the cursor shifted by this offset, so the menu has to undo it.
    pub card_rows: HashMap<(PaneId, crate::providers::ArtistSectionKind), f32>,
    pub recent: Option<ListGeometry>,
    /// Captured separately by [`CaptureSearchHistoryRows`] (see that struct).
    pub search_history: HashMap<PaneId, ListGeometry>,
    /// Search-bar input rects, anchoring the per-pane history dropdown
    /// overlay (`ui::view` reads these; clicks report their pane via
    /// `SearchInputClicked`, so no cursor hit-testing is involved).
    pub search_inputs: HashMap<PaneId, Rectangle>,
    /// Captured by [`CaptureContextMenu`] when the context menu opens.
    pub context_menu: Option<ContextMenuGeometry>,
    panes: Vec<PaneId>,
    /// Cached id→pane map for search inputs, so `text_input` is O(1).
    search_input_ids: HashMap<Id, PaneId>,
    /// The list whose rows are being visited right now — only the two lists
    /// with measured rows (`Sidebar`/`Library`) collect containers.
    current: Option<Slot>,
}

impl From<CaptureBounds> for Task<Message> {
    fn from(bounds: CaptureBounds) -> Self {
        iced_runtime::task::widget(bounds)
    }
}

impl CaptureBounds {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_panes(panes: Vec<PaneId>) -> Self {
        let mut bounds = Self {
            panes,
            ..Self::default()
        };
        for pane in bounds.panes.clone() {
            bounds
                .search_input_ids
                .insert(crate::app::ui::search_input_id(pane), pane);
        }
        bounds
    }

    pub fn track_geo(&self, pane: PaneId) -> Option<&ListGeometry> {
        self.tracks.get(&pane)
    }

    /// The captured viewport of any track list addressed by a pane id,
    /// including the virtual ones (queue, recent, manual search).
    pub fn list_geo(&self, pane: PaneId) -> Option<&ListGeometry> {
        match pane {
            QUEUE_PANE_ID => self.queue.as_ref(),
            RECENT_PANE_ID => self.recent.as_ref(),
            MANUAL_SEARCH_PANE_ID => self.manual_search.as_ref(),
            _ => self.tracks.get(&pane),
        }
    }

    /// How far an artist page's card shelf is scrolled sideways.
    pub fn card_row_scroll(
        &self,
        pane: PaneId,
        section: crate::providers::ArtistSectionKind,
    ) -> f32 {
        self.card_rows.get(&(pane, section)).copied().unwrap_or(0.0)
    }

    /// How far a scrollable is scrolled along its own axis, for turning a
    /// reported cursor point back into a window position.
    pub fn scroll_offset(&self, target: ScrollTarget) -> f32 {
        match target {
            ScrollTarget::CardRow(pane, section) => self.card_row_scroll(pane, section),
            _ => self
                .geometry(Slot::from(target))
                .map_or(0.0, |g| g.translation_y),
        }
    }

    /// Record an offset a scrollable reported live, keeping it fresh between
    /// capture passes (which only run on navigation, results and resize).
    pub fn set_scroll_offset(&mut self, target: ScrollTarget, translation: iced::Vector) {
        // Only a card host creates its geometry here — it holds nothing but
        // the offset, as its bounds are never read. Track lists are virtualized
        // off captured bounds and the library list is a drop target, so those
        // are only ever updated in place.
        let geo = match Slot::from(target) {
            Slot::CardRow(pane, section) => {
                self.card_rows.insert((pane, section), translation.x);
                None
            }
            Slot::Cards(pane) => Some(self.cards.entry(pane).or_default()),
            Slot::Tracks(pane) => self.tracks.get_mut(&pane),
            Slot::Queue => self.queue.as_mut(),
            Slot::Recent => self.recent.as_mut(),
            Slot::ManualSearch => self.manual_search.as_mut(),
            Slot::Library => self.library.as_mut(),
            Slot::Sidebar => self.sidebar.as_mut(),
        };
        if let Some(g) = geo {
            g.translation_y = translation.y;
        }
    }

    /// Drop everything captured for `pane`, so a closed pane leaves nothing
    /// behind (its lists are gone, so nothing can refresh them).
    pub fn forget_pane(&mut self, pane: PaneId) {
        self.tracks.remove(&pane);
        self.manual_search = None;
        self.cards.remove(&pane);
        self.search_history.remove(&pane);
        self.search_inputs.remove(&pane);
        self.search_input_ids.retain(|_, p| *p != pane);
        self.card_rows.retain(|(p, _), _| *p != pane);
    }

    /// The slot an id'd scrollable belongs to, if it is one we capture.
    fn slot(&self, id: &Id) -> Option<Slot> {
        if *id == SIDEBAR_LIST_ID {
            return Some(Slot::Sidebar);
        }
        if *id == LIBRARY_LIST_ID {
            return Some(Slot::Library);
        }
        if *id == QUEUE_LIST_ID {
            return Some(Slot::Queue);
        }
        if *id == QUEUE_RECENT_LIST_ID {
            return Some(Slot::Recent);
        }
        if *id == MANUAL_SEARCH_LIST_ID {
            return Some(Slot::ManualSearch);
        }
        self.panes.iter().copied().find_map(|pane| {
            if *id == track_list_id(pane) {
                Some(Slot::Tracks(pane))
            } else if *id == card_list_id(pane) {
                Some(Slot::Cards(pane))
            } else {
                crate::providers::ArtistSectionKind::ALL
                    .into_iter()
                    .find(|section| *id == card_row_id(pane, *section))
                    .map(|section| Slot::CardRow(pane, section))
            }
        })
    }

    /// The captured geometry for a slot, creating an empty one on first sight.
    /// Only the capture pass uses this, which is what creates the entries the
    /// other lists are read through.
    fn ensure_geometry(&mut self, slot: Slot) -> &mut ListGeometry {
        match slot {
            Slot::Sidebar => self.sidebar.get_or_insert_with(ListGeometry::default),
            Slot::Library => self.library.get_or_insert_with(ListGeometry::default),
            Slot::Queue => self.queue.get_or_insert_with(ListGeometry::default),
            Slot::Recent => self.recent.get_or_insert_with(ListGeometry::default),
            Slot::ManualSearch => self.manual_search.get_or_insert_with(ListGeometry::default),
            Slot::Tracks(pane) => self.tracks.entry(pane).or_default(),
            Slot::Cards(pane) => self.cards.entry(pane).or_default(),
            Slot::CardRow(_, _) => unreachable!("a card shelf has no geometry of its own"),
        }
    }

    fn geometry(&self, slot: Slot) -> Option<&ListGeometry> {
        match slot {
            Slot::Sidebar => self.sidebar.as_ref(),
            Slot::Library => self.library.as_ref(),
            Slot::Queue => self.queue.as_ref(),
            Slot::Recent => self.recent.as_ref(),
            Slot::ManualSearch => self.manual_search.as_ref(),
            Slot::Tracks(pane) => self.tracks.get(&pane),
            Slot::Cards(pane) => self.cards.get(&pane),
            Slot::CardRow(_, _) => None,
        }
    }

    /// The list whose bounds contain `point`, if any. Lists don't overlap so
    /// the first hit wins.
    pub fn get_containing(&self, point: iced::Point) -> Option<(ContainingList, &ListGeometry)> {
        if let Some(g) = &self.queue {
            if g.bounds.contains(point) {
                return Some((ContainingList::Queue, g));
            }
        }
        if let Some(g) = &self.recent {
            if g.bounds.contains(point) {
                return Some((ContainingList::Recent, g));
            }
        }
        for (pane, g) in &self.tracks {
            if g.bounds.contains(point) {
                return Some((ContainingList::Track(*pane), g));
            }
        }
        if let Some(g) = &self.sidebar {
            if g.bounds.contains(point) {
                return Some((ContainingList::Sidebar, g));
            }
        }
        if let Some(g) = &self.library {
            if g.bounds.contains(point) {
                return Some((ContainingList::Library, g));
            }
        }
        None
    }
}

impl Operation<Message> for CaptureBounds {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Message>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _content_bounds: Rectangle,
        translation: iced::Vector,
        _state: &mut dyn Scrollable,
    ) {
        let Some(slot) = id.and_then(|id| self.slot(id)) else {
            self.current = None;
            return;
        };
        // Only the two lists with measured rows (sidebar/library) collect containers.
        self.current = matches!(slot, Slot::Sidebar | Slot::Library).then_some(slot);
        match slot {
            // A shelf is horizontal-only: it has no geometry of its own.
            Slot::CardRow(pane, section) => {
                self.card_rows.insert((pane, section), translation.x);
            }
            _ => {
                *self.ensure_geometry(slot) = ListGeometry {
                    bounds,
                    translation_y: translation.y,
                    rows: Vec::new(),
                };
            }
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        let Some(slot @ (Slot::Sidebar | Slot::Library)) = self.current else {
            return;
        };
        if id.is_some() {
            self.ensure_geometry(slot).rows.push(bounds);
        }
    }

    fn text_input(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _state: &mut dyn iced_core::widget::operation::TextInput,
    ) {
        let Some(id) = id else {
            return;
        };
        if let Some(pane) = self.search_input_ids.get(id).copied() {
            self.search_inputs.insert(pane, bounds);
        }
    }

    fn finish(&self) -> Outcome<Message> {
        Outcome::Some(Message::ListBoundsCaptured(Box::new(self.clone())))
    }
}

#[derive(Default, Clone, Debug)]
pub struct CaptureSearchHistoryRows {
    pane: PaneId,
    geo: Option<ListGeometry>,
    current: bool,
}

impl From<CaptureSearchHistoryRows> for Task<Message> {
    fn from(capture: CaptureSearchHistoryRows) -> Self {
        iced_runtime::task::widget(capture)
    }
}

impl CaptureSearchHistoryRows {
    pub fn new(pane: PaneId) -> Self {
        Self {
            pane,
            ..Self::default()
        }
    }
}

impl Operation<Message> for CaptureSearchHistoryRows {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Message>)) {
        operate(self);
    }

    fn scrollable(
        &mut self,
        id: Option<&Id>,
        bounds: Rectangle,
        _content_bounds: Rectangle,
        translation: iced::Vector,
        _state: &mut dyn Scrollable,
    ) {
        if id == Some(&search_history_list_id(self.pane)) {
            self.current = true;
            self.geo = Some(ListGeometry {
                bounds,
                translation_y: translation.y,
                rows: Vec::new(),
            });
        } else {
            self.current = false;
        }
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        if self.current && id.is_some() {
            if let Some(g) = &mut self.geo {
                g.rows.push(bounds);
            }
        }
    }

    fn finish(&self) -> Outcome<Message> {
        match self.geo.clone() {
            Some(mut geo) => {
                if !geo.rows.is_empty() {
                    let last = theme::SEARCH_DROPDOWN_MAX_ITEMS.min(geo.rows.len()) - 1;
                    geo.bounds.height = geo.rows[last].y + geo.rows[last].height - geo.bounds.y;
                }
                Outcome::Some(Message::SearchHistoryBoundsCaptured(self.pane, geo))
            }
            None => Outcome::None,
        }
    }
}

/// Measured context-menu layout, derived from captured bounds. The view and
/// the position-flip logic both read this instead of raw rects.
#[derive(Debug, Clone)]
pub struct ContextMenuGeometry {
    pub panel: Rectangle,
    /// Main-menu row tops relative to the panel top, in display order.
    pub row_offsets: Vec<f32>,
    /// True once two consecutive captures agree, i.e. the measured width was
    /// not clipped by the window edge; rows only stretch to full width then.
    pub stable: bool,
}

/// Captures the context-menu panel and each main-menu row's bounds. Run once
/// when the menu opens; the geometry drives submenu alignment and the
/// flip-inside-window placement done in the handler.
#[derive(Default, Clone, Debug)]
pub struct CaptureContextMenu {
    panel: Option<Rectangle>,
    rows: Vec<Rectangle>,
}

impl From<CaptureContextMenu> for Task<Message> {
    fn from(capture: CaptureContextMenu) -> Self {
        iced_runtime::task::widget(capture)
    }
}

impl Operation<Message> for CaptureContextMenu {
    fn traverse(&mut self, operate: &mut dyn FnMut(&mut dyn Operation<Message>)) {
        operate(self);
    }

    fn container(&mut self, id: Option<&Id>, bounds: Rectangle) {
        use crate::app::ui::{CONTEXT_MENU_PANEL_ID, CONTEXT_MENU_ROW_ID};
        match id {
            Some(id) if *id == CONTEXT_MENU_ROW_ID => self.rows.push(bounds),
            Some(id) if *id == CONTEXT_MENU_PANEL_ID => self.panel = Some(bounds),
            _ => {}
        }
    }

    fn finish(&self) -> Outcome<Message> {
        let Some(panel) = self.panel else {
            return Outcome::None;
        };
        Outcome::Some(Message::ContextMenuBoundsCaptured {
            panel,
            row_offsets: self.rows.iter().map(|r| r.y - panel.y).collect(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ArtistSectionKind;

    #[test]
    #[allow(clippy::float_cmp)]
    fn card_shelves_are_keyed_per_pane_and_section() {
        let pane = 3;
        let mut bounds = CaptureBounds::with_panes(vec![pane, 4]);
        // Distinct per pane *and* section: every section lays its shelf out on
        // each pass, so one key per pane would make one shelf's offset land on
        // another's cards.
        assert_ne!(
            card_row_id(pane, ArtistSectionKind::Albums),
            card_row_id(pane, ArtistSectionKind::Related)
        );
        assert_ne!(
            card_row_id(pane, ArtistSectionKind::Albums),
            card_row_id(4, ArtistSectionKind::Albums)
        );

        bounds
            .card_rows
            .insert((pane, ArtistSectionKind::Albums), 140.0);
        assert_eq!(
            bounds.card_row_scroll(pane, ArtistSectionKind::Albums),
            140.0
        );
        assert_eq!(
            bounds.card_row_scroll(pane, ArtistSectionKind::Related),
            0.0
        );
    }

    #[test]
    fn only_live_pane_card_hosts_are_captured() {
        let bounds = CaptureBounds::with_panes(vec![3]);
        assert_eq!(bounds.slot(&card_list_id(3)), Some(Slot::Cards(3)));
        assert_eq!(
            bounds.slot(&card_row_id(3, ArtistSectionKind::Albums)),
            Some(Slot::CardRow(3, ArtistSectionKind::Albums))
        );
        assert_eq!(bounds.slot(&track_list_id(3)), Some(Slot::Tracks(3)));
        // A closed pane's ids stop being captured, so its shelves stop feeding
        // the pass.
        assert_eq!(bounds.slot(&card_list_id(9)), None);
        assert_eq!(
            bounds.slot(&card_row_id(9, ArtistSectionKind::Albums)),
            None
        );
    }

    #[test]
    fn forgetting_a_pane_drops_all_of_its_captures() {
        let pane = 3;
        let mut bounds = CaptureBounds::with_panes(vec![pane, 4]);
        bounds.cards.insert(pane, ListGeometry::default());
        bounds.tracks.insert(pane, ListGeometry::default());
        bounds
            .card_rows
            .insert((pane, ArtistSectionKind::Albums), 10.0);
        bounds.search_history.insert(pane, ListGeometry::default());
        bounds.search_inputs.insert(pane, Rectangle::default());

        bounds.forget_pane(pane);

        assert!(bounds.cards.is_empty());
        assert!(bounds.tracks.is_empty());
        assert!(bounds.search_history.is_empty());
        assert!(bounds.search_inputs.is_empty());
        assert!(bounds.card_rows.is_empty());
    }
}
