use crate::providers::{CardData, ProviderId, SearchScope, SearchTab};
use crate::types::Track;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct YouTubeVideo {
    pub id: String,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub duration: u32,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub thumbnail: String,
    #[serde(default)]
    pub album: Option<crate::types::TrackAlbum>,
    #[serde(default)]
    pub artist_id: Option<String>,
}

impl From<YouTubeVideo> for Track {
    fn from(v: YouTubeVideo) -> Self {
        Track::from_provider(
            ProviderId::YouTube,
            v.id,
            v.url,
            v.title,
            v.channel,
            v.duration,
            v.thumbnail,
            v.album,
            v.artist_id,
        )
    }
}

#[derive(Deserialize)]
struct YTDLPSearchResult {
    id: String,
    title: String,
    #[serde(default)]
    duration: u32,
    channel: String,
    #[serde(default)]
    webpage_url: String,
}

const YTM_SEARCH_URL: &str = "https://music.youtube.com/search?q=";

/// Run a search and split the result into the playable `Track` list (for
/// Songs/Videos) and the `SearchTab` describing which tab is active (carrying
/// the concrete card lists for Artists/Albums/Playlists). yt-dlp is the
/// fallback for pagination (`search_more`) and when ytmusicapi is unavailable;
/// it only yields playable tracks, so the scoped card tabs rely on ytmusicapi.
pub fn search(query: &str, scope: SearchScope, offset: usize) -> Result<(Vec<Track>, SearchTab)> {
    if offset == 0 {
        if let Ok(parts) = search_ytmusic(query, scope) {
            return Ok(parts);
        }
    }
    let videos = search_ytdlp(query, offset, crate::theme::SEARCH_PAGE_SIZE)?;
    let tracks: Vec<Track> = videos.into_iter().map(Track::from).collect();
    Ok((tracks, SearchTab::Songs))
}

