//! `SoundCloud` provider. Search, album/playlist discovery, and set browsing
//! all go through the `rsoundcloud` crate, which wraps `SoundCloud`'s internal
//! v2 API and needs no API key (it scrapes a `client_id` on construction).
//!
//! Actual streaming and downloading still use `yt-dlp` (the existing
//! `YouTube`-style pipeline): `rsoundcloud` only exposes HLS/progressive
//! transcodings that are `client_id`/authorization-gated, not a plain stream
//! URL, whereas `yt-dlp` resolves a track's `permalink_url` directly. So every
//! `Track` produced here carries a `SoundCloud` id + `permalink_url` so the
//! playback/download path can hand that URL to `yt-dlp`.

use super::ytdlp;
use crate::providers::{ArtistHeader, CardData, ProviderId, SearchScope, SearchTab};
use crate::types::Track;
use anyhow::{Context, Result};
use rsoundcloud::{
    models::playlist::AlbumPlaylist, models::track::BasicTrack, models::track::Track as SCTrack,
    models::user::User, CollectionParams, PlaylistsApi, ResourceId, SearchApi, SoundCloudClient,
    UsersApi,
};
use std::sync::OnceLock;
impl From<&AlbumPlaylist> for CardData {
    fn from(ap: &AlbumPlaylist) -> Self {
        CardData {
            id: ap.album_playlist.id.to_string(),
            title: ap.album_playlist.title.clone(),
            subtitle: ap.user.user.username.clone(),
            thumbnail: ap.album_playlist.artwork_url.clone().unwrap_or_default(),
        }
    }
}

/// A `rsoundcloud` track becomes a playable `Track`. The `permalink_url` +
/// numeric `id` let the existing `yt-dlp` stream/download path play it, so we
/// don't need `SoundCloud`'s (auth-gated) stream URLs.
impl From<&SCTrack> for Track {
    fn from(t: &SCTrack) -> Self {
        let mut track = Track::from_provider(
            ProviderId::SoundCloud,
            t.track.id.to_string(),
            t.track.permalink_url.clone(),
            t.track.title.clone(),
            t.user.username.clone(),
            (t.track.duration.max(0) as u64 / 1000) as u32,
            t.track
                .artwork_url
                .clone()
                .unwrap_or_else(|| t.user.avatar_url.clone()),
            None,
            Some(t.user.id.to_string()),
        );
        if let Some(pt) = track.providers.get_mut(&ProviderId::SoundCloud) {
            pt.play_count = t.track.playback_count.unwrap_or(0).max(0) as u64;
        }
        track
    }
}

/// Map an `rsoundcloud` user (artist) into a card for the Artists search tab.
impl From<&User> for CardData {
    fn from(u: &User) -> Self {
        CardData {
            id: u.user.id.to_string(),
            title: u.user.username.clone(),
            subtitle: u.user.full_name.clone(),
            thumbnail: u.user.avatar_url.clone(),
        }
    }
}

/// A `rsoundcloud` basic track (from an artist's track list) becomes a
/// playable `Track`, carrying the `permalink_url` for `yt-dlp` playback.
impl From<&BasicTrack> for Track {
    fn from(t: &BasicTrack) -> Self {
        let mut track = Track::from_provider(
            ProviderId::SoundCloud,
            t.track.id.to_string(),
            t.track.permalink_url.clone(),
            t.track.title.clone(),
            t.user.username.clone(),
            (t.track.duration.max(0) as u64 / 1000) as u32,
            t.track.artwork_url.clone().unwrap_or_default(),
            None,
            Some(t.user.id.to_string()),
        );
        if let Some(pt) = track.providers.get_mut(&ProviderId::SoundCloud) {
            pt.play_count = t.track.playback_count.unwrap_or(0).max(0) as u64;
        }
        track
    }
}

