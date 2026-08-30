//! Pluggable music search/stream/download providers.
//!
//! Every source of music (`YouTube`, `SoundCloud`, `MusicBrainz`, …)
//! is described by a [`ProviderId`] plus a small set of capability flags and
//! per-provider operations. Tracks keep a `crate::types::ProviderMap`-scoped
//! set of provider-specific identifiers, so a single logical track can be
//! played/downloaded from any provider that carries it.
//!
//! The shared types and the dispatch entry points live in this module; the
//! per-provider backends (`musicbrainz`, `soundcloud`, `youtube`)
//! implement the provider-specific search/resolve/download logic and return
//! [`crate::types::Track`]s carrying that provider's id.

mod artist_page;
mod musicbrainz;
mod soundcloud;
mod youtube;
mod ytdlp;

use std::{
    collections::HashMap,
    io::Read,
    process::{Command, Output, Stdio},
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::types::Track;

/// Run a short-lived child process to completion, killing it when it exceeds
/// `timeout`. stdout/stderr are drained on helper threads so a chatty child
/// can't deadlock on a full pipe while we poll `try_wait`.
pub(crate) fn run_command_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<Output> {
    fn drain(mut pipe: Option<impl Read>) -> Vec<u8> {
        pipe.take()
            .map(|mut p| {
                let mut buf = Vec::new();
                let _ = p.read_to_end(&mut buf);
                buf
            })
            .unwrap_or_default()
    }

    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut child = cmd
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to run {program}"))?;

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_handle = thread::spawn(move || drain(stdout));
    let stderr_handle = thread::spawn(move || drain(stderr));

    let started = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if started.elapsed() >= timeout {
                    let _ = child.kill();
                    let _ = child.wait();
                    anyhow::bail!(
                        "{program} timed out after {}s",
                        timeout.as_secs_f32().round()
                    );
                }
                thread::sleep(Duration::from_millis(100));
            }
            Err(e) => anyhow::bail!("Failed to wait for {program}: {e}"),
        }
    };

    let stdout = stdout_handle.join().unwrap_or_default();
    let stderr = stderr_handle.join().unwrap_or_default();
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

/// Identifies a music provider. Stored on tracks (which provider is the source of a
/// result) and in configuration (the default stream+download provider).
///
/// `Local` is reserved for user-imported files and is never shown in the
/// provider picker or the default-provider list.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, Default,
)]
pub enum ProviderId {
    #[default]
    YouTube,
    SoundCloud,
    MusicBrainz,
    Local,
}

/// Capability flags for a [`ProviderId`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(clippy::struct_excessive_bools)]
pub struct ProviderCaps {
    pub search: bool,
    pub stream: bool,
    pub download: bool,
    pub radio: bool,
}

impl ProviderId {
    pub fn label(self) -> &'static str {
        match self {
            ProviderId::YouTube => "YouTube",
            ProviderId::SoundCloud => "SoundCloud",
            ProviderId::MusicBrainz => "MusicBrainz",
            ProviderId::Local => "Local",
        }
    }

    /// All providers that appear in the search provider picker (excludes
    /// `Local`, which is not a remote search source).
    pub fn searchable() -> &'static [ProviderId] {
        &[
            ProviderId::YouTube,
            ProviderId::SoundCloud,
            ProviderId::MusicBrainz,
        ]
    }

    /// Providers eligible to be the default provider: must support both
    /// streaming and downloading (`MusicBrainz` is search-only; Local is
    /// excluded).
    pub fn defaultable() -> &'static [ProviderId] {
        &[ProviderId::YouTube, ProviderId::SoundCloud]
    }

    pub fn capabilities(self) -> ProviderCaps {
        let a = crate::deps::availability();
        match self {
            ProviderId::YouTube => ProviderCaps {
                search: a.yt_dlp || a.ytmusicapi,
                stream: a.yt_dlp,
                download: a.yt_dlp,
                radio: a.yt_dlp,
            },
            ProviderId::SoundCloud => ProviderCaps {
                // SoundCloud search is crate-based (no external tool); only
                // streaming/downloading route through yt-dlp.
                search: true,
                stream: a.yt_dlp,
                download: a.yt_dlp,
                radio: false,
            },
            ProviderId::MusicBrainz => ProviderCaps {
                search: true,
                stream: false,
                download: false,
                radio: false,
            },
            ProviderId::Local => ProviderCaps {
                search: false,
                stream: false,
                download: false,
                radio: false,
            },
        }
    }

    /// Scopes a search can be run against for this provider. `YouTube` exposes
    /// the full set; others are reduced to the track list plus whatever card
    /// browsing their API supports.
    pub fn supported_scopes(self) -> &'static [SearchScope] {
        match self {
            ProviderId::YouTube => SearchScope::all(),
            ProviderId::SoundCloud => &[
                SearchScope::Songs,
                SearchScope::Artists,
                SearchScope::Albums,
                SearchScope::Playlists,
            ],
            ProviderId::MusicBrainz => &[
                SearchScope::Songs,
                SearchScope::Artists,
                SearchScope::Albums,
            ],
            ProviderId::Local => &[SearchScope::Songs],
        }
    }

    /// Whether this provider streams/ downloads via yt-dlp (`YouTube`,
    /// `SoundCloud`) rather than a direct HTTP file URL.
    pub fn uses_ytdlp(self) -> bool {
        matches!(self, ProviderId::YouTube | ProviderId::SoundCloud)
    }
}

