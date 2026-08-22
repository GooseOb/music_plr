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

use crate::providers::{CardData, ProviderId, SearchScope, SearchTab};
use crate::types::{Track, TrackAlbum};
use anyhow::{Context, Result};
use rsoundcloud::{
    models::playlist::AlbumPlaylist, models::track::BasicTrack, models::track::Track as SCTrack,
    models::user::User, CollectionParams, PlaylistsApi, ResourceId, SearchApi, SoundCloudClient,
    UsersApi,
};
/// Map an `rsoundcloud` album/playlist into a card for the search tab.
fn album_to_card(ap: &AlbumPlaylist) -> CardData {
    CardData {
        id: ap.album_playlist.id.to_string(),
        title: ap.album_playlist.title.clone(),
        subtitle: ap.user.user.username.clone(),
        thumbnail: ap.album_playlist.artwork_url.clone().unwrap_or_default(),
    }
}

/// Map an `rsoundcloud` track into a playable `Track`. The `permalink_url` +
/// numeric `id` let the existing `yt-dlp` stream/download path play it, so we
/// don't need `SoundCloud`'s (auth-gated) stream URLs.
fn sc_track_to_track(t: &SCTrack) -> Track {
    Track::from_provider(
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
        Some(TrackAlbum {
            name: String::new(),
            id: String::new(),
        }),
        None,
    )
}

/// Map an `rsoundcloud` user (artist) into a card for the Artists search tab.
fn user_to_card(u: &User) -> CardData {
    CardData {
        id: u.user.id.to_string(),
        title: u.user.username.clone(),
        subtitle: u.user.full_name.clone(),
        thumbnail: u.user.avatar_url.clone(),
    }
}

/// Map a `rsoundcloud` basic track (from an artist's track list) into a
/// playable `Track`, carrying the `permalink_url` for `yt-dlp` playback.
fn sc_basic_track_to_track(t: &BasicTrack) -> Track {
    Track::from_provider(
        ProviderId::SoundCloud,
        t.track.id.to_string(),
        t.track.permalink_url.clone(),
        t.track.title.clone(),
        t.user.username.clone(),
        (t.track.duration.max(0) as u64 / 1000) as u32,
        t.track.artwork_url.clone().unwrap_or_default(),
        Some(TrackAlbum {
            name: String::new(),
            id: String::new(),
        }),
        None,
    )
}

/// Build a current-thread tokio runtime and run an async `rsoundcloud` call to
/// completion. The provider backends run on plain `std::thread`s, but
/// `rsoundcloud` is async, so we drive it with a throwaway runtime.
fn block_on_sc<F, T>(fut: F) -> Result<T>
where
    F: std::future::Future<Output = rsoundcloud::ClientResult<T>>,
{
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("Failed to build SoundCloud runtime")?;
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
        SearchScope::Albums => match search_albums(query, offset) {
            Ok(cards) => (Vec::new(), SearchTab::Albums(cards)),
            Err(e) => {
                tracing::warn!("SoundCloud album search failed: {e:#}");
                (Vec::new(), SearchTab::Albums(Vec::new()))
            }
        },
        SearchScope::Playlists => match search_playlists(query, offset) {
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
    Ok(tracks.iter().map(sc_track_to_track).collect())
}

fn search_users(query: &str, offset: usize) -> Result<Vec<CardData>> {
    let cards = block_on_sc(async {
        let client = sc_client().await?;
        client
            .search_users(query.to_string(), search_page(offset))
            .await
    })?;
    Ok(cards.iter().map(user_to_card).collect())
}

fn search_albums(query: &str, offset: usize) -> Result<Vec<CardData>> {
    let cards = block_on_sc(async {
        let client = sc_client().await?;
        client
            .search_albums(query.to_string(), search_page(offset))
            .await
    })?;
    Ok(cards.iter().map(album_to_card).collect())
}

fn search_playlists(query: &str, offset: usize) -> Result<Vec<CardData>> {
    let cards = block_on_sc(async {
        let client = sc_client().await?;
        client
            .search_playlists(query.to_string(), search_page(offset))
            .await
    })?;
    Ok(cards.iter().map(album_to_card).collect())
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
        Ok(tracks.iter().map(sc_basic_track_to_track).collect())
    } else {
        let tracks = block_on_sc(async {
            let client = sc_client().await?;
            client.get_playlist_tracks(ResourceId::Id(parsed)).await
        })?;
        Ok(tracks.iter().map(sc_track_to_track).collect())
    }
}

/// Resolve a logical track to a `SoundCloud` id via search. Returns
/// `Ok(None)` when no match is found (not an error).
pub fn resolve_id(track: &Track) -> Result<Option<(String, String)>> {
    let query = track.search_query();
    let items = search_tracks(&query, 0)?;
    Ok(items.into_iter().next().map(|t| {
        (
            t.provider_id(ProviderId::SoundCloud)
                .unwrap_or_default()
                .to_string(),
            t.primary_url().to_string(),
        )
    }))
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
    let ext = "mp3";
    let out = std::process::Command::new("yt-dlp")
        .args([
            "--extract-audio",
            "--audio-format",
            ext,
            "--audio-quality",
            "0",
            "--output",
            output_path.to_string_lossy().as_ref(),
            "--no-warnings",
            &url,
        ])
        .output()
        .context("Failed to download audio")?;

    if !out.status.success() {
        anyhow::bail!(
            "yt-dlp download failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(output_path.to_string_lossy().replace("%(ext)s", ext))
}
