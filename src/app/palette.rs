//! Spotlight-style palette state (`F1` / `?` / `Ctrl+P`): fuzzy search over commands,
//! views, saved library entries and playlists.
//!
//! The same overlay is the `Ctrl+K` playlist picker (`PaletteMode::Playlists`)
//! and the add-to-playlist picker (`PaletteMode::AddToPlaylist`); both narrow
//! the list to playlists, and the latter hands the chosen one to the caller.

use super::{
    command::{CommandId, COMMANDS},
    pane::PaneId,
    Message, MusicPlayer,
};
use crate::{app::ViewData, i18n::Strings, util::FuzzyQuery};

/// How many saved library entries the empty-query home screen previews.
const LIBRARY_PREVIEW: usize = 5;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingAdd {
    pub indices: Vec<usize>,
    /// Source list: a main pane id, or [`super::pane::QUEUE_PANE_ID`] /
    /// [`super::pane::RECENT_PANE_ID`] for the global panel lists.
    pub pane: PaneId,
}

/// What the palette is being used for. `All` is the command/help surface
/// (`F1` / `?` / `Ctrl+P`), `Playlists` is the `Ctrl+K` picker, and `AddToPlaylist` is
/// the context-menu picker that hands the chosen list back to the caller.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum PaletteMode {
    #[default]
    All,
    Playlists,
    AddToPlaylist(PendingAdd),
}

#[derive(Debug, Clone, Default)]
pub struct Palette {
    pub query: String,
    /// Index into the rows [`Palette::rows`] returns; group headers are not
    /// selectable, so this never points at one.
    pub selected: usize,
    pub mode: PaletteMode,
    /// Height of the list's viewport, reported by the scrollable. `None` until
    /// the list overflows and the scrollable has laid out, which is exactly
    /// when scrolling is needed.
    pub viewport: Option<f32>,
}

/// Fixed display order of the result groups.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteGroup {
    Commands,
    Views,
    Library,
    Playlists,
}

impl PaletteGroup {
    pub fn title(self, strings: &Strings) -> &'static str {
        match self {
            Self::Commands => strings.palette_commands,
            Self::Views => strings.palette_views,
            Self::Library => strings.library,
            Self::Playlists => strings.playlists,
        }
    }
}

/// The sidebar destinations, offered by name.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteView {
    Search,
    Downloads,
    Trashbin,
    Settings,
}

impl PaletteView {
    pub const ALL: [PaletteView; 4] = [
        PaletteView::Search,
        PaletteView::Downloads,
        PaletteView::Trashbin,
        PaletteView::Settings,
    ];

    pub fn label(self, strings: &Strings) -> &'static str {
        match self {
            Self::Search => strings.search,
            Self::Downloads => strings.downloads,
            Self::Trashbin => strings.trashbin,
            Self::Settings => strings.settings,
        }
    }

    /// Untranslated synonyms, matched in addition to the localized label.
    pub fn keywords(self) -> &'static str {
        match self {
            Self::Search => "search find browse",
            Self::Downloads => "downloads offline saved",
            Self::Trashbin => "trash bin deleted removed",
            Self::Settings => "settings preferences options config",
        }
    }

    pub fn message(self, pane: PaneId) -> Message {
        match self {
            // The sidebar button's message: restore the last search view, or
            // run the blank-query browse until one exists.
            Self::Search => Message::SidebarSearch,
            Self::Downloads => Message::NavigateTo(pane, ViewData::new_downloads(Vec::new())),
            Self::Trashbin => Message::NavigateTo(pane, ViewData::new_trashbin()),
            Self::Settings => Message::NavigateTo(pane, ViewData::new_settings()),
        }
    }
}

/// What a row does. Playlists and library entries carry their index into the
/// owning store; the rest resolve to a message.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PaletteItem {
    Command(CommandId),
    View(PaletteView),
    Library(usize),
    Playlist(usize),
}

impl PaletteItem {
    /// The message that runs this row, for the rows that are plain dispatches.
    /// Playlists and library cards depend on navigation state, so they return
    /// `None` and are resolved by the update handler.
    pub fn message(self, player: &MusicPlayer) -> Option<Message> {
        match self {
            Self::Command(id) => Some(id.message(player)),
            Self::View(view) => Some(view.message(player.focused_pane_id)),
            Self::Library(_) | Self::Playlist(_) => None,
        }
    }
}

