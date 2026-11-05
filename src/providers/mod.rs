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
mod bandcamp;
mod error;
pub(crate) mod home;
mod jellyfin;
pub(crate) mod lastfm;
mod musicbrainz;
mod soundcloud;
mod subsonic;
mod youtube;
pub(crate) mod ytdlp;

use std::{
    borrow::Cow,
    collections::HashMap,
    io::{Read, Write},
    process::{Command, Output, Stdio},
    sync::RwLock,
    thread,
    time::{Duration, Instant},
};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{data::custom_providers::CustomProviderKind, types::Track};

/// Drain a child pipe to completion, returning what was read. Run it on a
/// helper thread while polling `try_wait` so a chatty child can't deadlock
/// on a full pipe (see [`run_command_with_timeout`]).
pub(crate) fn drain_pipe(pipe: Option<impl Read>) -> Vec<u8> {
    pipe.map(|mut p| {
        let mut buf = Vec::new();
        let _ = p.read_to_end(&mut buf);
        buf
    })
    .unwrap_or_default()
}

/// Run a child process to completion, killing it when it exceeds `timeout`.
/// `stdin_input`, when present, is fed to the child's stdin on its own
/// thread. stdout/stderr are drained on helper threads so a chatty child
/// can't deadlock on a full pipe while we poll `try_wait`.
pub(crate) fn run_command_with_stdin_and_timeout(
    cmd: &mut Command,
    stdin_input: Option<Vec<u8>>,
    timeout: Duration,
) -> Result<Output> {
    let program = cmd.get_program().to_string_lossy().into_owned();
    let mut child = cmd
        .stdin(if stdin_input.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .with_context(|| format!("Failed to run {program}"))?;

    // Feed stdin on its own thread; the payload is small (a URL list), so
    // this never blocks, and a child that exits early just breaks the pipe.
    let stdin_handle = stdin_input.map(|input| {
        let stdin = child.stdin.take();
        thread::spawn(move || {
            if let Some(mut stdin) = stdin {
                let _ = stdin.write_all(&input);
            }
        })
    });

    let stdout = child.stdout.take();
    let stderr = child.stderr.take();
    let stdout_handle = thread::spawn(move || drain_pipe(stdout));
    let stderr_handle = thread::spawn(move || drain_pipe(stderr));

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
    if let Some(handle) = stdin_handle {
        let _ = handle.join();
    }
    Ok(Output {
        status,
        stdout,
        stderr,
    })
}

/// Run a short-lived child process with no stdin to completion, killing it
/// when it exceeds `timeout`. Thin wrapper over
/// [`run_command_with_stdin_and_timeout`].
pub(crate) fn run_command_with_timeout(cmd: &mut Command, timeout: Duration) -> Result<Output> {
    run_command_with_stdin_and_timeout(cmd, None, timeout)
}

/// Shared HTTP agent for keyless JSON providers (Last.fm, Bandcamp).
pub(crate) fn http_agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::config::Config::builder()
            .timeout_connect(Some(std::time::Duration::from_secs(15)))
            .timeout_global(Some(std::time::Duration::from_secs(30)))
            .build()
            .new_agent()
    })
}

/// Shared multi-thread tokio runtime for async provider backends
/// (`rsoundcloud`, `bandcamp` crate). One runtime for all providers instead of
/// one per provider.
pub(crate) fn shared_runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
            .expect("Failed to build provider runtime")
    })
}

/// Ensure `dir` exists and join `{id}.mp3` — the shared download-path preamble.
pub(crate) fn download_file_path(dir: &std::path::Path, id: &str) -> std::path::PathBuf {
    let _ = std::fs::create_dir_all(dir);
    dir.join(format!("{id}.mp3"))
}

/// Stream a request's body into `path`, rejecting an empty download. `what`
/// names the source backend in the error text.
pub(crate) fn download_to_file(
    request: ureq::RequestBuilder<ureq::typestate::WithoutBody>,
    path: &std::path::Path,
    what: &str,
) -> Result<()> {
    let mut response = request
        .call()
        .with_context(|| format!("{what} download failed"))?;
    let mut file = std::fs::File::create(path).context("download file not writable")?;
    std::io::copy(&mut response.body_mut().as_reader(), &mut file)
        .with_context(|| format!("{what} download failed"))?;
    file.flush().context("download flush failed")?;
    if file.metadata().is_ok_and(|meta| meta.len() == 0) {
        let _ = std::fs::remove_file(path);
        anyhow::bail!("{what} download was empty");
    }
    Ok(())
}

/// Identifies a music provider. Stored on tracks (which provider is the source of a
/// result) and in configuration (the default stream+download provider).
///
/// `Local` is reserved for user-imported files and is never shown in the
/// provider picker or the default-provider list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum ProviderId {
    #[default]
    YouTube,
    SoundCloud,
    MusicBrainz,
    Bandcamp,
    LastFm,
    Local,
    /// A user-configured provider; the kind names its backend and the id
    /// indexes [`custom_providers`](crate::data::custom_providers).
    Custom(CustomProviderKind, u64),
}

