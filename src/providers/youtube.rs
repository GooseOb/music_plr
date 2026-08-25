use super::{run_command_with_timeout, ytdlp};
use crate::providers::{
    ArtistAlbumCard, ArtistHeader, CardData, ProviderId, RelatedArtistCard, SearchScope, SearchTab,
};
use crate::types::Track;
use anyhow::{Context, Result};
use serde::Deserialize;
use std::{
    io::Write,
    process::{Command, Stdio},
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

#[derive(Debug, Clone, serde::Deserialize)]
pub struct YouTubeVideo {
    pub id: String,
    pub title: String,
    pub url: String,
    #[serde(default, deserialize_with = "deserialize_flexible_duration")]
    pub duration: u32,
    #[serde(default)]
    pub channel: String,
    #[serde(default)]
    pub thumbnail: String,
    #[serde(default)]
    pub album: Option<crate::types::TrackAlbum>,
    #[serde(default)]
    pub artist_id: Option<String>,
    /// ytmusicapi's abbreviated view count ("1.2M", "841M plays"), parsed
    /// to an exact number at deserialization; 0 when absent.
    #[serde(default, deserialize_with = "deserialize_view_count")]
    pub views: u64,
}

impl From<YouTubeVideo> for Track {
    fn from(v: YouTubeVideo) -> Self {
        let mut track = Track::from_provider(
            ProviderId::YouTube,
            v.id,
            v.url,
            v.title,
            v.channel,
            v.duration,
            v.thumbnail,
            v.album,
            v.artist_id,
        );
        if let Some(pt) = track.providers.get_mut(&ProviderId::YouTube) {
            pt.play_count = v.views;
        }
        track
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
    #[serde(default)]
    view_count: Option<u64>,
}

const YTM_SEARCH_URL: &str = "https://music.youtube.com/search?q=";

/// Durations beyond a week are garbage (e.g. "1e30"), not tracks.
const MAX_TRACK_SECS: f64 = 7.0 * 24.0 * 3600.0;

/// Accept either an abbreviated string ("1.2M", "841M plays") or a plain
/// number for view counts.
fn deserialize_view_count<'de, D>(deserializer: D) -> Result<u64, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde_json::Value;
    Ok(match Value::deserialize(deserializer)? {
        Value::Number(n) => n.as_u64().unwrap_or(0),
        Value::String(s) => parse_abbreviated_count(&s),
        _ => 0,
    })
}

/// ytmusicapi emits durations either as seconds (search) or as a raw
/// "M:SS"/"H:MM:SS" string (browse/watch shelves); accept both.
fn deserialize_flexible_duration<'de, D>(deserializer: D) -> Result<u32, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde_json::Value;
    let secs = match Value::deserialize(deserializer)? {
        Value::Number(n) => n.as_f64().unwrap_or(0.0),
        Value::String(s) => s
            .split(':')
            .try_fold(0.0_f64, |acc, part| {
                part.trim().parse::<f64>().map(|p| acc * 60.0 + p)
            })
            .unwrap_or(0.0),
        _ => 0.0,
    };
    let secs = if secs.is_finite() && (0.0..MAX_TRACK_SECS).contains(&secs) {
        secs
    } else {
        0.0
    };
    Ok(secs as u32)
}

/// Parse ytmusicapi's view/play counts ("1.2M", "847", "3.4K", "841M plays").
fn parse_abbreviated_count(s: &str) -> u64 {
    let s = s
        .trim()
        .trim_end_matches("plays")
        .trim_end_matches("views")
        .trim()
        .replace(['\u{a0}', ','], "");
    let (num, mult) = match s.chars().last() {
        Some('K' | 'k') => (&s[..s.len() - 1], 1_000u64),
        Some('M' | 'm') => (&s[..s.len() - 1], 1_000_000),
        Some('B' | 'b') => (&s[..s.len() - 1], 1_000_000_000),
        _ => (s.as_str(), 1),
    };
    num.parse::<f64>().map_or(0, |n| (n * mult as f64) as u64)
}

const SEARCH_TIMEOUT: Duration = Duration::from_mins(1);
const PYTHON_TIMEOUT: Duration = Duration::from_secs(30);