/// A search scope: which kind of result a provider returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SearchScope {
    #[default]
    Songs,
    Videos,
    Artists,
    Albums,
    Playlists,
}

impl SearchScope {
    pub fn all() -> &'static [SearchScope] {
        &[
            SearchScope::Songs,
            SearchScope::Videos,
            SearchScope::Artists,
            SearchScope::Albums,
            SearchScope::Playlists,
        ]
    }

    /// The `filter=` argument ytmusicapi expects for this scope (YouTube-only
    /// mapping; other providers ignore it).
    pub fn youtube_filter(self) -> &'static str {
        match self {
            SearchScope::Songs => "songs",
            SearchScope::Videos => "videos",
            SearchScope::Artists => "artists",
            SearchScope::Albums => "albums",
            SearchScope::Playlists => "playlists",
        }
    }
}

/// Provider-agnostic card result (artist/album/playlist drill-down).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CardData {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub thumbnail: String,
}

/// Provider-agnostic search result: the playable tracks plus the active tab
/// describing which scopes produced card lists. Only `YouTube` currently
/// produces card tabs; other providers return a flat track list under
/// `SearchTab::Songs`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum SearchTab {
    #[default]
    Songs,
    Videos,
    Artists(Vec<CardData>),
    Albums(Vec<CardData>),
    Playlists(Vec<CardData>),
}

impl SearchTab {
    /// The tab for a search scope when no results have arrived yet.
    pub fn from_scope(scope: SearchScope) -> Self {
        match scope {
            SearchScope::Songs => SearchTab::Songs,
            SearchScope::Videos => SearchTab::Videos,
            SearchScope::Artists => SearchTab::Artists(Vec::new()),
            SearchScope::Albums => SearchTab::Albums(Vec::new()),
            SearchScope::Playlists => SearchTab::Playlists(Vec::new()),
        }
    }

    /// The search scope represented by this tab (inverse of [`from_scope`]).
    pub fn scope(&self) -> SearchScope {
        match self {
            SearchTab::Songs => SearchScope::Songs,
            SearchTab::Videos => SearchScope::Videos,
            SearchTab::Artists(_) => SearchScope::Artists,
            SearchTab::Albums(_) => SearchScope::Albums,
            SearchTab::Playlists(_) => SearchScope::Playlists,
        }
    }

    /// Whether this tab shows the playable track list (vs. card results).
    pub fn is_track_tab(&self) -> bool {
        matches!(self, SearchTab::Songs | SearchTab::Videos)
    }

    /// Number of card results shown by this tab (artists/albums/playlists).
    /// Returns `None` for the track tabs (`Songs`/`Videos`), whose result
    /// count lives in the sibling `Vec<Track>` returned alongside the tab.
    pub fn card_count(&self) -> Option<usize> {
        match self {
            SearchTab::Songs | SearchTab::Videos => None,
            SearchTab::Artists(items) | SearchTab::Albums(items) | SearchTab::Playlists(items) => {
                Some(items.len())
            }
        }
    }
}