/// Run an async `rsoundcloud` call to completion on the shared current-thread
/// tokio runtime. The provider backends run on plain `std::thread`s, but
/// `rsoundcloud` is async, so we drive it with one lazily-built runtime reused
/// across calls (building a runtime per call is needlessly expensive).
fn block_on_sc<F, T, E>(fut: F) -> Result<T>
where
    F: std::future::Future<Output = std::result::Result<T, E>>,
    E: std::fmt::Debug,
{
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    let rt = RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("Failed to build SoundCloud runtime")
    });
    rt.block_on(fut)
        .map_err(|e| anyhow::anyhow!("SoundCloud API error: {e:?}"))
}

/// A ready `SoundCloudClient` (scrapes a `client_id` on first use).
async fn sc_client() -> rsoundcloud::ClientResult<SoundCloudClient> {
    SoundCloudClient::default().await
}

fn search_page(offset: usize) -> CollectionParams {
    CollectionParams::new(
        Some(crate::theme::SEARCH_PAGE_SIZE as u32),
        Some(offset as u32),
    )
}

pub fn search(query: &str, scope: SearchScope, offset: usize) -> (Vec<Track>, SearchTab) {
    match scope {
        SearchScope::Songs => match search_tracks(query, offset) {
            Ok(tracks) => (tracks, SearchTab::Songs),
            Err(e) => {
                tracing::warn!("SoundCloud song search failed: {e:#}");
                (Vec::new(), SearchTab::Songs)
            }
        },
        SearchScope::Artists => match search_users(query, offset) {
            Ok(cards) => (Vec::new(), SearchTab::Artists(cards)),
            Err(e) => {
                tracing::warn!("SoundCloud artist search failed: {e:#}");
                (Vec::new(), SearchTab::Artists(Vec::new()))
            }
        },
        SearchScope::Albums => match search_sets(query, offset, true) {
            Ok(cards) => (Vec::new(), SearchTab::Albums(cards)),
            Err(e) => {
                tracing::warn!("SoundCloud album search failed: {e:#}");
                (Vec::new(), SearchTab::Albums(Vec::new()))
            }
        },
        SearchScope::Playlists => match search_sets(query, offset, false) {
            Ok(cards) => (Vec::new(), SearchTab::Playlists(cards)),
            Err(e) => {
                tracing::warn!("SoundCloud playlist search failed: {e:#}");
                (Vec::new(), SearchTab::Playlists(Vec::new()))
            }
        },
        // Defensive fallback for any scope SoundCloud doesn't expose (e.g.
        // Videos): treat it as a song search.
        SearchScope::Videos => match search_tracks(query, offset) {
            Ok(tracks) => (tracks, SearchTab::Songs),
            Err(e) => {
                tracing::warn!("SoundCloud song search failed: {e:#}");
                (Vec::new(), SearchTab::Songs)
            }
        },
    }
}

fn search_tracks(query: &str, offset: usize) -> Result<Vec<Track>> {
    let tracks = block_on_sc(async {
        let client = sc_client().await?;
        client
            .search_tracks(query.to_string(), search_page(offset))
            .await
    })?;
    Ok(tracks.iter().map(Track::from).collect())
}

fn search_users(query: &str, offset: usize) -> Result<Vec<CardData>> {
    let cards = block_on_sc(async {
        let client = sc_client().await?;
        client
            .search_users(query.to_string(), search_page(offset))
            .await
    })?;
    Ok(cards.iter().map(CardData::from).collect())
}

/// Search albums or playlists (both return the same `AlbumPlaylist` shape and
/// map through `From<&AlbumPlaylist>`, so they share one body).
fn search_sets(query: &str, offset: usize, albums: bool) -> Result<Vec<CardData>> {
    let cards = block_on_sc(async {
        let client = sc_client().await?;
        if albums {
            client
                .search_albums(query.to_string(), search_page(offset))
                .await
        } else {
            client
                .search_playlists(query.to_string(), search_page(offset))
                .await
        }
    })?;
    Ok(cards.iter().map(CardData::from).collect())
}

pub fn search_more(query: &str, offset: usize) -> Vec<Track> {
    search_tracks(query, offset).unwrap_or_default()
}

