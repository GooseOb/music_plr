use crate::{
    data::library::{LibraryItem, LibraryKind},
    provider::ProviderId,
    types::Track,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Default)]
pub struct RequestIdGenerator(u64);

impl RequestIdGenerator {
    pub fn next(&mut self) -> u64 {
        self.0 += 1;
        self.0
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ViewData {
    pub kind: ViewKind,
    /// The view's own track list. For `Playlist` the tracks live in the
    /// `PlaylistStore` (see [`MusicPlayer::view_tracks`]); this stays empty.
    pub tracks: Vec<Track>,
    /// Used by `Search` and `Radio`; harmless (always false) elsewhere.
    pub loading: bool,
    pub selection: Vec<usize>,
    pub scroll: f32,
    #[serde(skip)]
    pub request_id: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ViewKind {
    Search {
        exhausted: bool,
        query: String,
        provider: ProviderId,
        tab: crate::provider::SearchTab,
    },
    SongRadio(String),
    ArtistRadio(String),
    Artist {
        id: String,
        name: String,
    },
    Album {
        id: String,
        name: String,
    },
    PlaylistView {
        id: String,
        name: String,
    },
    Playlist {
        index: Option<usize>,
        name: String,
    },
    Downloads,
    Settings,
}

impl From<LibraryItem> for ViewKind {
    fn from(item: LibraryItem) -> Self {
        match item.kind {
            LibraryKind::Artist => ViewKind::Artist {
                id: item.id,
                name: item.title,
            },
            LibraryKind::Album => ViewKind::Album {
                id: item.id,
                name: item.title,
            },
            LibraryKind::Playlist => ViewKind::PlaylistView {
                id: item.id,
                name: item.title,
            },
        }
    }
}

impl Default for ViewKind {
    fn default() -> Self {
        ViewKind::Search {
            exhausted: false,
            query: String::new(),
            provider: ProviderId::YouTube,
            tab: crate::provider::SearchTab::Songs,
        }
    }
}

impl ViewKind {
    pub fn browse_params(&self) -> Option<(&str, &'static str, &str)> {
        match self {
            ViewKind::Artist { id, name } => Some((id, "artist", name)),
            ViewKind::Album { id, name } => Some((id, "album", name)),
            ViewKind::PlaylistView { id, name } => Some((id, "playlist", name)),
            _ => None,
        }
    }
}

impl ViewData {
    /// True for Search and Radio views (the scrollable text lists).
    pub fn is_search_like(&self) -> bool {
        matches!(
            self.kind,
            ViewKind::Search { .. }
                | ViewKind::SongRadio(_)
                | ViewKind::ArtistRadio(_)
                | ViewKind::Artist { .. }
                | ViewKind::Album { .. }
                | ViewKind::PlaylistView { .. }
        )
    }

    /// True if this and `other` are the same view kind. Used by navigation to
    /// detect a no-op self-navigation. Compares variant identity and the
    /// fields that distinguish views. For `Search` the `query` is included so
    /// that re-running a search for the same query is a no-op (and distinct
    /// queries are separate history entries), preventing the nav history from
    /// flooding with duplicate-query `Search` snapshots.
    pub fn same_kind(&self, other: &Self) -> bool {
        match (&self.kind, &other.kind) {
            (
                ViewKind::Search {
                    exhausted: a,
                    query: qa,
                    provider: pa,
                    tab: ta,
                },
                ViewKind::Search {
                    exhausted: b,
                    query: qb,
                    provider: pb,
                    tab: tb,
                },
            ) => a == b && qa == qb && pa == pb && ta == tb,
            // Distinct variants despite identical bodies: a SongRadio and an
            // ArtistRadio with the same label are different views.
            (ViewKind::SongRadio(a), ViewKind::SongRadio(b))
            | (ViewKind::ArtistRadio(a), ViewKind::ArtistRadio(b)) => a == b,
            (
                ViewKind::Playlist { index: a, name: c },
                ViewKind::Playlist { index: b, name: d },
            ) => a == b && c == d,
            (ViewKind::Downloads, ViewKind::Downloads)
            | (ViewKind::Settings, ViewKind::Settings) => true,
            _ => false,
        }
    }

    /// The radio header label, or empty when not on a radio view.
    pub fn label(&self) -> &str {
        match &self.kind {
            ViewKind::SongRadio(l) | ViewKind::ArtistRadio(l) => l,
            _ => "",
        }
    }

    /// Whether the search results are exhausted (no more pages). Only
    /// meaningful for `Search`; `false` otherwise.
    pub fn exhausted(&self) -> bool {
        matches!(
            self.kind,
            ViewKind::Search {
                exhausted: true,
                ..
            }
        )
    }

    /// Set the search `exhausted` flag. A no-op when not on `Search`.
    pub fn set_exhausted(&mut self, value: bool) {
        if let ViewKind::Search { exhausted, .. } = &mut self.kind {
            *exhausted = value;
        }
    }

    /// The search query for the `Search` view, or empty when not on `Search`.
    pub fn search_query(&self) -> &str {
        match &self.kind {
            ViewKind::Search { query, .. } => query,
            _ => "",
        }
    }

    /// The selected playlist index for the Playlist view, or `None`.
    pub fn selected_playlist_id(&self) -> Option<usize> {
        match &self.kind {
            ViewKind::Playlist { index, .. } => *index,
            _ => None,
        }
    }

    pub fn playlist_name(&self) -> &str {
        match &self.kind {
            ViewKind::Playlist { name, .. } => name,
            _ => "",
        }
    }

    // ── constructors ─────────────────────────────────────────────

    /// Create a fresh `Search` view for `query` on `provider` at the given
    /// `scope`.
    pub fn new_search(
        query: String,
        provider: ProviderId,
        scope: crate::provider::SearchScope,
    ) -> Self {
        Self {
            kind: ViewKind::Search {
                exhausted: false,
                query,
                provider,
                tab: crate::provider::SearchTab::from_scope(scope),
            },
            ..Default::default()
        }
    }

    /// Create a `Radio` view from the given kind (which already holds the
    /// label), initially loading.
    pub fn new_radio(kind: ViewKind) -> Self {
        Self {
            kind,
            loading: true,
            ..Default::default()
        }
    }

    /// Create a `Playlist` view.
    pub fn new_playlist(index: Option<usize>, name: String) -> Self {
        Self {
            kind: ViewKind::Playlist { index, name },
            ..Default::default()
        }
    }

    /// Create a `Downloads` view with the given tracks.
    pub fn new_downloads(tracks: Vec<Track>) -> Self {
        Self {
            kind: ViewKind::Downloads,
            tracks,
            ..Default::default()
        }
    }

    /// Create a `Settings` view.
    pub fn new_settings() -> Self {
        Self {
            kind: ViewKind::Settings,
            ..Default::default()
        }
    }
}
