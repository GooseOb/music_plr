use crate::types::Track;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct NavEntry {
    pub data: ViewData,
}

/// All per-view state, stored in one flat struct. The `kind` field is the
/// only variant-specific part; everything else is shared view chrome that is
/// identical regardless of which view is active. Serialized into [`NavEntry`]
/// for back/forward history and [`crate::data::session::SessionState`] for restore.
///
/// `query` (the search-bar text) is intentionally kept as a field on
/// [`MusicPlayer`] rather than here: the search bar is always visible
/// regardless of which view is active, so the query is global UI state
/// rather than view-specific data.
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
    pub bounds: Option<iced::Rectangle>,
}

/// The kind of view currently active. Carries everything that differs between
/// views: the search `exhausted` flag, the radio label, and the selected
/// playlist identity.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewKind {
    Search {
        exhausted: bool,
        query: String,
        /// Which search tab is active. For `Songs`/`Videos` the playable track
        /// list lives in `ViewData.tracks`; the other variants carry their own
        /// concrete-typed card lists for drill-down.
        tab: crate::youtube::SearchTab,
    },
    SongRadio(String),
    ArtistRadio(String),
    /// Drill-down into an artist's songs (browse id from a search result).
    Artist {
        browse_id: String,
        name: String,
    },
    /// Drill-down into an album's tracklist (browse id from a search result).
    Album {
        browse_id: String,
        title: String,
    },
    /// Drill-down into a playlist's tracks (playlist id from a search result).
    PlaylistView {
        playlist_id: String,
        title: String,
    },
    Playlist {
        selected_playlist: Option<usize>,
        playlist_name: String,
    },
    Downloads,
}

impl Default for ViewKind {
    fn default() -> Self {
        ViewKind::Search {
            exhausted: false,
            query: String::new(),
            tab: crate::youtube::SearchTab::Songs,
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
                    tab: ta,
                },
                ViewKind::Search {
                    exhausted: b,
                    query: qb,
                    tab: tb,
                },
            ) => a == b && qa == qb && ta == tb,
            // Distinct variants despite identical bodies: a SongRadio and an
            // ArtistRadio with the same label are different views.
            (ViewKind::SongRadio(a), ViewKind::SongRadio(b))
            | (ViewKind::ArtistRadio(a), ViewKind::ArtistRadio(b)) => a == b,
            (
                ViewKind::Playlist {
                    selected_playlist: a,
                    playlist_name: c,
                },
                ViewKind::Playlist {
                    selected_playlist: b,
                    playlist_name: d,
                },
            ) => a == b && c == d,
            (ViewKind::Downloads, ViewKind::Downloads) => true,
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
            ViewKind::Playlist {
                selected_playlist, ..
            } => *selected_playlist,
            _ => None,
        }
    }

    pub fn playlist_name(&self) -> &str {
        match &self.kind {
            ViewKind::Playlist { playlist_name, .. } => playlist_name,
            _ => "",
        }
    }

    // ── constructors ─────────────────────────────────────────────

    /// Create a fresh `Search` view for `query` on the given search `scope`.
    pub fn new_search(query: String, scope: crate::youtube::SearchScope) -> Self {
        Self {
            kind: ViewKind::Search {
                exhausted: false,
                query,
                tab: crate::youtube::SearchTab::from_scope(scope),
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

    /// Create a `Playlist` view, preserving the selected playlist from the
    /// previous view data if it was already a Playlist view.
    pub fn new_playlist(
        selected_playlist: Option<usize>,
        playlist_name: String,
        old: Option<&Self>,
    ) -> Self {
        let (sp, name) = match old {
            Some(v) if matches!(v.kind, ViewKind::Playlist { .. }) => {
                (v.selected_playlist_id(), v.playlist_name().to_string())
            }
            _ => (selected_playlist, playlist_name),
        };
        Self {
            kind: ViewKind::Playlist {
                selected_playlist: sp,
                playlist_name: name,
            },
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
}