/// Browse a `SoundCloud` artist, album, or playlist by id.
/// - `"artist"` → the artist's tracks (mirrors `YouTube`'s artist drill-down)
/// - `"album"` / `"playlist"` → the set's tracks (both share one endpoint)
pub fn browse(id: &str, kind: &str) -> Result<Vec<Track>> {
    let parsed: u64 = id
        .parse()
        .with_context(|| format!("Invalid SoundCloud id: {id}"))?;
    if kind == "artist" {
        let tracks = block_on_sc(async {
            let client = sc_client().await?;
            client.get_user_tracks(ResourceId::Id(parsed)).await
        })?;
        Ok(tracks.iter().map(Track::from).collect())
    } else {
        let tracks = block_on_sc(async {
            let client = sc_client().await?;
            client.get_playlist_tracks(ResourceId::Id(parsed)).await
        })?;
        Ok(tracks.iter().map(Track::from).collect())
    }
}

/// Per-endpoint section fetches; each returns just its section's data.
async fn header_data(client: &SoundCloudClient, id: u64) -> Result<ArtistHeader> {
    let user = retry(3, &mut || client.get_user(ResourceId::Id(id)))
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    Ok(ArtistHeader {
        image: user.user.avatar_url.clone(),
        stats: vec![(
            "SoundCloud Followers".to_string(),
            crate::util::format_count(user.user.followers_count.max(0) as u64),
        )],
        description: user.description.clone().unwrap_or_default(),
    })
}

async fn popular_tracks(client: &SoundCloudClient, id: u64) -> Result<Vec<Track>> {
    let tracks = retry(3, &mut || {
        client.get_user_popular_tracks(ResourceId::Id(id))
    })
    .await
    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    Ok(tracks.iter().map(Track::from).collect())
}

async fn album_cards(
    client: &SoundCloudClient,
    id: u64,
) -> Result<Vec<crate::providers::ArtistAlbumCard>> {
    let albums = retry(3, &mut || client.get_user_albums(ResourceId::Id(id)))
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    Ok(albums
        .iter()
        .map(|ap| crate::providers::ArtistAlbumCard {
            id: ap.album_playlist.id.to_string(),
            title: ap.album_playlist.title.clone(),
            date: String::new(),
            badge: "Album".to_string(),
            thumbnail: ap.album_playlist.artwork_url.clone().unwrap_or_default(),
        })
        .collect())
}

async fn playlist_cards(client: &SoundCloudClient, id: u64) -> Result<Vec<CardData>> {
    let playlists = retry(3, &mut || client.get_user_playlists(ResourceId::Id(id)))
        .await
        .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    Ok(playlists
        .iter()
        .map(|ap| CardData {
            id: ap.album_playlist.id.to_string(),
            title: ap.album_playlist.title.clone(),
            subtitle: String::new(),
            thumbnail: ap.album_playlist.artwork_url.clone().unwrap_or_default(),
        })
        .collect())
}

async fn related_artists(
    client: &SoundCloudClient,
    id: u64,
) -> Result<Vec<crate::providers::RelatedArtistCard>> {
    let related = retry(3, &mut || {
        client.get_user_related_artists(ResourceId::Id(id))
    })
    .await
    .map_err(|e| anyhow::anyhow!("{e:?}"))?;
    Ok(related
        .iter()
        .map(|u| crate::providers::RelatedArtistCard {
            id: u.user.id.to_string(),
            name: u.user.username.clone(),
            stat: crate::util::format_count(u.user.followers_count.max(0) as u64),
            thumbnail: u.user.avatar_url.clone(),
        })
        .collect())
}