/// One rendered row: the action plus the text the filter matched.
#[derive(Debug, Clone)]
pub struct PaletteRow {
    pub item: PaletteItem,
    pub label: String,
    /// Trailing hint: the key chord for commands, the provider for saved
    /// library entries, the track count for playlists.
    pub detail: Option<String>,
    pub group: PaletteGroup,
}

impl Palette {
    /// Rows matching the live query, in group order. The view and the keyboard
    /// handlers both read this, so the selection index stays aligned.
    pub fn rows(&self, player: &MusicPlayer) -> Vec<PaletteRow> {
        let strings = player.strings;
        let needle = FuzzyQuery::new(&self.query);
        let mut rows = Vec::new();

        // The pickers only ever offer playlists.
        if self.mode == PaletteMode::All {
            for command in COMMANDS {
                let label = (command.label)(strings);
                if needle.matches(label) || needle.matches(command.keywords) {
                    rows.push(PaletteRow {
                        item: PaletteItem::Command(command.id),
                        label: label.to_string(),
                        detail: Some(command.keys.to_string()).filter(|k| !k.is_empty()),
                        group: PaletteGroup::Commands,
                    });
                }
            }
            for view in PaletteView::ALL {
                let label = view.label(strings);
                if needle.matches(label) || needle.matches(view.keywords()) {
                    rows.push(PaletteRow {
                        item: PaletteItem::View(view),
                        label: label.to_string(),
                        detail: None,
                        group: PaletteGroup::Views,
                    });
                }
            }
            // An empty query is the palette's home screen: cap the saved
            // library there (newest first) so a large library doesn't bury the
            // commands, but keep every match once the user types.
            let library_limit = if self.query.trim().is_empty() {
                LIBRARY_PREVIEW
            } else {
                usize::MAX
            };
            for (index, item) in player
                .library
                .items
                .iter()
                .enumerate()
                .rev()
                .take(library_limit)
            {
                if needle.matches(&item.title) {
                    rows.push(PaletteRow {
                        item: PaletteItem::Library(index),
                        label: item.title.clone(),
                        detail: Some(strings.provider(item.provider).to_string()),
                        group: PaletteGroup::Library,
                    });
                }
            }
        }

        for (index, playlist) in player.playlists.playlists.iter().enumerate() {
            if needle.matches(&playlist.name) {
                rows.push(PaletteRow {
                    item: PaletteItem::Playlist(index),
                    label: playlist.name.clone(),
                    detail: Some(playlist.tracks.len().to_string()),
                    group: PaletteGroup::Playlists,
                });
            }
        }

        rows
    }

    /// Scroll offset that centers the selected row in a `viewport`-tall list,
    /// clamped to the scrollable range. `None` when the selection has no row
    /// (nothing matched) or the viewport height isn't known yet.
    pub fn scroll_target(&self, rows: &[PaletteRow], viewport: f32) -> Option<f32> {
        if self.selected >= rows.len() {
            return None;
        }
        let row_center = row_top(rows, self.selected) + crate::theme::PALETTE_ROW_HEIGHT / 2.0;
        let max = (content_height(rows) - viewport).max(0.0);
        Some((row_center - viewport / 2.0).clamp(0.0, max))
    }
}

/// Top edge of row `index` in list coordinates. Rows and group headers are
/// pinned to the heights the view gives them, so the offset can be computed
/// without measuring the rendered list.
fn row_top(rows: &[PaletteRow], index: usize) -> f32 {
    let mut y = 0.0;
    let mut group = None;
    for (pos, row) in rows.iter().enumerate() {
        if group != Some(row.group) {
            y += crate::theme::PALETTE_HEADER_HEIGHT + crate::theme::SPACING_XS;
            group = Some(row.group);
        }
        if pos == index {
            return y;
        }
        y += crate::theme::PALETTE_ROW_HEIGHT + crate::theme::SPACING_XS;
    }
    y
}