/// Provider-scoped search entry point. Dispatches to the per-provider backend
/// and maps results into tracks carrying that provider's id.
pub fn search(
    provider: ProviderId,
    query: &str,
    scope: SearchScope,
    offset: usize,
) -> Result<(Vec<Track>, SearchTab)> {
    match provider {
        ProviderId::YouTube => youtube::search(query, scope, offset),
        ProviderId::SoundCloud => Ok(soundcloud::search(query, scope, offset)),
        ProviderId::MusicBrainz => Ok(musicbrainz::search(query, scope, offset)),
        ProviderId::Local => Ok((Vec::new(), SearchTab::Songs)),
    }
}

/// Pagination for the active search, mirroring [`search`].
pub fn search_more(provider: ProviderId, query: &str, offset: usize) -> Result<Vec<Track>> {
    match provider {
        ProviderId::YouTube => youtube::search_more(query, offset),
        ProviderId::SoundCloud => Ok(soundcloud::search_more(query, offset)),
        ProviderId::MusicBrainz => Ok(musicbrainz::search_more(query, offset)),
        ProviderId::Local => Ok(Vec::new()),
    }
}

/// Drill down into a card (artist/album/playlist) for `provider`. Returns the
/// browsed tracks; empty for providers that don't support browsing.
/// Album metadata shown in the album view header (type + release year).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AlbumMeta {
    pub badge: String,
    pub date: String,
    pub thumbnail: String,
}

pub fn browse(
    provider: ProviderId,
    id: &str,
    kind: &str,
) -> Result<(Vec<Track>, Option<AlbumMeta>)> {
    match provider {
        ProviderId::YouTube => youtube::browse(id, kind),
        ProviderId::SoundCloud => Ok((soundcloud::browse(id, kind)?, None)),
        ProviderId::MusicBrainz => Ok((musicbrainz::browse(id, kind)?, None)),
        ProviderId::Local => Ok((Vec::new(), None)),
    }
}

pub use artist_page::{
    ArtistAlbumCard, ArtistDataKind, ArtistHeader, ArtistKindData, ArtistPage, ArtistPageState,
    ArtistSection, ArtistSectionKind, RelatedArtistCard, SectionContent,
};

/// One artist-page section fetch result: the kind and its data (or error).
/// See [`spawn_artist_kinds_fetch`].
pub struct ArtistKindResult(pub ArtistDataKind, pub Result<ArtistKindData, String>);

/// Fetch each requested kind of `provider`'s artist page on a background
/// thread, delivering one [`ArtistKindResult`] per kind as soon as its data
/// is available (via the returned channel). Providers that get everything in
/// one call (`YouTube`, `MusicBrainz`) send all kinds after that call;
/// `SoundCloud`'s independent endpoints deliver incrementally, so one
/// failing section no longer blanks the others.
pub fn spawn_artist_kinds_fetch(
    provider: ProviderId,
    id: &str,
    kinds: &'static [ArtistDataKind],
) -> std::sync::mpsc::Receiver<ArtistKindResult> {
    let (tx, rx) = std::sync::mpsc::channel();
    let id = id.to_string();
    thread::spawn(move || {
        if provider == ProviderId::SoundCloud {
            soundcloud::fetch_artist_kinds(&id, kinds, &tx);
        } else {
            // One-shot providers: a single request answers every kind; split
            // it into per-kind payloads and send them all at once. YouTube's
            // popular tracks lack durations/view counts; a yt-dlp enrichment
            // pass runs AFTER the fan-out (it takes seconds) and resends
            // Popular so the page never waits on it.
            let result = match provider {
                ProviderId::YouTube => youtube::fetch_artist_page(&id, kinds),
                ProviderId::MusicBrainz => musicbrainz::fetch_artist_page(&id, kinds),
                _ => Ok(artist_page::ArtistPage::default()),
            }
            .map_err(|e| format!("{e:#}"));
            for &kind in kinds {
                let data = match &result {
                    Ok(page) => single_kind_data(page, kind)
                        .map_or_else(|| Err(format!("No {kind:?} data")), Ok),
                    Err(e) => Err(e.clone()),
                };
                let _ = tx.send(ArtistKindResult(kind, data));
            }
            if provider == ProviderId::YouTube {
                if let Ok(page) = &result {
                    if ArtistDataKind::Popular.wanted(kinds) {
                        let mut page = page.clone();
                        youtube::enrich_track_metadata(&mut page.popular);
                        let _ = tx.send(ArtistKindResult(
                            ArtistDataKind::Popular,
                            Ok(ArtistKindData::Popular(page.popular)),
                        ));
                    }
                }
            }
        }
    });
    rx
}