pub fn fetch_artist_kinds(
    id: &str,
    kinds: &[crate::providers::ArtistDataKind],
    tx: &std::sync::mpsc::Sender<crate::providers::ArtistKindResult>,
) {
    use crate::providers::{ArtistDataKind as K, ArtistKindData as D, ArtistKindResult};
    let Ok(parsed) = id.parse::<u64>() else {
        let msg = format!("Invalid SoundCloud id: {id}");
        for &kind in kinds {
            let _ = tx.send(ArtistKindResult(kind, Err(msg.clone())));
        }
        return;
    };
    // Client init failing must fail every requested kind — an early return
    // would leave the channel empty and the sections stuck in Loading.
    let client = match block_on_sc(sc_client()) {
        Ok(client) => std::sync::Arc::new(client),
        Err(e) => {
            let msg = format!("{e:#}");
            for &kind in kinds {
                let _ = tx.send(ArtistKindResult(kind, Err(msg.clone())));
            }
            return;
        }
    };
    let _ = block_on_sc(async {
        let mut handles = Vec::new();
        macro_rules! kind_task {
            ($variant:ident, $kind:expr, $fetch:expr) => {
                if $kind.wanted(kinds) {
                    let (client, tx) = (client.clone(), tx.clone());
                    let kind = $kind;
                    handles.push((
                        kind,
                        tokio::spawn(async move {
                            let result = $fetch(&client, parsed)
                                .await
                                .map(D::$variant)
                                .map_err(|e| format!("{e:#}"));
                            let _ = tx.send(ArtistKindResult(kind, result));
                        }),
                    ));
                }
            };
        }
        kind_task!(Header, K::Header, header_data);
        kind_task!(Popular, K::Popular, popular_tracks);
        kind_task!(Albums, K::Albums, album_cards);
        kind_task!(Playlists, K::Playlists, playlist_cards);
        kind_task!(Related, K::Related, related_artists);
        for (kind, handle) in handles {
            // A panicked task would otherwise strand its section in Loading.
            if handle.await.is_err() {
                let _ = tx.send(ArtistKindResult(
                    kind,
                    Err("SoundCloud request failed".to_string()),
                ));
            }
        }
        Ok::<(), rsoundcloud::ClientError>(())
    });
}

/// Run one request future with up to `attempts` tries and linear backoff —
/// `SoundCloud`'s internal API answers 403/429 under bursts.
async fn retry<T, Fut>(attempts: u32, f: &mut impl FnMut() -> Fut) -> rsoundcloud::ClientResult<T>
where
    Fut: std::future::Future<Output = rsoundcloud::ClientResult<T>>,
{
    for attempt in 0..attempts {
        match f().await {
            Ok(v) => return Ok(v),
            Err(e) if attempt + 1 < attempts => {
                tracing::warn!(
                    "SoundCloud request failed ({e:?}); retrying in {}s",
                    attempt + 1
                );
                tokio::time::sleep(std::time::Duration::from_secs(u64::from(attempt) + 1)).await;
            }
            Err(e) => return Err(e),
        }
    }
    unreachable!()
}

/// Resolve an artist name to a `SoundCloud` user id via user search. Returns
/// `Ok(None)` when nothing matched.
pub fn resolve_artist_id(name: &str) -> Result<Option<String>> {
    Ok(search_users(name, 0)?.into_iter().next().map(|c| c.id))
}

/// Resolve a logical track to a `SoundCloud` track via search. Returns the
/// full resolved `Track` (carrying id/url plus duration/thumbnail/album) so
/// the rich metadata survives the resolution, or `Ok(None)` when no match is
/// found (not an error).
pub fn resolve_id(track: &Track) -> Result<Option<Track>> {
    let query = track.search_query();
    let items = search_tracks(&query, 0)?;
    Ok(items.into_iter().next())
}

/// Download the track's audio. The track must carry a `SoundCloud` id/url.
/// Streaming/downloading is the one part that stays on `yt-dlp` because
/// `rsoundcloud` doesn't expose a plain stream URL.
pub fn download(track: &Track, download_dir: &str) -> Result<String> {
    let url = track
        .provider_url(ProviderId::SoundCloud)
        .unwrap_or_else(|| track.primary_url())
        .to_string();
    let id = track
        .provider_id(ProviderId::SoundCloud)
        .unwrap_or("download");
    let dir = std::path::Path::new(download_dir);
    let _ = std::fs::create_dir_all(dir);
    let output_path = dir.join(format!("{id}.mp3"));
    ytdlp::download_audio(&url, output_path.to_string_lossy().as_ref(), &[])
}
