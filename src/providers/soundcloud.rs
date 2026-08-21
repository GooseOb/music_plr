//! `SoundCloud` provider. Search and download both go through `yt-dlp`, which
//! supports `SoundCloud` directly (no API key). Streaming reuses the `YouTube`
//! pipeline because both are yt-dlp-backed.

use crate::providers::{ProviderId, SearchScope, SearchTab};
use crate::types::{ProviderTrack, Track, TrackAlbum, TrackArtist};
use anyhow::Result;
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
struct SCThumbnail {
    #[serde(default)]
    url: String,
    #[serde(default)]
    width: u32,
}

#[derive(Debug, Clone, Deserialize)]
struct SCDirectResult {
    id: String,
    title: String,
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    uploader: String,
    #[serde(default)]
    webpage_url: String,
    #[serde(default)]
    thumbnail: String,
    #[serde(default)]
    thumbnails: Vec<SCThumbnail>,
}

/// Pick the largest available thumbnail URL from a `SoundCloud` result, falling
/// back to the (often absent) top-level `thumbnail` field.
fn best_thumbnail(item: &SCDirectResult) -> String {
    item.thumbnails
        .iter()
        .max_by_key(|t| t.width)
        .map_or_else(|| item.thumbnail.clone(), |t| t.url.clone())
}

/// Run a yt-dlp flat search against `SoundCloud` and return the resolved track
/// stubs (id + metadata). `SoundCloud` search uses yt-dlp's `scsearchN:<query>`
/// selector (the public web `/search?q=` URL returns HTTP 404 via yt-dlp).
fn sc_search(query: &str, start: usize, end: usize) -> Vec<SCDirectResult> {
    // yt-dlp treats `scsearchN:query` as a search returning N results; the
    // playlist-start/end window then slices the page for pagination.
    let search_url = format!("scsearch{end}:{query}");
    let args: Vec<String> = vec![
        "--flat-playlist".into(),
        "--dump-json".into(),
        "--no-warnings".into(),
        "--playlist-start".into(),
        start.to_string(),
        "--playlist-end".into(),
        end.to_string(),
        search_url,
    ];

    let Ok(out) = std::process::Command::new("yt-dlp").args(&args).output() else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }

    let mut results = Vec::new();
    for line in String::from_utf8_lossy(&out.stdout).lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(item) = serde_json::from_str::<SCDirectResult>(line) {
            results.push(item);
        }
    }
    results
}

fn to_track(item: &SCDirectResult) -> Track {
    let mut providers = std::collections::HashMap::new();
    providers.insert(
        ProviderId::SoundCloud,
        ProviderTrack {
            id: item.id.clone(),
            url: if item.webpage_url.is_empty() {
                format!("https://soundcloud.com/{}", item.id)
            } else {
                item.webpage_url.clone()
            },
            artist_id: None,
        },
    );
    Track {
        title: item.title.clone(),
        artist: TrackArtist {
            name: item.uploader.clone(),
            id: None,
        },
        duration: item.duration as u32,
        thumbnail: best_thumbnail(item),
        download_path: None,
        album: Some(TrackAlbum {
            name: String::new(),
            id: String::new(),
        }),
        origin: ProviderId::SoundCloud,
        providers,
    }
}

pub fn search(query: &str, _scope: SearchScope, offset: usize) -> (Vec<Track>, SearchTab) {
    let items = sc_search(query, offset + 1, offset + crate::theme::SEARCH_PAGE_SIZE);
    let tracks = items.into_iter().map(|t| to_track(&t)).collect();
    (tracks, SearchTab::Songs)
}

pub fn search_more(query: &str, offset: usize) -> Vec<Track> {
    let items = sc_search(query, offset + 1, offset + crate::theme::SEARCH_PAGE_SIZE);
    items.into_iter().map(|t| to_track(&t)).collect()
}

/// Resolve a logical track to a `SoundCloud` id via yt-dlp search.
pub fn resolve_id(track: &Track) -> Option<(String, String)> {
    let query = track.search_query();
    let items = sc_search(&query, 1, 1);
    items.into_iter().next().map(|i| (i.id, i.webpage_url))
}

/// Download the track's audio. The track must carry a `SoundCloud` id/url.
pub fn download(track: &Track, download_dir: &str) -> Result<String> {
    let url = track
        .provider_url(ProviderId::SoundCloud)
        .map_or_else(|| track.primary_url().to_string(), str::to_string);
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
        .context_or_anyhow("Failed to download audio")?;

    if !out.status.success() {
        anyhow::bail!(
            "yt-dlp download failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(output_path.to_string_lossy().replace("%(ext)s", ext))
}

trait ContextOrAnyhow {
    fn context_or_anyhow(self, msg: &str) -> Result<std::process::Output>;
}
impl ContextOrAnyhow for std::io::Result<std::process::Output> {
    fn context_or_anyhow(self, msg: &str) -> Result<std::process::Output> {
        self.map_err(|e| anyhow::anyhow!("{msg}: {e}"))
    }
}