impl Serialize for ProviderId {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.serde_key())
    }
}

impl<'de> Deserialize<'de> for ProviderId {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let key = String::deserialize(deserializer)?;
        Self::from_slug(&key)
            .or_else(|| Self::from_legacy_key(&key))
            .ok_or_else(|| serde::de::Error::custom(format!("unknown provider \"{key}\"")))
    }
}

/// Providers the user switched off in Settings. Mirrored from
/// [`crate::data::config::Config::disabled_providers`] at startup and on every
/// toggle so the static provider lists can filter without threading the config
/// through every call site — the same reason the custom-provider registry is
/// process-global.
static DISABLED_PROVIDERS: RwLock<Vec<ProviderId>> = RwLock::new(Vec::new());

/// Replace the set of providers switched off in Settings, rebuilding the
/// offered lists ([`ProviderId::all`] and friends) with it.
pub fn set_disabled_providers(providers: Vec<ProviderId>) {
    if let Ok(mut disabled) = DISABLED_PROVIDERS.write() {
        *disabled = providers;
    }
    refresh_provider_lists();
}

/// Snapshots of every provider list the UI offers, so the render path reads a
/// ready list instead of re-deriving it from the disabled set and the
/// custom-server registry on every frame. `None` until the first rebuild;
/// readers then fall back to building their list on the spot.
static OFFERED_LISTS: RwLock<Option<OfferedLists>> = RwLock::new(None);

/// One rebuild of the offered-provider lists.
#[derive(Debug, Default)]
struct OfferedLists {
    all: Vec<ProviderId>,
    searchable: Vec<ProviderId>,
    defaultable: Vec<ProviderId>,
    header: Vec<ProviderId>,
    /// The registered custom servers, regardless of the disabled set: the
    /// artist-page section picker offers every kind of server.
    custom_servers: Vec<ProviderId>,
}

/// Rebuild the offered lists. The one refresh point for anything they derive
/// from: the disabled set (via [`set_disabled_providers`]) and the custom-server
/// registry (via `custom_providers::set_registry`).
pub fn refresh_provider_lists() {
    if let Ok(mut offered) = OFFERED_LISTS.write() {
        *offered = Some(build_offered_lists());
    }
}

fn build_offered_lists() -> OfferedLists {
    let custom_servers = crate::data::custom_providers::all_provider_ids();
    let offered_customs: Vec<ProviderId> = custom_servers
        .iter()
        .copied()
        .filter(|provider| provider.enabled())
        .collect();
    let offered = |mut providers: Vec<ProviderId>| {
        providers.extend(offered_customs.iter().copied());
        providers.retain(|provider| provider.enabled());
        providers
    };
    OfferedLists {
        all: offered(ProviderId::builtin_all().to_vec()),
        searchable: offered(
            ProviderId::builtin_all()
                .iter()
                .copied()
                .filter(|provider| *provider != ProviderId::Local)
                .collect(),
        ),
        defaultable: offered(vec![
            ProviderId::YouTube,
            ProviderId::SoundCloud,
            ProviderId::Bandcamp,
        ]),
        header: offered(vec![
            ProviderId::YouTube,
            ProviderId::SoundCloud,
            ProviderId::Bandcamp,
            ProviderId::LastFm,
        ]),
        custom_servers,
    }
}