/// Write the embedded ytmusicapi script to a unique temp file, run it with
/// `python3` in the given `mode`, and return its stdout. Unique per pid +
/// call counter (concurrent searches used to race on one fixed filename) and
/// removed after the run.
fn run_python(mode: &str, args: &[&str]) -> Result<String> {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let script_path = std::env::temp_dir().join(format!(
        "music_plr_search_{}_{}.py",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::write(&script_path, include_str!("../youtube_search.py"))
        .context("Failed to write ytmusicapi script")?;

    let result = (|| {
        let mut cmd = Command::new("python3");
        cmd.arg(&script_path).arg(mode).args(args);
        let output = run_command_with_timeout(&mut cmd, PYTHON_TIMEOUT)
            .context("Failed to run python3. Is it installed?")?;
        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            anyhow::bail!("ytmusicapi {mode} failed: {stderr}");
        }
        Ok(String::from_utf8_lossy(&output.stdout).into_owned())
    })();

    let _ = std::fs::remove_file(&script_path);
    result
}

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
/// Album browse payload: the python helper returns `{meta, tracks}` for
/// albums; other kinds (and older helpers) return a bare track list.
#[derive(Deserialize)]
#[serde(untagged)]
enum YtBrowseOutput {
    Album {
        #[serde(default)]
        meta: crate::providers::AlbumMeta,
        #[serde(default)]
        tracks: Vec<YouTubeVideo>,
    },
    Tracks(Vec<YouTubeVideo>),
}

pub fn browse(id: &str, kind: &str) -> Result<(Vec<Track>, Option<crate::providers::AlbumMeta>)> {
    let stdout = run_python("browse", &[id, "50", kind])?;
    match serde_json::from_str::<YtBrowseOutput>(&stdout)
        .context("Failed to parse ytmusicapi browse output")?
    {
        YtBrowseOutput::Album { meta, tracks } => {
            let has_meta = !meta.badge.is_empty() || !meta.date.is_empty();
            let out = if has_meta { Some(meta) } else { None };
            Ok((tracks.into_iter().map(Track::from).collect(), out))
        }
        YtBrowseOutput::Tracks(items) => Ok((items.into_iter().map(Track::from).collect(), None)),
    }
}

/// Raw JSON shape returned by the python helper's `artist_page` mode. The
/// card lists deserialize straight into the shared provider types.
#[derive(Deserialize, Default)]
#[serde(default)]
struct YtArtistPageRaw {
    header: ArtistHeader,
    popular: Vec<YouTubeVideo>,
    albums: Vec<ArtistAlbumCard>,
    playlists: Vec<CardData>,
    related: Vec<RelatedArtistCard>,
}

/// Fetch the full artist page (header, popular songs, album/single/playlist
/// shelves, related artists) via the python helper.
pub fn fetch_artist_page(
    id: &str,
    kinds: &[crate::providers::ArtistDataKind],
) -> Result<crate::providers::ArtistPage> {
    use crate::providers::{ArtistDataKind as K, ArtistPage};

    // YouTube serves the whole page in a single request regardless of which
    // kinds are asked; unrequested kinds are filtered out of the result.
    let stdout = run_python("artist_page", &[id])?;
    let mut raw: YtArtistPageRaw =
        serde_json::from_str(&stdout).context("Failed to parse ytmusicapi artist_page output")?;
    // The songs shelf carries no durations or view counts; those arrive
    // later via [`enrich_track_metadata`] (a batched yt-dlp pass) so the
    // page can render without waiting for it.
    if !K::Popular.wanted(kinds) {
        raw.popular.clear();
    }
    if !K::Header.wanted(kinds) {
        raw.header.stats.clear();
        raw.header.description.clear();
        raw.header.image.clear();
    }
    if !K::Albums.wanted(kinds) {
        raw.albums.clear();
    }
    if !K::Playlists.wanted(kinds) {
        raw.playlists.clear();
    }
    if !K::Related.wanted(kinds) {
        raw.related.clear();
    }
    Ok(ArtistPage {
        header: Some(raw.header),
        popular: raw.popular.into_iter().map(Track::from).collect(),
        albums: raw.albums,
        playlists: raw.playlists,
        related: raw.related,
    })
}

/// Resolve an artist name to a `YouTube` channel browseId via the Artists
/// search scope. Returns `Ok(None)` when nothing matched.
pub fn resolve_artist_id(name: &str) -> Result<Option<String>> {
    let (_, tab) = search_ytmusic(name, SearchScope::Artists)?;
    Ok(match tab {
        SearchTab::Artists(cards) => cards.into_iter().next().map(|c| c.id),
        _ => None,
    })
}

fn search_ytmusic(query: &str, scope: SearchScope) -> Result<(Vec<Track>, SearchTab)> {
    let limit = 20;
    let limit_str = limit.to_string();
    let scope_arg = scope.youtube_filter();
    let stdout = run_python("search", &[query, scope_arg, &limit_str])?;

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

    let mut cmd = Command::new("yt-dlp");
    cmd.args(&args);
    let flat_output = run_command_with_timeout(&mut cmd, SEARCH_TIMEOUT)
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
                views: 0,
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
            if video.views == 0 {
                video.views = item.view_count.unwrap_or(0);
            }
            // Prefer the metadata pass's url (it may correct/normalize the
            // flat pass's url); otherwise keep the already-set flat url.
            if !item.webpage_url.is_empty() {
                video.url = item.webpage_url.clone();
            }
        }
    }
}

