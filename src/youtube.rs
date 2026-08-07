use anyhow::{Context, Result};
use serde::Deserialize;
use std::{
    io::Write,
    process::{Command, Stdio},
};

#[derive(Debug, Clone)]
pub struct YouTubeVideo {
    pub id: String,
    pub title: String,
    pub url: String,
    pub duration: f64,
    pub channel: String,
    pub thumbnail: String,
}

#[derive(Deserialize)]
struct YTDLPSearchResult {
    id: String,
    title: String,
    #[serde(default)]
    duration: f64,
    channel: String,
    #[serde(default)]
    webpage_url: String,
}

const YTM_SEARCH_URL: &str = "https://music.youtube.com/search?q=";

pub fn search(query: &str, offset: usize) -> Result<Vec<YouTubeVideo>> {
    // Primary: ytmusicapi for the initial page (songs, not channels).
    // yt-dlp is the fallback for pagination (search_more) and when ytmusicapi
    // is unavailable. yt-dlp's YTM search mixes channel/mix entries that
    // get filtered out, so it's less useful for the initial page.
    if offset == 0 {
        if let Ok(videos) = search_ytmusic(query) {
            return Ok(videos);
        }
    }
    search_ytdlp(query, offset, crate::theme::SEARCH_PAGE_SIZE)
}

fn search_ytmusic(query: &str) -> Result<Vec<YouTubeVideo>> {
    let script_path = std::env::temp_dir().join("music_plr_search.py");
    std::fs::write(&script_path, include_str!("./youtube_search.py"))
        .context("Failed to write ytmusicapi script")?;

    let limit = 20;
    let output = Command::new("python3")
        .arg(&script_path)
        .arg(query)
        .arg(limit.to_string())
        .output()
        .context("Failed to run python3. Is it installed?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("ytmusicapi failed: {stderr}");
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let items: Vec<YtMusicResult> =
        serde_json::from_str(&stdout).context("Failed to parse ytmusicapi output")?;

    Ok(items
        .into_iter()
        .map(|r| YouTubeVideo {
            id: r.id,
            title: r.title,
            url: r.url,
            duration: f64::from(r.duration),
            channel: r.channel,
            thumbnail: r.thumbnail,
        })
        .collect())
}

#[derive(Deserialize)]
struct YtMusicResult {
    id: String,
    title: String,
    url: String,
    #[serde(default)]
    duration: u32,
    #[serde(default)]
    channel: String,
    #[serde(default)]
    thumbnail: String,
}

fn search_ytdlp(query: &str, offset: usize, page_size: usize) -> Result<Vec<YouTubeVideo>> {
    // yt-dlp --playlist-start/--playlist-end are 1-based, so add 1 to the
    // 0-based offset to get the 1-based start position.
    let (mut videos, valid_ids) = flat_search(query, offset + 1, offset + page_size)?;
    enrich_with_metadata(&mut videos, &valid_ids);
    Ok(videos)
}

pub fn search_more(query: &str, offset: usize) -> Result<Vec<YouTubeVideo>> {
    search_ytdlp(query, offset, crate::theme::SEARCH_PAGE_SIZE)
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
            if id.len() > 12 || id.starts_with("MPRE") || id.starts_with("UC") {
                continue;
            }
            valid_ids.push(id.clone());
            videos.push(YouTubeVideo {
                id: id.clone(),
                title: item.title,
                url: String::new(),
                duration: 0.0,
                channel: String::new(),
                thumbnail: format!("https://i.ytimg.com/vi/{id}/mqdefault.jpg"),
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
            video.url = if item.webpage_url.is_empty() {
                format!("https://youtube.com/watch?v={}", video.id)
            } else {
                item.webpage_url.clone()
            };
        }
        if video.url.is_empty() {
            video.url = format!("https://youtube.com/watch?v={}", video.id);
        }
    }
}

// Single batched metadata pass over yt-dlp for a list of video ids. Returns a
// map keyed by video id; a failed yt-dlp invocation yields an empty map so
// callers gracefully fall back to the cheap flat-search stubs.
#[allow(clippy::manual_let_else)]
fn fetch_batch_metadata(
    valid_ids: &[String],
) -> std::collections::HashMap<String, YTDLPSearchResult> {
    use std::collections::HashMap;
    let mut results: HashMap<String, YTDLPSearchResult> = HashMap::new();
    let mut child = match Command::new("yt-dlp")
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
    {
        Ok(c) => c,
        Err(_) => return results,
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

pub fn radio_song(song_name: &str) -> Result<Vec<YouTubeVideo>> {
    search(&format!("{song_name} similar songs"), 0)
}

pub fn radio_artist(artist_name: &str) -> Result<Vec<YouTubeVideo>> {
    search(&format!("{artist_name} official songs"), 0)
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