/// Snapshot of one cached list. Before the first rebuild (tests read earlier)
/// the list is built instead of read, so no caller sees an empty cache.
fn cached_list(select: fn(&OfferedLists) -> &[ProviderId]) -> Vec<ProviderId> {
    if let Ok(offered) = OFFERED_LISTS.read() {
        if let Some(offered) = offered.as_ref() {
            return select(offered).to_vec();
        }
    }
    select(&build_offered_lists()).to_vec()
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
    /// Whether this provider is switched on. A disabled provider is hidden
    /// from every picker, submenu and server-playlist group, while tracks that
    /// already carry its identity keep playing.
    pub fn enabled(self) -> bool {
        DISABLED_PROVIDERS
            .read()
            .map_or(true, |disabled| !disabled.contains(&self))
    }

    /// Directory name for this provider's cached files (thumbnails, stream
    /// cache) and the prefix of its persisted keys. Lowercase ASCII, safe to
    /// use as a path segment.
    pub fn slug(self) -> Cow<'static, str> {
        match self {
            ProviderId::YouTube => Cow::Borrowed("youtube"),
            ProviderId::SoundCloud => Cow::Borrowed("soundcloud"),
            ProviderId::MusicBrainz => Cow::Borrowed("musicbrainz"),
            ProviderId::Bandcamp => Cow::Borrowed("bandcamp"),
            ProviderId::LastFm => Cow::Borrowed("lastfm"),
            ProviderId::Local => Cow::Borrowed("local"),
            ProviderId::Custom(kind, id) => Cow::Owned(format!("{}-{id}", kind.slug())),
        }
    }

    /// Key used for `ProviderId` in serialized maps and settings. Built-in
    /// providers keep their historical variant names so existing files parse,
    /// while custom providers get a stable `<kind>-<id>` slug.
    fn serde_key(self) -> String {
        match self {
            ProviderId::LastFm => "LastFm".to_string(),
            ProviderId::Custom(_, _) => self.slug().into_owned(),
            other => other.name().into_owned(),
        }
    }

    /// Inverse of [`slug`](ProviderId::slug); `None` for unknown directory names.
    pub fn from_slug(slug: &str) -> Option<Self> {
        match slug {
            "youtube" => Some(ProviderId::YouTube),
            "soundcloud" => Some(ProviderId::SoundCloud),
            "musicbrainz" => Some(ProviderId::MusicBrainz),
            "bandcamp" => Some(ProviderId::Bandcamp),
            "lastfm" => Some(ProviderId::LastFm),
            "local" => Some(ProviderId::Local),
            _ => {
                let (kind, id) = slug.rsplit_once('-')?;
                Some(ProviderId::Custom(
                    CustomProviderKind::from_slug(kind)?,
                    id.parse().ok()?,
                ))
            }
        }
    }

    /// Provider-namespaced cache key (`slug:id`) for a song id. Single source
    /// of truth for every cache (stream index, downloads, lyrics,
    /// normalization) so ids from different providers cannot collide.
    pub fn cache_key(self, id: &str) -> String {
        format!("{}:{id}", self.slug())
    }

    /// Inverse of the pre-slug `Debug`-prefixed key segment (e.g. `YouTube`,
    /// `LastFm`); `None` for anything else (including current slugs). Used
    /// only by one-time migrations of persisted keys to [`cache_key`](ProviderId::cache_key).
    pub fn from_legacy_key(prefix: &str) -> Option<Self> {
        match prefix {
            "YouTube" => Some(ProviderId::YouTube),
            "SoundCloud" => Some(ProviderId::SoundCloud),
            "MusicBrainz" => Some(ProviderId::MusicBrainz),
            "Bandcamp" => Some(ProviderId::Bandcamp),
            "LastFm" => Some(ProviderId::LastFm),
            "Local" => Some(ProviderId::Local),
            _ => None,
        }
    }

    /// Untranslated brand name (and the `Local` placeholder). Never show this
    /// to the user: display text comes from [`Strings::provider`].
    ///
    /// [`Strings::provider`]: crate::i18n::Strings::provider
    pub fn name(self) -> Cow<'static, str> {
        match self {
            ProviderId::YouTube => Cow::Borrowed("YouTube"),
            ProviderId::SoundCloud => Cow::Borrowed("SoundCloud"),
            ProviderId::MusicBrainz => Cow::Borrowed("MusicBrainz"),
            ProviderId::Bandcamp => Cow::Borrowed("Bandcamp"),
            ProviderId::LastFm => Cow::Borrowed("Last.fm"),
            ProviderId::Local => Cow::Borrowed("Local"),
            ProviderId::Custom(kind, id) => crate::data::custom_providers::display_name(kind, id)
                .map_or_else(|| Cow::Borrowed(kind.label()), Cow::Owned),
        }
    }

    /// Built-in providers only, without consulting the custom-provider
    /// registry. Used by migrations and by [`all`](ProviderId::all).
    pub fn builtin_all() -> &'static [ProviderId] {
        &[
            ProviderId::YouTube,
            ProviderId::SoundCloud,
            ProviderId::MusicBrainz,
            ProviderId::Bandcamp,
            ProviderId::LastFm,
            ProviderId::Local,
        ]
    }

    /// Every provider, enabled or not: the built-ins plus the registered
    /// custom ones. For "which providers exist" questions (cache lookups,
    /// migrations); the pickers use [`all`](ProviderId::all) and friends so
    /// switched-off providers stay out of the UI.
    pub fn known() -> Vec<ProviderId> {
        let mut providers = Self::builtin_all().to_vec();
        providers.extend(ProviderId::custom_servers());
        providers
    }

    /// Every enabled provider, including `Local` and the registered custom
    /// ones. Prefer the narrower lists ([`searchable`](ProviderId::searchable),
    /// [`defaultable`](ProviderId::defaultable),
    /// [`header_providers`](ProviderId::header_providers)) when only capable
    /// providers make sense. Providers the user switched off in Settings are
    /// left out, so this is the "offered" set rather than every known id.
    pub fn all() -> Vec<ProviderId> {
        cached_list(|lists| &lists.all)
    }

    /// All enabled providers that appear in the search provider picker
    /// (excludes `Local`, which is not a remote search source).
    pub fn searchable() -> Vec<ProviderId> {
        cached_list(|lists| &lists.searchable)
    }

    /// Enabled providers eligible to be the default provider: must support
    /// both streaming and downloading (search-only `MusicBrainz`/`LastFm` and
    /// `Local` are excluded).
    pub fn defaultable() -> Vec<ProviderId> {
        cached_list(|lists| &lists.defaultable)
    }

    /// Enabled providers that can serve an artist-page header worth displaying
    /// (picture + stats + description). Shown in the artist header picker
    /// and pre-fetched as companions by `open_artist`. `MusicBrainz` is
    /// excluded: its header is stats-only with no picture.
    pub fn header_providers() -> Vec<ProviderId> {
        cached_list(|lists| &lists.header)
    }

    /// The registered custom servers, whether switched on or off. Off shows up
    /// only where a section picker needs kinds, not capabilities.
    pub fn custom_servers() -> Vec<ProviderId> {
        cached_list(|lists| &lists.custom_servers)
    }

    pub fn capabilities(self) -> ProviderCaps {
        let a = crate::deps::availability();
        match self {
            ProviderId::YouTube => ProviderCaps {
                search: a.ytmusicapi || a.yt_dlp,
                stream: a.yt_dlp,
                download: a.yt_dlp,
                radio: a.ytmusicapi,
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
            ProviderId::Bandcamp => ProviderCaps {
                search: true,
                stream: true,
                download: true,
                radio: false,
            },
            ProviderId::LastFm => ProviderCaps {
                search: true,
                stream: false,
                download: false,
                radio: true,
            },
            ProviderId::Local => ProviderCaps {
                search: false,
                stream: false,
                download: false,
                radio: false,
            },
            ProviderId::Custom(kind, id) => {
                let configured = crate::data::custom_providers::contains(kind, id);
                ProviderCaps {
                    search: configured,
                    stream: configured,
                    download: configured,
                    radio: configured,
                }
            }
        }
    }

    /// Scopes a search can be run against for this provider. `YouTube` exposes
    /// the full set; others are reduced to the track list plus whatever card
    /// browsing their API supports.
    pub fn supported_scopes(self) -> &'static [SearchScope] {
        match self {
            ProviderId::YouTube => SearchScope::all(),
            ProviderId::SoundCloud | ProviderId::Custom(_, _) => &[
                SearchScope::Songs,
                SearchScope::Artists,
                SearchScope::Albums,
                SearchScope::Playlists,
            ],
            ProviderId::MusicBrainz | ProviderId::Bandcamp | ProviderId::LastFm => &[
                SearchScope::Songs,
                SearchScope::Artists,
                SearchScope::Albums,
            ],
            ProviderId::Local => &[SearchScope::Songs],
        }
    }

    /// The scopes that return playable tracks rather than cards, restricted to
    /// the ones this provider supports (`Songs` everywhere, `Videos` for
    /// `YouTube`). Picker for pick-one-result searches (the Edit Track popup's
    /// "Manual search"), where an artist/album/playlist scope would yield
    /// nothing selectable.
    pub fn track_scopes(self) -> Vec<SearchScope> {
        [SearchScope::Songs, SearchScope::Videos]
            .into_iter()
            .filter(|scope| self.supported_scopes().contains(scope))
            .collect()
    }

    /// Whether a card tab can page past its first page. Backends whose artist/
    /// album/playlist results come from a one-shot call (`YouTube`'s ytmusicapi
    /// helper, Bandcamp's single search page) answer later pages empty instead.
    pub fn pages_cards(self) -> bool {
        matches!(
            self,
            ProviderId::SoundCloud
                | ProviderId::MusicBrainz
                | ProviderId::LastFm
                | ProviderId::Custom(_, _)
        )
    }

    /// Whether this provider streams/ downloads via yt-dlp (`YouTube`,
    /// `SoundCloud`) rather than a direct HTTP file URL.
    pub fn uses_ytdlp(self) -> bool {
        matches!(self, ProviderId::YouTube | ProviderId::SoundCloud)
    }

    /// Whether this provider is a user-configured backend (as opposed to a
    /// built-in source or `Local`).
    pub fn is_custom(self) -> bool {
        matches!(self, ProviderId::Custom(_, _))
    }

    /// The public web page for the artist this provider identifies by `id`.
    /// Providers whose artist ids *are* public addresses (channel ids, MBIDs,
    /// Last.fm artist names) resolve from the id alone; the rest only advertise
    /// a page on their cards, so a track carrying just the id has none.
    pub fn artist_page_url(self, id: &str) -> Option<String> {
        if id.is_empty() {
            return None;
        }
        match self {
            ProviderId::YouTube => Some(format!("https://music.youtube.com/channel/{id}")),
            ProviderId::MusicBrainz => Some(format!("https://musicbrainz.org/artist/{id}")),
            // Last.fm identifies artists by name, which is the last path
            // segment of their page.
            ProviderId::LastFm => Some(format!(
                "https://www.last.fm/music/{}",
                crate::util::urlencode(id)
            )),
            // SoundCloud and Bandcamp address artists by numeric user/band id;
            // `Local` has no artist at all. Custom servers each have their own
            // web UI, so no page can be derived from the API id.
            ProviderId::SoundCloud
            | ProviderId::Bandcamp
            | ProviderId::Local
            | ProviderId::Custom(_, _) => None,
        }
    }

    /// The public web page for the album this provider identifies by `id` —
    /// the id a track carries in its `TrackAlbum`. Only providers whose album
    /// ids *are* public addresses resolve; the rest advertise a page on their
    /// cards alone. `MusicBrainz` tracks name a release group while its album
    /// cards name a release, so the two spellings differ.
    pub fn album_page_url(self, id: &str) -> Option<String> {
        if id.is_empty() {
            return None;
        }
        match self {
            // The same browse id the album cards carry, so a track's album page
            // and an album card's page are one address.
            ProviderId::YouTube => Some(format!("https://music.youtube.com/browse/{id}")),
            ProviderId::MusicBrainz => Some(format!("https://musicbrainz.org/release-group/{id}")),
            // SoundCloud and Bandcamp carry numeric ids; Last.fm tracks carry
            // no album at all.
            ProviderId::SoundCloud
            | ProviderId::Bandcamp
            | ProviderId::LastFm
            | ProviderId::Local
            | ProviderId::Custom(_, _) => None,
        }
    }

    /// The public web page for one of this provider's tracks, when one can be
    /// derived from the data the provider handed us. `None` when the stored
    /// `url` is a media file or a local path and no page address is
    /// reconstructible from the id.
    pub fn track_page_url(self, pt: &ProviderTrack) -> Option<String> {
        match self {
            // These already store the page address: YouTube's watch page,
            // SoundCloud's permalink, Last.fm's track page.
            ProviderId::YouTube | ProviderId::SoundCloud | ProviderId::LastFm => {
                (!pt.url.is_empty()).then(|| pt.url.clone())
            }
            ProviderId::MusicBrainz => {
                (!pt.id.is_empty()).then(|| format!("https://musicbrainz.org/track/{}", pt.id))
            }
            // Bandcamp's `url` is a direct MP3 and its page slug is not
            // derivable from the numeric track id; `Local` has no web page.
            // A custom provider's `url` is a token-authenticated stream endpoint.
            ProviderId::Bandcamp | ProviderId::Local | ProviderId::Custom(_, _) => None,
        }
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

/// Provider-agnostic card result (artist/album/playlist drill-down). `url` is
/// the public page the provider advertised for the card, empty when it exposes
/// none (the card is then not offered "Open in browser").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct CardData {
    pub id: String,
    pub title: String,
    pub subtitle: String,
    pub thumbnail: String,
    pub url: String,
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

    /// Append the next page of card results. A page of a different kind (a
    /// provider that answered a card query with tracks) is dropped, so the
    /// visible tab always stays a single coherent list.
    pub fn append_cards(&mut self, page: Self) {
        match (self, page) {
            (SearchTab::Artists(items), SearchTab::Artists(more))
            | (SearchTab::Albums(items), SearchTab::Albums(more))
            | (SearchTab::Playlists(items), SearchTab::Playlists(more)) => items.extend(more),
            _ => {}
        }
    }
}