/// Fill missing duration/view-count data on YouTube-sourced tracks with one
/// batched yt-dlp metadata pass. Used as a second phase after the artist
/// page renders, so popular rows don't wait on yt-dlp.
pub fn enrich_track_metadata(tracks: &mut [crate::types::Track]) {
    let ids: Vec<String> = tracks
        .iter()
        .filter_map(|t| t.provider_id(ProviderId::YouTube).map(str::to_string))
        .collect();
    if ids.is_empty() {
        return;
    }
    let metadata = fetch_batch_metadata(&ids);
    for track in tracks.iter_mut() {
        let Some(id) = track.provider_id(ProviderId::YouTube).map(str::to_string) else {
            continue;
        };
        if let (Some(item), Some(pt)) = (
            metadata.get(&id),
            track.providers.get_mut(&ProviderId::YouTube),
        ) {
            if pt.duration == 0 {
                pt.duration = item.duration;
            }
            // The songs shelf never carries counts; the yt-dlp pass's
            // exact view_count is the authoritative fallback.
            if pt.play_count == 0 {
                pt.play_count = item.view_count.unwrap_or(0);
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
    let video_arg = video_id.unwrap_or("");
    let playlist_arg = playlist_id.unwrap_or("");
    let stdout = run_python("watch", &[video_arg, playlist_arg, "50"])?;
    let items: Vec<YouTubeVideo> =
        serde_json::from_str(&stdout).context("Failed to parse ytmusicapi watch output")?;
    Ok(items.into_iter().map(Track::from).collect())
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
    ytdlp::download_audio(
        video_url,
        output_path,
        &["--extractor-args", "youtube:player_client=web_embedded"],
    )
}

/// Whether `id` looks like a real `YouTube` video `id` (11 chars, not a
/// mix/playlist/channel entry). yt-dlp's flat playlist also returns mixes
/// (`MPRE` prefix), channel uploads (`UC` prefix), and other non-video
/// entries whose ids are longer than the 11-character video id; those are
/// filtered out so the search results stay playable.
fn is_video_id(id: &str) -> bool {
    id.len() == 11 && !id.starts_with("MPRE") && !id.starts_with("UC")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deserializes_view_count_shapes() {
        let v: YouTubeVideo = serde_json::from_value(serde_json::json!({
            "id": "a", "title": "t", "url": "u", "views": "841M plays"
        }))
        .unwrap();
        assert_eq!(v.views, 841_000_000);
        let v: YouTubeVideo = serde_json::from_value(serde_json::json!({
            "id": "a", "title": "t", "url": "u", "views": null
        }))
        .unwrap();
        assert_eq!(v.views, 0);
        let v: YouTubeVideo = serde_json::from_value(serde_json::json!({
            "id": "a", "title": "t", "url": "u", "views": 841_234
        }))
        .unwrap();
        assert_eq!(v.views, 841_234);
        for fractional in [-12.5f64, 841_234.7, u64::MAX as f64 * 2.0] {
            let v: YouTubeVideo = serde_json::from_value(serde_json::json!({
                "id": "a", "title": "t", "url": "u", "views": fractional
            }))
            .unwrap();
            assert_eq!(v.views, 0, "non-u64 number {fractional} degrades to 0");
        }
    }

    #[test]
    fn deserializes_mixed_duration_shapes() {
        let v: YouTubeVideo = serde_json::from_value(serde_json::json!({
            "id": "a", "title": "t", "url": "u",
            "duration": "4:36"
        }))
        .unwrap();
        assert_eq!(v.duration, 276);
        let v: YouTubeVideo = serde_json::from_value(serde_json::json!({
            "id": "a", "title": "t", "url": "u", "duration": 321.0
        }))
        .unwrap();
        assert_eq!(v.duration, 321);
        let v: YouTubeVideo = serde_json::from_value(serde_json::json!({
            "id": "a", "title": "t", "url": "u", "duration": null
        }))
        .unwrap();
        assert_eq!(v.duration, 0);
        for malformed in ["-1:30", "1e30", "4:xx"] {
            let v: YouTubeVideo = serde_json::from_value(serde_json::json!({
                "id": "a", "title": "t", "url": "u", "duration": malformed
            }))
            .unwrap();
            assert_eq!(v.duration, 0, "malformed duration {malformed} -> 0");
        }
        let v: YouTubeVideo = serde_json::from_value(serde_json::json!({
            "id": "a", "title": "t", "url": "u", "duration": 1e30
        }))
        .unwrap();
        assert_eq!(v.duration, 0);
        let v: YouTubeVideo = serde_json::from_value(serde_json::json!({
            "id": "a", "title": "t", "url": "u", "duration": "2:00:05"
        }))
        .unwrap();
        assert_eq!(v.duration, 7205);
    }
    use super::parse_abbreviated_count as p;

    #[test]
    fn parses_abbreviated_counts() {
        assert_eq!(p("847"), 847);
        assert_eq!(p("1.2K"), 1_200);
        assert_eq!(p("966K"), 966_000);
        assert_eq!(p("4.9M"), 4_900_000);
        assert_eq!(p("841M plays"), 841_000_000);
        assert_eq!(p("614M\u{a0}plays"), 614_000_000);
        assert_eq!(p("3.4B views"), 3_400_000_000);
        assert_eq!(p(""), 0);
        assert_eq!(p("garbage"), 0);
        assert_eq!(p(" \u{a0}1.2K "), 1_200);
        assert_eq!(p("-3K"), 0);
    }
}