/// Height of the whole rendered list: headers, rows and the gaps between them.
fn content_height(rows: &[PaletteRow]) -> f32 {
    let mut y = 0.0;
    let mut group = None;
    for row in rows {
        if group != Some(row.group) {
            y += crate::theme::PALETTE_HEADER_HEIGHT + crate::theme::SPACING_XS;
            group = Some(row.group);
        }
        y += crate::theme::PALETTE_ROW_HEIGHT + crate::theme::SPACING_XS;
    }
    (y - crate::theme::SPACING_XS).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data::{config, library::Card, playlists::Playlist},
        providers::ProviderId,
    };

    fn player() -> MusicPlayer {
        let mut p = MusicPlayer::new_with(config::Config::default());
        p.playlists.playlists = vec![
            Playlist {
                name: "Chill".into(),
                tracks: Vec::new(),
            },
            Playlist {
                name: "Focus".into(),
                tracks: Vec::new(),
            },
        ];
        p.library.items = vec![Card::new(crate::data::library::LibraryItem {
            kind: crate::data::library::LibraryKind::Album,
            id: "a1".into(),
            title: "Blue Train".into(),
            thumbnail: String::new(),
            provider: ProviderId::YouTube,
            url: String::new(),
        })];
        p
    }

    fn palette(query: &str) -> Palette {
        Palette {
            query: query.into(),
            ..Default::default()
        }
    }

    #[test]
    fn empty_query_lists_every_group_in_order() {
        let p = player();
        let rows = palette("").rows(&p);
        let groups: Vec<_> = rows.iter().map(|r| r.group).collect();
        assert_eq!(
            groups.first().copied(),
            Some(PaletteGroup::Commands),
            "{groups:?}"
        );
        assert!(groups.contains(&PaletteGroup::Views));
        assert!(groups.contains(&PaletteGroup::Library));
        assert_eq!(groups.last().copied(), Some(PaletteGroup::Playlists));
        // Groups are contiguous: a later group never reappears after another.
        let mut seen = Vec::new();
        for group in &groups {
            if seen.last() != Some(group) {
                assert!(!seen.contains(group), "group {group:?} is not contiguous");
                seen.push(*group);
            }
        }
    }

    #[test]
    fn keywords_find_a_command_its_label_does_not_name() {
        let p = player();
        let rows = palette("lyrics").rows(&p);
        assert_eq!(
            rows.iter().map(|r| r.item).collect::<Vec<_>>(),
            vec![PaletteItem::Command(CommandId::ShowLyrics)]
        );
    }

    #[test]
    fn playlists_filter_by_name() {
        let p = player();
        let rows = palette("chil").rows(&p);
        assert_eq!(
            rows.iter().map(|r| r.item).collect::<Vec<_>>(),
            vec![PaletteItem::Playlist(0)]
        );
    }

    #[test]
    fn pending_add_offers_only_playlists() {
        let p = player();
        let mut palette = palette("");
        palette.mode = PaletteMode::AddToPlaylist(PendingAdd {
            indices: vec![0],
            pane: p.focused_pane_id,
        });
        let rows = palette.rows(&p);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.group == PaletteGroup::Playlists));
    }

    #[test]
    fn playlist_picker_offers_only_playlists() {
        let p = player();
        let mut palette = palette("");
        palette.mode = PaletteMode::Playlists;
        let rows = palette.rows(&p);
        assert_eq!(rows.len(), 2);
        assert!(rows.iter().all(|r| r.group == PaletteGroup::Playlists));
    }

    #[test]
    fn scroll_target_centers_the_selected_row() {
        let p = player();
        let rows = palette("").rows(&p);
        let mut palette = palette("");
        let viewport = 120.0;

        palette.selected = 3;
        let center = row_top(&rows, 3) + crate::theme::PALETTE_ROW_HEIGHT / 2.0 - viewport / 2.0;
        let target = palette.scroll_target(&rows, viewport).expect("a row");
        assert!((target - center).abs() < 0.01, "{target} != {center}");

        // The last row can't scroll past the end of the list.
        palette.selected = rows.len() - 1;
        let target = palette.scroll_target(&rows, viewport).expect("a row");
        assert!((target - (content_height(&rows) - viewport)).abs() < 0.01);

        // A list that fits never scrolls, and an empty one has no target.
        palette.selected = 0;
        let target = palette.scroll_target(&rows, 10_000.0).expect("a row");
        assert!(target.abs() < 0.01, "{target} != 0");
        assert!(palette.scroll_target(&[], viewport).is_none());
    }
}