/// Calls `fn_name(args)` on the backend module of the custom provider's kind;
/// the sub-module paths are fully qualified so sibling modules can use it too.
macro_rules! custom_dispatch {
    ($custom_kind:expr, $fn_name:ident($($arg:expr),* $(,)?)) => {
        match $custom_kind {
            $crate::providers::CustomProviderKind::Subsonic => {
                $crate::providers::subsonic::$fn_name($($arg),*)
            }
            $crate::providers::CustomProviderKind::Jellyfin => {
                $crate::providers::jellyfin::$fn_name($($arg),*)
            }
        }
    };
}
pub(crate) use custom_dispatch;

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
        ProviderId::Bandcamp => bandcamp::search(query, scope, offset),
        ProviderId::LastFm => lastfm::search(query, scope, offset),
        ProviderId::Custom(kind, _) => {
            custom_dispatch!(kind, search(provider, query, scope, offset))
        }
        ProviderId::Local => Ok((Vec::new(), SearchTab::Songs)),
    }
}

/// Pagination for the active search, mirroring [`search`]. The track tabs use
/// each backend's own append path (`YouTube` goes back to yt-dlp,
/// Bandcamp/Last.fm re-run their paged track search), while a card tab is just
/// the next page of the same scope: every provider that offers artist/album/
/// playlist results already takes an offset there. `YouTube`'s card tabs come
/// from ytmusicapi, which serves one page only, so it answers with an empty
/// page and the tab is marked exhausted.
pub fn search_more(
    provider: ProviderId,
    query: &str,
    scope: SearchScope,
    offset: usize,
) -> Result<(Vec<Track>, SearchTab)> {
    if !matches!(scope, SearchScope::Songs | SearchScope::Videos) {
        return match provider {
            ProviderId::YouTube | ProviderId::Local => {
                Ok((Vec::new(), SearchTab::from_scope(scope)))
            }
            _ => search(provider, query, scope, offset),
        };
    }
    let tracks = match provider {
        ProviderId::YouTube => youtube::search_more(query, offset)?,
        ProviderId::SoundCloud => soundcloud::search_more(query, offset),
        ProviderId::MusicBrainz => musicbrainz::search_more(query, offset),
        ProviderId::Bandcamp => bandcamp::search_more(query, offset)?,
        ProviderId::LastFm => lastfm::search_more(query, offset)?,
        ProviderId::Custom(kind, _) => {
            custom_dispatch!(kind, search_more(provider, query, offset))?
        }
        ProviderId::Local => Vec::new(),
    };
    Ok((tracks, SearchTab::from_scope(scope)))
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

/// One playlist that lives on a user-configured server, as the server's own
/// playlist list reports it. The contents are fetched separately through
/// [`browse`], so this stays a small, cacheable index entry.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServerPlaylist {
    pub id: String,
    pub name: String,
    pub track_count: usize,
    /// The playlist's cover-art URL, empty when the server has none.
    pub thumbnail: String,
}

