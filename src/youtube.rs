use anyhow::{Context, Result};
use serde::Deserialize;
use std::io::Write;
use std::process::{Command, Stdio};

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

pub fn search(query: &str, _offset: usize) -> Result<Vec<YouTubeVideo>> {
    if let Ok(videos) = search_ytmusic(query) {
        return Ok(videos);
    }
    search_ytdlp(query)
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
        anyhow::bail!("ytmusicapi failed: {}", stderr);
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
            duration: r.duration as f64,
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

fn search_ytdlp(query: &str) -> Result<Vec<YouTubeVideo>> {
    let (mut videos, valid_ids) = flat_search(query, 0, 10)?;
    enrich_with_metadata(&mut videos, &valid_ids);
    Ok(videos)
}

pub fn search_more(query: &str, offset: usize) -> Result<Vec<YouTubeVideo>> {
    let (mut videos, valid_ids) = flat_search(query, offset + 1, offset + 10)?;
    enrich_with_metadata(&mut videos, &valid_ids);
    Ok(videos)
}

// yt-dlp --flat-playlist pass: collect lightweight video stubs plus the ids
// that need a second, more expensive metadata pass. Playlist offsets are
// 1-based per yt-dlp's --playlist-start/--playlist-end convention.
fn flat_search(query: &str, start: usize, end: usize) -> Result<(Vec<YouTubeVideo>, Vec<String>)> {
    let start_str = start.to_string();
    let end_str = end.to_string();
    let mut args: Vec<&str> = vec![
        "--default-search",
        YTM_SEARCH_URL,
        "--flat-playlist",
        "--dump-json",
        "--no-warnings",
    ];
    if start > 0 {
        args.push("--playlist-start");
        args.push(&start_str);
    }
    args.push("--playlist-end");
    args.push(&end_str);
    args.push(query);

    let flat_output = Command::new("yt-dlp")
        .args(&args)
        .output()
        .context("Failed to run yt-dlp. Is it installed?")?;

    if !flat_output.status.success() {
        let stderr = String::from_utf8_lossy(&flat_output.stderr);
        anyhow::bail!("yt-dlp search failed: {}", stderr);
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
                thumbnail: format!("https://i.ytimg.com/vi/{}/mqdefault.jpg", id),
            });
        }
    }

    Ok((videos, valid_ids))
}

// Second yt-dlp pass (--batch-file) filling duration/channel/url for an ordered
// list of ids, in place. Skipped silently if yt-dlp is unavailable.
fn enrich_with_metadata(videos: &mut [YouTubeVideo], valid_ids: &[String]) {
    if valid_ids.is_empty() {
        return;
    }

    for (i, item) in fetch_batch_metadata(valid_ids).into_iter().enumerate() {
        if let Some(video) = videos.get_mut(i) {
            video.duration = item.duration;
            video.channel = item.channel;
            video.url = if item.webpage_url.is_empty() {
                format!("https://youtube.com/watch?v={}", video.id)
            } else {
                item.webpage_url
            };
        }
    }

    for video in videos.iter_mut() {
        if video.url.is_empty() {
            video.url = format!("https://youtube.com/watch?v={}", video.id);
        }
    }
}

// Single batched metadata pass over yt-dlp for a list of video ids. Returns
// results in input order; a failed yt-dlp invocation yields an empty list so
// callers gracefully fall back to the cheap flat-search stubs.
fn fetch_batch_metadata(valid_ids: &[String]) -> Vec<YTDLPSearchResult> {
    let mut results: Vec<YTDLPSearchResult> = Vec::new();
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
            let _ = writeln!(stdin, "https://youtube.com/watch?v={}", id);
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
                    results.push(item);
                }
            }
        }
    }
    results
}

pub fn radio_song(song_name: &str) -> Result<Vec<YouTubeVideo>> {
    search(&format!("{} similar songs", song_name), 0)
}

pub fn radio_artist(artist_name: &str) -> Result<Vec<YouTubeVideo>> {
    search(&format!("{} official songs", artist_name), 0)
}

pub fn download(video_url: &str, download_dir: &str) -> Result<String> {
    let id = video_url
        .split("v=")
        .nth(1)
        .and_then(|s| s.split('&').next())
        .unwrap_or("download");
    let dir = std::path::Path::new(download_dir);
    let _ = std::fs::create_dir_all(dir);
    let output_path = dir.join(format!("{}.mp3", id));
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
        anyhow::bail!("yt-dlp download failed: {}", stderr);
    }

    Ok(output_path.replace("%(ext)s", ext))
}
