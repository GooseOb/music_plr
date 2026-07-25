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
    #[allow(dead_code)]
    pub thumbnail: String,
    pub channel: String,
}

#[derive(Deserialize)]
struct YTDLPSearchResult {
    id: String,
    title: String,
    #[allow(dead_code)]
    url: String,
    #[serde(default)]
    duration: f64,
    #[serde(default)]
    thumbnail: String,
    #[serde(default)]
    channel: String,
    #[serde(default)]
    webpage_url: String,
}

const YTM_SEARCH_URL: &str = "https://music.youtube.com/search?q=";

const PY_SEARCH_SCRIPT: &str = r#"
import sys, json
try:
    from ytmusicapi import YTMusic
except ImportError:
    print(json.dumps({"error": "ytmusicapi not installed"}))
    sys.exit(1)
query = sys.argv[1] if len(sys.argv) > 1 else ""
limit = int(sys.argv[2]) if len(sys.argv) > 2 else 10
if not query:
    print(json.dumps([]))
    sys.exit(0)
results = YTMusic().search(query, filter="songs", limit=limit)
out = []
for r in results:
    vid = r.get("videoId", "")
    if not vid:
        continue
    artists = r.get("artists", [])
    artist = artists[0].get("name", "") if artists else ""
    duration = r.get("duration_seconds", 0) or 0
    thumbs = r.get("thumbnails") or []
    thumb = thumbs[-1].get("url", "") if thumbs else ""
    out.append({
        "id": vid,
        "title": r.get("title", ""),
        "url": f"https://youtube.com/watch?v={vid}",
        "duration": duration,
        "thumbnail": thumb,
        "channel": artist,
    })
print(json.dumps(out))
"#;

pub fn search(query: &str, _offset: usize) -> Result<Vec<YouTubeVideo>> {
    // Try ytmusicapi first
    if let Ok(videos) = search_ytmusic(query) {
        return Ok(videos);
    }
    // Fall back to yt-dlp
    search_ytdlp(query)
}

fn search_ytmusic(query: &str) -> Result<Vec<YouTubeVideo>> {
    let script_path = std::env::temp_dir().join("music_plr_search.py");
    std::fs::write(&script_path, PY_SEARCH_SCRIPT).context("Failed to write ytmusicapi script")?;

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
            thumbnail: r.thumbnail.unwrap_or_default(),
            channel: r.channel,
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
    thumbnail: Option<String>,
    #[serde(default)]
    channel: String,
}

fn search_ytdlp(query: &str) -> Result<Vec<YouTubeVideo>> {
    let mut args: Vec<&str> = vec![
        "--default-search",
        YTM_SEARCH_URL,
        "--flat-playlist",
        "--dump-json",
        "--no-warnings",
        "--playlist-end",
        "10",
    ];
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
            if id.len() > 12 || id.starts_with("MPRE") || id.starts_with("UC") {
                continue;
            }
            valid_ids.push(id.clone());
            videos.push(YouTubeVideo {
                id,
                title: item.title,
                url: String::new(),
                duration: 0.0,
                thumbnail: String::new(),
                channel: String::new(),
            });
        }
    }

    // Batch metadata fetch via --batch-file -
    if !valid_ids.is_empty() {
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
            Err(_) => return Ok(videos),
        };

        if let Some(ref mut stdin) = child.stdin {
            for id in &valid_ids {
                let _ = writeln!(stdin, "https://youtube.com/watch?v={}", id);
            }
        }
        drop(child.stdin.take());

        if let Ok(output) = child.wait_with_output() {
            if output.status.success() {
                for (i, line) in String::from_utf8_lossy(&output.stdout).lines().enumerate() {
                    if line.trim().is_empty() {
                        continue;
                    }
                    if let Ok(item) = serde_json::from_str::<YTDLPSearchResult>(line) {
                        if let Some(video) = videos.get_mut(i) {
                            video.duration = item.duration;
                            video.thumbnail = if item.thumbnail.is_empty() {
                                format!("https://img.youtube.com/vi/{}/hqdefault.jpg", video.id)
                            } else {
                                item.thumbnail
                            };
                            video.channel = item.channel;
                            video.url = if item.webpage_url.is_empty() {
                                format!("https://youtube.com/watch?v={}", video.id)
                            } else {
                                item.webpage_url
                            };
                        }
                    }
                }
            }
        }
    }

    for video in &mut videos {
        if video.url.is_empty() {
            video.url = format!("https://youtube.com/watch?v={}", video.id);
        }
        if video.thumbnail.is_empty() {
            video.thumbnail = format!("https://img.youtube.com/vi/{}/hqdefault.jpg", video.id);
        }
    }

    Ok(videos)
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