/// Outcome of writing tracks into a server playlist. `skipped` counts the
/// tracks that carry no id on that server (they stay untouched), so the UI can
/// report honestly instead of pretending everything landed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct PlaylistWrite {
    pub added: usize,
    pub skipped: usize,
}

/// Whether this provider has server-side playlists the app can list and edit.
pub fn playlists_supported(provider: ProviderId) -> bool {
    provider.is_custom()
}

/// The ids that address `tracks` on `provider`, plus how many tracks had none.
/// Playlists are written by server id, so a track that was never resolved to
/// this server cannot be added to one of its playlists.
pub(crate) fn playlist_track_ids(provider: ProviderId, tracks: &[Track]) -> (Vec<String>, usize) {
    let mut ids = Vec::with_capacity(tracks.len());
    let mut skipped = 0;
    for track in tracks {
        match track.provider_id(provider) {
            Some(id) if !id.is_empty() => ids.push(id.to_string()),
            _ => skipped += 1,
        }
    }
    (ids, skipped)
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
        ProviderId::Bandcamp => bandcamp::browse(id, kind),
        ProviderId::LastFm => lastfm::browse(id, kind),
        ProviderId::Custom(custom_kind, _) => {
            custom_dispatch!(custom_kind, browse(provider, id, kind))
        }
        ProviderId::Local => Ok((Vec::new(), None)),
    }
}

