//! Jamendo provider. Jamendo exposes a free REST API for independent/royalty
//! free music. It requires a `client_id`; we ship a public demo id so the
//! provider works out of the box. Searches degrade to empty results if the
//! network or API is unavailable.

use crate::data::JsonStore;
use crate::provider::{ProviderId, SearchScope, SearchTab};
use crate::types::{ProviderTrack, Track, TrackAlbum, TrackArtist};
use crate::util::urlencode;
use anyhow::Result;
use serde::Deserialize;

/// The client id used for API calls, taken from the app config so the user
/// can supply their own registered id.
fn client_id() -> String {
    crate::data::config::Config::load().jamendo_client_id
}

#[derive(Debug, Deserialize)]
struct JamendoTrack {
    id: String,
    name: String,
    #[serde(default)]
    artist_name: String,
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    audio: String,
    #[serde(default)]
    audiodownload: String,
    #[serde(default)]
    image: String,
    #[serde(default)]
    album_name: String,
}

#[derive(Debug, Deserialize)]
struct JamendoResponse {
    #[serde(default)]
    results: Vec<JamendoTrack>,
}

fn api_get(path: &str) -> Option<JamendoResponse> {
    let url = format!(
        "https://api.jamendo.com/v3.0/{path}&client_id={}",
        client_id()
    );
    let resp = ureq::get(&url).call().ok()?;
    let body = resp.into_body().read_to_string().ok()?;
    serde_json::from_str(&body).ok()
}

fn to_track(t: &JamendoTrack) -> Track {
    let stream_url = if t.audio.is_empty() {
        t.audiodownload.clone()
    } else {
        t.audio.clone()
    };
    let mut providers = std::collections::HashMap::new();
    providers.insert(
        ProviderId::Jamendo,
        ProviderTrack {
            id: t.id.clone(),
            url: stream_url.clone(),
            artist_id: None,
        },
    );
    Track {
        title: t.name.clone(),
        artist: TrackArtist {
            name: t.artist_name.clone(),
            id: None,
        },
        duration: t.duration as u32,
        thumbnail: t.image.clone(),
        download_path: None,
        album: Some(TrackAlbum {
            name: t.album_name.clone(),
            id: String::new(),
        }),
        origin: ProviderId::Jamendo,
        providers,
    }
}

pub fn search(query: &str, scope: SearchScope, offset: usize) -> (Vec<Track>, SearchTab) {
    let (search_field, value) = match scope {
        SearchScope::Artists => ("artist_name", query),
        SearchScope::Albums => ("album_name", query),
        _ => ("track_name", query),
    };
    let path = format!(
        "tracks/?format=json&limit={}&offset={}&{}={}&include=musicinfo&audioformat=mp32",
        crate::theme::SEARCH_PAGE_SIZE,
        offset,
        search_field,
        urlencode(value)
    );
    let tracks = api_get(&path)
        .map(|r| r.results.into_iter().map(|t| to_track(&t)).collect())
        .unwrap_or_default();
    (tracks, SearchTab::Songs)
}

pub fn search_more(query: &str, offset: usize) -> Vec<Track> {
    search(query, SearchScope::Songs, offset).0
}

/// Resolve a logical track to a Jamendo id via search.
pub fn resolve_id(track: &Track) -> Option<(String, String)> {
    let q = if track.artist.name.is_empty() {
        track.title.clone()
    } else {
        format!("{} {}", track.title, track.artist.name)
    };
    let path = format!(
        "tracks/?format=json&limit=1&track_name={}&include=musicinfo&audioformat=mp32",
        urlencode(&q)
    );
    api_get(&path)
        .and_then(|r| r.results.into_iter().next())
        .map(|t| {
            let url = if t.audio.is_empty() {
                t.audiodownload.clone()
            } else {
                t.audio.clone()
            };
            (t.id, url)
        })
}

/// Download the track's audio. The track must carry a Jamendo id/url.
pub fn download(track: &Track, download_dir: &str) -> Result<String> {
    let url = track
        .provider_url(ProviderId::Jamendo)
        .map_or_else(|| track.primary_url().to_string(), str::to_string);
    if url.is_empty() {
        anyhow::bail!("no Jamendo audio url for track");
    }
    let id = track.provider_id(ProviderId::Jamendo).unwrap_or("download");
    let dir = std::path::Path::new(download_dir);
    let _ = std::fs::create_dir_all(dir);
    let output_path = dir.join(format!("{id}.mp3"));
    let resp = ureq::get(&url).call()?;
    let mut body = resp.into_body();
    let mut reader = body.as_reader();
    let mut file = std::fs::File::create(&output_path)?;
    std::io::copy(&mut reader, &mut file)?;
    Ok(output_path.to_string_lossy().to_string())
}