/// Browse the contents of an artist/album/playlist, returning its tracks.
pub fn browse(id: &str, kind: &str) -> Result<Vec<Track>> {
    let script_path = std::env::temp_dir().join("music_plr_search.py");
    std::fs::write(&script_path, include_str!("../youtube_search.py"))
        .context("Failed to write ytmusicapi script")?;

    let output = Command::new("python3")
        .arg(&script_path)
        .arg("browse")
        .arg(id)
        .arg("50")
        .arg(kind)
        .output()
        .context("Failed to run python3. Is it installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ytmusicapi browse failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let items: Vec<YouTubeVideo> =
        serde_json::from_str(&stdout).context("Failed to parse ytmusicapi browse output")?;

    Ok(items.into_iter().map(Track::from).collect())
}

fn search_ytmusic(query: &str, scope: SearchScope) -> Result<(Vec<Track>, SearchTab)> {
    let script_path = std::env::temp_dir().join("music_plr_search.py");
    std::fs::write(&script_path, include_str!("../youtube_search.py"))
        .context("Failed to write ytmusicapi script")?;

    let limit = 20;
    let scope_arg = scope.youtube_filter();
    let output = Command::new("python3")
        .arg(&script_path)
        .arg("search")
        .arg(query)
        .arg(scope_arg)
        .arg(limit.to_string())
        .output()
        .context("Failed to run python3. Is it installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ytmusicapi failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let raw: Vec<serde_json::Value> =
        serde_json::from_str(&stdout).context("Failed to parse ytmusicapi output")?;

    let mut tracks: Vec<Track> = Vec::new();
    let mut cards: Vec<CardData> = Vec::new();
    let mut albums: Vec<CardData> = Vec::new();
    let mut playlists: Vec<CardData> = Vec::new();
    for v in raw {
        let kind = v.get("kind").and_then(|k| k.as_str()).unwrap_or("track");
        let id = v["id"].as_str().unwrap_or_default().to_string();
        let title = v["title"].as_str().unwrap_or_default().to_string();
        let subtitle = v["subtitle"].as_str().unwrap_or_default().to_string();
        let thumbnail = v["thumbnail"].as_str().unwrap_or_default().to_string();
        match kind {
            "artist" => cards.push(CardData {
                id,
                title,
                subtitle: String::new(),
                thumbnail,
            }),
            "album" => albums.push(CardData {
                id,
                title,
                subtitle,
                thumbnail,
            }),
            "playlist" => playlists.push(CardData {
                id,
                title,
                subtitle,
                thumbnail,
            }),
            _ => {
                // song / video -> YouTubeVideo
                if let Ok(video) = serde_json::from_value::<YouTubeVideo>(v) {
                    tracks.push(video.into());
                }
            }
        }
    }

    let tab = match scope {
        SearchScope::Songs => SearchTab::Songs,
        SearchScope::Videos => SearchTab::Videos,
        SearchScope::Artists => SearchTab::Artists(cards),
        SearchScope::Albums => SearchTab::Albums(albums),
        SearchScope::Playlists => SearchTab::Playlists(playlists),
    };
    Ok((tracks, tab))
}

fn search_ytdlp(query: &str, offset: usize, page_size: usize) -> Result<Vec<YouTubeVideo>> {
    // yt-dlp --playlist-start/--playlist-end are 1-based, so add 1 to the
    // 0-based offset to get the 1-based start position.
    let (mut videos, valid_ids) = flat_search(query, offset + 1, offset + page_size)?;
    enrich_with_metadata(&mut videos, &valid_ids);
    Ok(videos)
}

pub fn search_more(query: &str, offset: usize) -> Result<Vec<Track>> {
    // Pagination only works through yt-dlp, which yields playable tracks.
    // (Scoped non-All searches get their first page from ytmusicapi; further
    // pages fall back to general track results — acceptable for "Load More".)
    let videos = search_ytdlp(query, offset, crate::theme::SEARCH_PAGE_SIZE)?;
    Ok(videos.into_iter().map(Track::from).collect())
}

// yt-dlp --flat-playlist pass: collect lightweight video stubs plus the ids
// that need a second, more expensive metadata pass. Playlist offsets are
// 1-based per yt-dlp's --playlist-start/--playlist-end convention; callers
// must convert 0-based offsets to 1-based before calling.
fn flat_search(query: &str, start: usize, end: usize) -> Result<(Vec<YouTubeVideo>, Vec<String>)> {
    let start_str = start.to_string();
    let end_str = end.to_string();
    let args: Vec<&str> = vec![
        "--default-search",
        YTM_SEARCH_URL,
        "--flat-playlist",
        "--dump-json",
        "--no-warnings",
        "--playlist-start",
        &start_str,
        "--playlist-end",
        &end_str,
        query,
    ];

    let flat_output = Command::new("yt-dlp")
        .args(&args)
        .output()
        .context("Failed to run yt-dlp. Is it installed?")?;

    if !flat_output.status.success() {
        let stderr = String::from_utf8_lossy(&flat_output.stderr);
        anyhow::bail!("yt-dlp search failed: {stderr}");
    }

    let mut videos: Vec<YouTubeVideo> = Vec::new();
    let mut valid_ids: Vec<String> = Vec::new();

    for line in String::from_utf8_lossy(&flat_output.stdout).lines() {
        if line.trim().is_empty() {
            continue;
        }
        if let Ok(item) = serde_json::from_str::<YTDLPSearchResult>(line) {
            let id = item.id;
            // Skip non-video playlist entries (mixes, channels, etc.).
            if !is_video_id(&id) {
                continue;
            }
            valid_ids.push(id.clone());
            videos.push(YouTubeVideo {
                id: id.clone(),
                title: item.title,
                // Reuse the flat pass's real webpage_url (preserves Music URLs)
                url: if item.webpage_url.is_empty() {
                    format!("https://youtube.com/watch?v={id}")
                } else {
                    item.webpage_url
                },
                duration: 0,
                channel: String::new(),
                thumbnail: format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"),
                album: None,
                artist_id: None,
            });
        }
    }

    Ok((videos, valid_ids))
}

// Second yt-dlp pass (--batch-file) filling duration/channel/url for the
// given ids. Keyed by video id (not position) so that a silently dropped id
// in yt-dlp's batch output can't mis-assign metadata to the wrong video.
fn enrich_with_metadata(videos: &mut [YouTubeVideo], valid_ids: &[String]) {
    if valid_ids.is_empty() {
        return;
    }

    let metadata = fetch_batch_metadata(valid_ids);
    for video in videos.iter_mut() {
        if let Some(item) = metadata.get(&video.id) {
            video.duration = item.duration;
            video.channel = item.channel.clone();
            // Prefer the metadata pass's url (it may correct/normalize the
            // flat pass's url); otherwise keep the already-set flat url.
            if !item.webpage_url.is_empty() {
                video.url = item.webpage_url.clone();
            }
        }
    }
}

// Single batched metadata pass over yt-dlp for a list of video ids. Returns a
// map keyed by video id; a failed yt-dlp invocation yields an empty map so
// callers gracefully fall back to the cheap flat-search stubs.
fn fetch_batch_metadata(
    valid_ids: &[String],
) -> std::collections::HashMap<String, YTDLPSearchResult> {
    use std::collections::HashMap;
    let mut results: HashMap<String, YTDLPSearchResult> = HashMap::new();

    let Ok(mut child) = Command::new("yt-dlp")
        .args([
            "--batch-file",
            "-",
            "--dump-json",
            "--skip-download",
            "--no-warnings",
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
    else {
        return results;
    };

    if let Some(ref mut stdin) = child.stdin {
        for id in valid_ids {
            let _ = writeln!(stdin, "https://youtube.com/watch?v={id}");
        }
    }
    drop(child.stdin.take());

    if let Ok(output) = child.wait_with_output() {
        if output.status.success() {
            for line in String::from_utf8_lossy(&output.stdout).lines() {
                if line.trim().is_empty() {
                    continue;
                }
                if let Ok(item) = serde_json::from_str::<YTDLPSearchResult>(line) {
                    results.insert(item.id.clone(), item);
                }
            }
        }
    }
    results
}

/// Fetch a `YouTube` Music radio/mix playlist via ytmusicapi's watch-playlist
/// engine (the same one behind the site's "Start radio" button). Pass a
/// `video_id` for a song-seeded mix or a `playlist_id` (e.g. an artist
/// browseId) for an artist/playlist mix. Durations and thumbnails are derived
/// locally from the watch-playlist response, so no extra yt-dlp pass is
/// needed (which keeps radio generation to a couple of seconds).
pub fn watch_playlist(video_id: Option<&str>, playlist_id: Option<&str>) -> Result<Vec<Track>> {
    let script_path = std::env::temp_dir().join("music_plr_search.py");
    std::fs::write(&script_path, include_str!("../youtube_search.py"))
        .context("Failed to write ytmusicapi script")?;

    let video_arg = video_id.unwrap_or("");
    let playlist_arg = playlist_id.unwrap_or("");
    let output = Command::new("python3")
        .arg(&script_path)
        .arg("watch")
        .arg(video_arg)
        .arg(playlist_arg)
        .arg("50")
        .output()
        .context("Failed to run python3. Is it installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ytmusicapi watch_playlist failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let items: Vec<YouTubeVideo> =
        serde_json::from_str(&stdout).context("Failed to parse ytmusicapi watch output")?;

    let videos: Vec<YouTubeVideo> = items;

    Ok(videos.into_iter().map(Track::from).collect())
}

/// Build a song radio from a real `YouTube` Music mix seeded by the track's
/// `video_id`.
pub fn radio_song(video_id: &str) -> Result<Vec<Track>> {
    watch_playlist(Some(video_id), None)
}

/// Build an artist radio from a real `YouTube` Music mix seeded by the artist's
/// `browse_id` (resolved from the track's provider artist id).
pub fn radio_artist(browse_id: &str) -> Result<Vec<Track>> {
    watch_playlist(None, Some(browse_id))
}

/// Resolve a logical track (title + artist) to a `YouTube` video id by running a
/// yt-dlp search and returning the first result's id. Used by the "play via /
/// download from `YouTube`" flow when a track lacks a `YouTube` id.
pub fn resolve_id(track: &Track) -> Result<Option<Track>> {
    let query = track.search_query();
    let (videos, _) = flat_search(&query, 1, 1)?;
    Ok(videos.into_iter().next().map(|mut v| {
        if v.url.is_empty() {
            v.url = format!("https://www.youtube.com/watch?v={}", v.id);
        }
        Track::from(v)
    }))
}

pub fn download(video_url: &str, download_dir: &str) -> Result<String> {
    let id = video_url
        .split("v=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .unwrap_or("download");
    let dir = std::path::Path::new(download_dir);
    let _ = std::fs::create_dir_all(dir);
    let output_path = dir.join(format!("{id}.mp3"));
    download_audio(video_url, output_path.to_string_lossy().as_ref())
}

pub fn download_audio(video_url: &str, output_path: &str) -> Result<String> {
    let ext = "mp3";
    let output = Command::new("yt-dlp")
        .args([
            "--extract-audio",
            "--audio-format",
            ext,
            "--audio-quality",
            "0",
            "--output",
            output_path,
            "--no-warnings",
            "--extractor-args",
            "youtube:player_client=web_embedded",
            video_url,
        ])
        .output()
        .context("Failed to download audio")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("yt-dlp download failed: {stderr}");
    }

    Ok(output_path.replace("%(ext)s", ext))
}

/// Whether `id` looks like a real `YouTube` video `id` (11 chars, not a
/// mix/playlist/channel entry). yt-dlp's flat playlist also returns mixes
/// (`MPRE` prefix), channel uploads (`UC` prefix), and other non-video
/// entries whose ids are longer than the 11-character video id; those are
/// filtered out so the search results stay playable.
fn is_video_id(id: &str) -> bool {
    id.len() == 11 && !id.starts_with("MPRE") && !id.starts_with("UC")
}