/// Every playlist on `provider`'s server (empty for providers without one).
pub fn list_playlists(provider: ProviderId) -> Result<Vec<ServerPlaylist>> {
    match provider {
        ProviderId::Custom(kind, _) => custom_dispatch!(kind, list_playlists(provider)),
        _ => Ok(Vec::new()),
    }
}

pub fn add_to_playlist(
    provider: ProviderId,
    playlist_id: &str,
    tracks: &[Track],
) -> Result<PlaylistWrite> {
    match provider {
        ProviderId::Custom(kind, _) => {
            custom_dispatch!(kind, add_to_playlist(provider, playlist_id, tracks))
        }
        _ => anyhow::bail!("provider does not support playlists"),
    }
}

pub fn remove_from_playlist(
    provider: ProviderId,
    playlist_id: &str,
    positions: &[usize],
) -> Result<usize> {
    match provider {
        ProviderId::Custom(kind, _) => {
            custom_dispatch!(kind, remove_from_playlist(provider, playlist_id, positions))
        }
        _ => anyhow::bail!("provider does not support playlists"),
    }
}

/// Rewrite `playlist_id` so its tracks are in the order of `tracks` (which must
/// be the playlist's current contents in the wanted order).
pub fn reorder_playlist(provider: ProviderId, playlist_id: &str, tracks: &[Track]) -> Result<()> {
    match provider {
        ProviderId::Custom(kind, _) => {
            custom_dispatch!(kind, reorder_playlist(provider, playlist_id, tracks))
        }
        _ => anyhow::bail!("provider does not support playlists"),
    }
}

pub fn create_playlist(
    provider: ProviderId,
    name: &str,
    tracks: &[Track],
) -> Result<ServerPlaylist> {
    match provider {
        ProviderId::Custom(kind, _) => {
            custom_dispatch!(kind, create_playlist(provider, name, tracks))
        }
        _ => anyhow::bail!("provider does not support playlists"),
    }
}

pub fn rename_playlist(provider: ProviderId, playlist_id: &str, name: &str) -> Result<()> {
    match provider {
        ProviderId::Custom(kind, _) => {
            custom_dispatch!(kind, rename_playlist(provider, playlist_id, name))
        }
        _ => anyhow::bail!("provider does not support playlists"),
    }
}

