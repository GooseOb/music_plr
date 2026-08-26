use serde::{Deserialize, Serialize};

use crate::{
    data::library::{LibraryItem, LibraryKind},
    load_state::LoadState,
    providers::ProviderId,
    types::Track,
};

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
    pub content: LoadState<Vec<Track>>,
    pub selection: Vec<usize>,
    pub scroll: f32,
    #[serde(skip)]
    pub request_id: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct SearchData {
    pub query: String,
    pub provider: ProviderId,
    pub tab: crate::providers::SearchTab,
    pub exhausted: bool,
    #[serde(skip)]
    pub append_in_flight: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct AlbumRef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub badge: String,
    #[serde(default)]
    pub date: String,
    #[serde(default)]
    pub thumbnail: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaylistRef {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub thumbnail: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlaylistEntry {
    pub index: usize,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ArtistEntry {
    pub id: String,
    pub name: String,
    /// The provider that owns `id` (the page's entry point).
    pub source: ProviderId,
    /// Per-section selected providers, loaded content and known
    /// per-provider artist ids — all persisted so Back/Forward restores
    /// the page exactly as it was.
    pub page: Box<crate::providers::ArtistPageState>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ViewKind {
    Search(SearchData),
    SongRadio(String),
    ArtistRadio(String),
    Artist(ArtistEntry),
    Album(AlbumRef),
    PlaylistView(PlaylistRef),
    Playlist(PlaylistEntry),
    Downloads,
    Settings,
}

impl From<LibraryItem> for ViewKind {
    fn from(item: LibraryItem) -> Self {
        match item.kind {
            LibraryKind::Artist => ViewKind::Artist(ArtistEntry {
                id: item.id.clone(),
                name: item.title,
                source: item.provider,
                page: Box::new(crate::providers::ArtistPageState::new(
                    item.provider,
                    &item.id,
                )),
            }),
            LibraryKind::Album => ViewKind::Album(AlbumRef {
                id: item.id,
                name: item.title,
                thumbnail: item.thumbnail,
                ..Default::default()
            }),
            LibraryKind::Playlist => ViewKind::PlaylistView(PlaylistRef {
                id: item.id,
                name: item.title,
                thumbnail: item.thumbnail,
            }),
        }
    }
}

impl Default for ViewKind {
    fn default() -> Self {
        ViewKind::Search(SearchData {
            query: String::new(),
            provider: ProviderId::YouTube,
            tab: crate::providers::SearchTab::Songs,
            ..Default::default()
        })
    }
}

impl ViewKind {
    pub fn browse_params(&self) -> Option<(&str, &'static str, &str)> {
        match self {
            // Artist pages have their own load path (`open_artist`) and are
            // not served by the generic browse flow.
            ViewKind::Album(r) => Some((&r.id, "album", &r.name)),
            ViewKind::PlaylistView(r) => Some((&r.id, "playlist", &r.name)),
            _ => None,
        }
    }
}

impl ViewData {
    /// True for Search and Radio views (the scrollable text lists).
    pub fn is_search_like(&self) -> bool {
        matches!(
            self.kind,
            ViewKind::Search(_)
                | ViewKind::SongRadio(_)
                | ViewKind::ArtistRadio(_)
                | ViewKind::Artist(_)
                | ViewKind::Album(_)
                | ViewKind::PlaylistView(_)
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
            // Compare identity fields only: `exhausted`/`append_in_flight`
            // are transient UI state, not what distinguishes views.
            (ViewKind::Search(a), ViewKind::Search(b)) => {
                a.query == b.query && a.provider == b.provider && a.tab == b.tab
            }
            // Distinct variants despite identical bodies: a SongRadio and an
            // ArtistRadio with the same label are different views.
            (ViewKind::SongRadio(a), ViewKind::SongRadio(b))
            | (ViewKind::ArtistRadio(a), ViewKind::ArtistRadio(b)) => a == b,
            (ViewKind::Artist(a), ViewKind::Artist(b)) => a.id == b.id && a.source == b.source,
            (ViewKind::Playlist(a), ViewKind::Playlist(b)) => a == b,
            (ViewKind::Downloads, ViewKind::Downloads)
            | (ViewKind::Settings, ViewKind::Settings) => true,
            _ => false,
        }
    }

    // ── constructors ─────────────────────────────────────────────

    /// Create a fresh `Search` view for `query` on `provider` at the given
    /// `scope`.
    pub fn new_search(
        query: String,
        provider: ProviderId,
        scope: crate::providers::SearchScope,
    ) -> Self {
        Self {
            kind: ViewKind::Search(SearchData {
                query,
                provider,
                tab: crate::providers::SearchTab::from_scope(scope),
                ..Default::default()
            }),
            content: LoadState::Loading,
            ..Default::default()
        }
    }

    /// Create a `Radio` view from the given kind (which already holds the
    /// label), initially loading.
    pub fn new_radio(kind: ViewKind) -> Self {
        Self {
            kind,
            content: LoadState::Loading,
            ..Default::default()
        }
    }

    /// Create a `Playlist` view.
    pub fn new_playlist(index: usize, name: String) -> Self {
        Self {
            kind: ViewKind::Playlist(PlaylistEntry { index, name }),
            ..Default::default()
        }
    }

    /// Create a `Downloads` view with the given tracks.
    pub fn new_downloads(tracks: Vec<Track>) -> Self {
        Self {
            kind: ViewKind::Downloads,
            content: LoadState::Ready(tracks),
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

    /// The view's track list; empty while loading or after a failure.
    pub fn tracks(&self) -> &[Track] {
        match &self.content {
            LoadState::Ready(tracks) => tracks.as_slice(),
            _ => &[],
        }
    }

    pub fn set_tracks(&mut self, tracks: Vec<Track>) {
        self.content = LoadState::Ready(tracks);
    }

    pub fn set_failed(&mut self, msg: String) {
        self.content = LoadState::Failed(msg);
    }

    pub fn tracks_mut(&mut self) -> Option<&mut Vec<Track>> {
        match &mut self.content {
            LoadState::Ready(tracks) => Some(tracks),
            _ => None,
        }
    }
}