/// Project `page` down to just `kind`'s data. `None` means the page has no
/// data for that kind (e.g. no header) — the kind is skipped rather than
/// cached as covered with empty content.
fn single_kind_data(page: &ArtistPage, kind: ArtistDataKind) -> Option<ArtistKindData> {
    use artist_page::ArtistKindData as D;
    match kind {
        ArtistDataKind::Header => page.header.clone().map(D::Header),
        ArtistDataKind::Popular => Some(D::Popular(page.popular.clone())),
        ArtistDataKind::Albums => Some(D::Albums(page.albums.clone())),
        ArtistDataKind::Playlists => Some(D::Playlists(page.playlists.clone())),
        ArtistDataKind::Related => Some(D::Related(page.related.clone())),
    }
}

/// Resolve an artist name to that provider's artist id (used to lazily open
/// a page section on a provider whose id isn't known yet). Returns
/// `Ok(None)` when no match was found.
pub fn resolve_artist_id(provider: ProviderId, name: &str) -> Result<Option<String>> {
    match provider {
        ProviderId::YouTube => youtube::resolve_artist_id(name),
        ProviderId::SoundCloud => soundcloud::resolve_artist_id(name),
        ProviderId::MusicBrainz => musicbrainz::resolve_artist_id(name),
        ProviderId::Local => Ok(None),
    }
}

/// Build a song radio from a provider id (`YouTube` only supports similarity).
pub fn radio_song(provider: ProviderId, id: &str) -> Result<Vec<Track>> {
    match provider {
        ProviderId::YouTube => youtube::radio_song(id),
        ProviderId::SoundCloud | ProviderId::MusicBrainz | ProviderId::Local => Ok(Vec::new()),
    }
}

/// Build an artist radio from a provider browse id (`YouTube` only).
pub fn radio_artist(provider: ProviderId, id: &str) -> Result<Vec<Track>> {
    match provider {
        ProviderId::YouTube => youtube::radio_artist(id),
        ProviderId::SoundCloud | ProviderId::MusicBrainz | ProviderId::Local => Ok(Vec::new()),
    }
}

/// Resolve a logical track (title/artist) to this provider's track by
/// searching. Returns the full resolved `Track` (carrying that provider's
/// id/url plus its duration/thumbnail/album) so the rich metadata survives
/// the resolution; returns `Ok(None)` if no match was found or the provider
/// cannot resolve (e.g. `Local`), and `Err` only on a genuine provider
/// failure. Drives the "play via / download from [provider]" flow.
pub fn resolve_id(provider: ProviderId, track: &Track) -> Result<Option<Track>> {
    match provider {
        ProviderId::YouTube => youtube::resolve_id(track),
        ProviderId::SoundCloud => soundcloud::resolve_id(track),
        ProviderId::MusicBrainz => musicbrainz::resolve_id(track),
        ProviderId::Local => Ok(None),
    }
}

/// Download a track's audio for `provider` into `download_dir`. The track must
/// already carry that provider's id/url.
pub fn download(provider: ProviderId, track: &Track, download_dir: &str) -> Result<String> {
    match provider {
        ProviderId::YouTube => {
            let url = track
                .provider_url(provider)
                .unwrap_or_else(|| track.primary_url());
            youtube::download(url, download_dir)
        }
        ProviderId::SoundCloud => soundcloud::download(track, download_dir),
        _ => anyhow::bail!("provider does not support downloading"),
    }
}

/// Per-provider identifier/url for a track. A single logical track may carry
/// several of these (one per provider that has resolved it). Provider-specific
/// display metadata (`duration`/`thumbnail`/`album`) lives here so each
/// provider's view of a track is self-contained.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderTrack {
    pub id: String,
    /// For remote providers, the streamable/playable URL. For the `Local`
    /// provider this is the on-disk path of the imported/downloaded audio
    /// file (a local file has no remote URL).
    pub url: String,
    pub artist_id: Option<String>,
    pub duration: u32,
    pub thumbnail: String,
    pub album: Option<crate::types::TrackAlbum>,
    #[serde(default)]
    pub play_count: u64,
}

/// Map of provider id -> that provider's identifier/url for a track.
pub type ProviderMap = HashMap<ProviderId, ProviderTrack>;