pub fn delete_playlist(provider: ProviderId, playlist_id: &str) -> Result<()> {
    match provider {
        ProviderId::Custom(kind, _) => {
            custom_dispatch!(kind, delete_playlist(provider, playlist_id))
        }
        _ => anyhow::bail!("provider does not support playlists"),
    }
}

pub use artist_page::{
    ArtistAlbumCard, ArtistDataKind, ArtistHeader, ArtistKindData, ArtistPage, ArtistPageState,
    ArtistSection, ArtistSectionKind, RelatedArtistCard, SectionContent,
};
pub use error::{ErrorKind, ProviderError};
pub use home::{
    fetch_home, home_providers, sort_sections, HomeCard, HomeSection, HomeSectionData,
    HomeSectionKey, HomeSectionKind,
};

/// One artist-page section fetch result: the kind and its data (or error).
/// See [`spawn_artist_kinds_fetch`].
pub struct ArtistKindResult(
    pub ArtistDataKind,
    pub Result<ArtistKindData, ProviderError>,
);

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
            // it into per-kind payloads and send them all at once. When
            // YouTube's popular tracks still lack durations/view counts, a
            // yt-dlp enrichment pass runs AFTER the fan-out (it takes
            // seconds) and resends Popular so the page never waits on it.
            let result = match provider {
                ProviderId::YouTube => youtube::fetch_artist_page(&id, kinds),
                ProviderId::MusicBrainz => musicbrainz::fetch_artist_page(&id, kinds),
                ProviderId::Bandcamp => bandcamp::fetch_artist_page(&id, kinds),
                ProviderId::LastFm => lastfm::fetch_artist_page(&id, kinds),
                ProviderId::Custom(kind, _) => {
                    custom_dispatch!(kind, fetch_artist_page(provider, &id, kinds))
                }
                _ => Ok(artist_page::ArtistPage::default()),
            }
            .map_err(ProviderError::from);
            for &kind in kinds {
                let data = match &result {
                    Ok(page) => single_kind_data(page, kind).map_or_else(
                        || {
                            Err(ProviderError::new(
                                ErrorKind::Other,
                                format!("No {kind:?} data"),
                            ))
                        },
                        Ok,
                    ),
                    Err(e) => Err(e.clone()),
                };
                let _ = tx.send(ArtistKindResult(kind, data));
            }
            if provider == ProviderId::YouTube {
                if let Ok(page) = &result {
                    if ArtistDataKind::Popular.wanted(kinds)
                        && youtube::needs_enrichment(&page.popular)
                    {
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

/// Probe a candidate custom-provider configuration without saving it. Used by
/// the Settings form's "Test connection" button.
pub fn test_custom_provider(
    kind: CustomProviderKind,
    server_url: &str,
    username: &str,
    password: &str,
) -> Result<String> {
    custom_dispatch!(kind, test_connection(server_url, username, password))
}

/// Resolve an artist name to that provider's artist id (used to lazily open
/// a page section on a provider whose id isn't known yet). Returns
/// `Ok(None)` when no match was found.
pub fn resolve_artist_id(provider: ProviderId, name: &str) -> Result<Option<String>> {
    match provider {
        ProviderId::YouTube => youtube::resolve_artist_id(name),
        ProviderId::SoundCloud => soundcloud::resolve_artist_id(name),
        ProviderId::MusicBrainz => musicbrainz::resolve_artist_id(name),
        ProviderId::Bandcamp => bandcamp::resolve_artist_id(name),
        ProviderId::LastFm => lastfm::resolve_artist_id(name),
        ProviderId::Custom(kind, _) => custom_dispatch!(kind, resolve_artist_id(provider, name)),
        ProviderId::Local => Ok(None),
    }
}

/// Build a song radio from a provider id.
pub fn radio_song(provider: ProviderId, id: &str) -> Result<Vec<Track>> {
    match provider {
        ProviderId::YouTube => youtube::radio_song(id),
        ProviderId::LastFm => lastfm::radio_song(id),
        ProviderId::Custom(kind, _) => custom_dispatch!(kind, radio_song(provider, id)),
        ProviderId::SoundCloud
        | ProviderId::MusicBrainz
        | ProviderId::Bandcamp
        | ProviderId::Local => Ok(Vec::new()),
    }
}

/// Build an artist radio from a provider browse id.
pub fn radio_artist(provider: ProviderId, id: &str) -> Result<Vec<Track>> {
    match provider {
        ProviderId::YouTube => youtube::radio_artist(id),
        ProviderId::LastFm => lastfm::radio_artist(id),
        ProviderId::Custom(kind, _) => custom_dispatch!(kind, radio_artist(provider, id)),
        ProviderId::SoundCloud
        | ProviderId::MusicBrainz
        | ProviderId::Bandcamp
        | ProviderId::Local => Ok(Vec::new()),
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
        ProviderId::Bandcamp => bandcamp::resolve_id(track),
        ProviderId::LastFm => lastfm::resolve_id(track),
        ProviderId::Custom(kind, _) => custom_dispatch!(kind, resolve_id(provider, track)),
        ProviderId::Local => Ok(None),
    }
}

/// Progress events from the `YouTube` player-client race that precedes
/// streaming/downloading. Emitted on the caller's channel (`BackendResult`
/// for downloads, `PlayerState` for streams) and surfaced as toasts.
#[derive(Debug, Clone)]
pub enum ClientEvent {
    Resolving,
    Resolved(String),
    Unavailable,
}

/// Download a track's audio for `provider` into `download_dir`. The track must
/// already carry that provider's id/url. `emit` receives the player-client
/// race events (`YouTube` only; other providers ignore it).
pub fn download(
    provider: ProviderId,
    track: &Track,
    download_dir: &std::path::Path,
    emit: &dyn Fn(ClientEvent),
) -> Result<String> {
    match provider {
        ProviderId::YouTube => {
            let url = track
                .provider_url(provider)
                .unwrap_or_else(|| track.primary_url());
            youtube::download(url, download_dir, emit)
        }
        ProviderId::SoundCloud => soundcloud::download(track, download_dir),
        ProviderId::Bandcamp => bandcamp::download(track, download_dir),
        ProviderId::Custom(kind, _) => {
            custom_dispatch!(kind, download(provider, track, download_dir))
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn album_pages_resolve_only_where_the_id_is_a_public_address() {
        // The browse id a YouTube track carries is the same one its album
        // cards carry, so both resolve to one address.
        assert_eq!(
            ProviderId::YouTube.album_page_url("MPREb_abc"),
            Some("https://music.youtube.com/browse/MPREb_abc".into())
        );
        // MusicBrainz tracks carry the release group, not the release.
        assert_eq!(
            ProviderId::MusicBrainz.album_page_url("rg-1"),
            Some("https://musicbrainz.org/release-group/rg-1".into())
        );
        for p in [
            ProviderId::SoundCloud,
            ProviderId::Bandcamp,
            ProviderId::LastFm,
            ProviderId::Local,
        ] {
            assert_eq!(p.album_page_url("12345"), None, "{p:?}");
        }
        assert_eq!(ProviderId::YouTube.album_page_url(""), None);
    }

    #[test]
    fn artist_pages_resolve_only_where_the_id_is_a_public_address() {
        assert_eq!(
            ProviderId::YouTube.artist_page_url("UCabc"),
            Some("https://music.youtube.com/channel/UCabc".into())
        );
        assert_eq!(
            ProviderId::MusicBrainz.artist_page_url("mbid-1"),
            Some("https://musicbrainz.org/artist/mbid-1".into())
        );
        // Last.fm identifies artists by name, so the id needs encoding.
        assert_eq!(
            ProviderId::LastFm.artist_page_url("Sigur Rós"),
            Some("https://www.last.fm/music/Sigur+R%C3%B3s".into())
        );
        // Numeric user/band ids address nothing on the web.
        assert_eq!(ProviderId::SoundCloud.artist_page_url("12345"), None);
        assert_eq!(ProviderId::Bandcamp.artist_page_url("12345"), None);
        assert_eq!(ProviderId::Local.artist_page_url("12345"), None);
        assert_eq!(ProviderId::YouTube.artist_page_url(""), None);
    }

    fn card(id: &str) -> CardData {
        CardData {
            id: id.into(),
            title: id.into(),
            ..Default::default()
        }
    }

    #[test]
    fn a_card_page_only_extends_its_own_tab() {
        let mut tab = SearchTab::Artists(vec![card("a")]);
        tab.append_cards(SearchTab::Artists(vec![card("b")]));
        assert_eq!(tab.card_count(), Some(2));

        // A page of another kind cannot land in this list.
        tab.append_cards(SearchTab::Albums(vec![card("c")]));
        tab.append_cards(SearchTab::Songs);
        assert_eq!(tab.card_count(), Some(2));
        assert!(matches!(tab, SearchTab::Artists(_)));
    }

    #[test]
    fn card_pagination_is_limited_to_backends_that_can_offset_it() {
        for p in [
            ProviderId::SoundCloud,
            ProviderId::MusicBrainz,
            ProviderId::LastFm,
            ProviderId::Custom(CustomProviderKind::Subsonic, 1),
            ProviderId::Custom(CustomProviderKind::Jellyfin, 1),
        ] {
            assert!(p.pages_cards(), "{p:?}");
        }
        for p in [ProviderId::YouTube, ProviderId::Bandcamp, ProviderId::Local] {
            assert!(!p.pages_cards(), "{p:?}");
        }

        // YouTube's card tabs come from a one-shot ytmusicapi call, so asking
        // for another page answers empty (which terminates the paging) without
        // reaching yt-dlp.
        let (tracks, tab) =
            search_more(ProviderId::YouTube, "night drive", SearchScope::Artists, 10).unwrap();
        assert!(tracks.is_empty());
        assert_eq!(tab.card_count(), Some(0));
        assert_eq!(tab.scope(), SearchScope::Artists);
    }
}
