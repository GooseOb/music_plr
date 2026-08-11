//! Lyrics fetching, currently backed by `LRCLib` (free, no API key).

use anyhow::{Context, Result};
use serde::Deserialize;
use std::fmt::Write as _;

pub const LRCLIB_BASE: &str = "https://lrclib.net/api";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum LyricsProvider {
    #[default]
    #[serde(rename = "lrclib")]
    LrcLib,
}

impl LyricsProvider {
    pub fn name(self) -> &'static str {
        match self {
            LyricsProvider::LrcLib => "LRCLib",
        }
    }

    pub fn all() -> &'static [LyricsProvider] {
        &[LyricsProvider::LrcLib]
    }

    pub fn fetch(self, req: &LyricsRequest) -> Result<Option<Lyrics>> {
        match self {
            LyricsProvider::LrcLib => fetch_lrclib(req),
        }
    }
}

#[derive(Debug, Clone)]
pub struct LyricsRequest {
    pub artist: String,
    pub title: String,
    pub album: String,
    pub duration: u32,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lyrics {
    pub timed: Vec<(f32, String)>,
    pub plain: String,
    pub synced: bool,
    pub provider: LyricsProvider,
}

impl Lyrics {
    pub fn active_index(&self, position_secs: f32) -> Option<usize> {
        if self.timed.is_empty() {
            return None;
        }
        let mut idx = 0;
        for (i, (t, _)) in self.timed.iter().enumerate() {
            if *t <= position_secs {
                idx = i;
            } else {
                break;
            }
        }
        Some(idx)
    }
}

#[derive(Debug, Clone, Default)]
pub struct LyricsClient {
    selected: LyricsProvider,
}

impl LyricsClient {
    pub fn new(selected: LyricsProvider) -> Self {
        Self { selected }
    }

    pub fn selected(&self) -> LyricsProvider {
        self.selected
    }

    pub fn fetch(&self, req: &LyricsRequest) -> Result<Option<Lyrics>> {
        self.selected.fetch(req)
    }
}

fn fetch_lrclib(req: &LyricsRequest) -> Result<Option<Lyrics>> {
    if let Some(lyrics) = get(req)? {
        return Ok(Some(lyrics));
    }
    search(req)
}

#[derive(Debug, Deserialize)]
struct LrcLibRecord {
    #[serde(rename = "syncedLyrics", default)]
    synced_lyrics: Option<String>,
    #[serde(rename = "plainLyrics", default)]
    plain_lyrics: Option<String>,
}

fn get(req: &LyricsRequest) -> Result<Option<Lyrics>> {
    let mut url = format!(
        "{}/get?artist_name={}&track_name={}",
        LRCLIB_BASE,
        urlencoding(&req.artist),
        urlencoding(&req.title)
    );
    if !req.album.is_empty() {
        url.push_str("&album_name=");
        let _ = std::write!(url, "{}", urlencoding(&req.album));
    }
    if req.duration > 0 {
        url.push_str("&duration=");
        let _ = std::write!(url, "{}", req.duration);
    }

    let resp: LrcLibRecord = match ureq::get(&url)
        .header(
            "User-Agent",
            "music_plr/0.1 (https://github.com/gooseob/music_plr)",
        )
        .call()
    {
        Ok(mut r) => r
            .body_mut()
            .read_json()
            .context("LRCLib response was not valid JSON")?,
        // 404 means no match; report it as None rather than an error so the
        // UI can show "not found" instead of surfacing a transport failure.
        Err(ureq::Error::StatusCode(404)) => return Ok(None),
        Err(e) => return Err(e).context("LRCLib request failed"),
    };

    Ok(Some(record_to_lyrics(resp, LyricsProvider::LrcLib)))
}

fn search(req: &LyricsRequest) -> Result<Option<Lyrics>> {
    let url = format!(
        "{}/search?artist_name={}&track_name={}",
        LRCLIB_BASE,
        urlencoding(&req.artist),
        urlencoding(&req.title)
    );

    let resp: Vec<LrcLibRecord> = ureq::get(&url)
        .header(
            "User-Agent",
            "music_plr/0.1 (https://github.com/gooseob/music_plr)",
        )
        .call()
        .context("LRCLib request failed")?
        .body_mut()
        .read_json()
        .context("LRCLib response was not valid JSON")?;

    Ok(resp
        .into_iter()
        .next()
        .map(|rec| record_to_lyrics(rec, LyricsProvider::LrcLib)))
}

fn record_to_lyrics(rec: LrcLibRecord, provider: LyricsProvider) -> Lyrics {
    let synced = rec.synced_lyrics.filter(|s| !s.trim().is_empty());
    let plain = rec.plain_lyrics.filter(|s| !s.trim().is_empty());
    let plain = plain.unwrap_or_default();
    let timed = synced.as_deref().map(parse_lrc).unwrap_or_default();
    Lyrics {
        synced: !timed.is_empty(),
        timed,
        plain,
        provider,
    }
}

fn parse_lrc(text: &str) -> Vec<(f32, String)> {
    let mut out = Vec::new();
    for line in text.lines() {
        if let Some((secs, content)) = parse_lrc_line(line) {
            out.push((secs, content));
        }
    }
    out
}

fn parse_lrc_line(line: &str) -> Option<(f32, String)> {
    let open = line.find('[')?;
    let close = line[open..].find(']')? + open;
    let stamp = &line[open + 1..close];
    let content = line[close + 1..].trim().to_string();
    let secs = parse_timestamp(stamp)?;
    Some((secs, content))
}

fn parse_timestamp(stamp: &str) -> Option<f32> {
    let mut parts = stamp.split(':');
    let min: f32 = parts.next()?.parse().ok()?;
    let sec: f32 = parts.next()?.parse().ok()?;
    Some(min * 60.0 + sec)
}

fn urlencoding(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        match c {
            ' ' => out.push('+'),
            '+' => out.push_str("%2B"),
            '&' => out.push_str("%26"),
            '#' => out.push_str("%23"),
            '=' => out.push_str("%3D"),
            c if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' || c == '~' => {
                out.push(c);
            }
            c => {
                let bytes = c.to_string().into_bytes();
                for b in bytes {
                    let _ = std::write!(out, "%{b:02X}");
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_basic_lrc() {
        let text = "[00:12.34]First line\n[00:16.80]Second line\n[00:21.05]Chorus";
        let timed = parse_lrc(text);
        assert_eq!(timed.len(), 3);
        assert!((timed[0].0 - 12.34).abs() < 0.001);
        assert_eq!(timed[0].1, "First line");
        assert!((timed[2].0 - 21.05).abs() < 0.001);
    }

    #[test]
    fn skips_lines_without_timestamp() {
        let text = "intro\n[00:01.00]Real line";
        let timed = parse_lrc(text);
        assert_eq!(timed.len(), 1);
        assert_eq!(timed[0].1, "Real line");
    }

    #[test]
    fn active_index_follows_position() {
        let lrc = Lyrics {
            synced: true,
            timed: vec![(0.0, "a".into()), (10.0, "b".into()), (20.0, "c".into())],
            plain: String::new(),
            provider: LyricsProvider::LrcLib,
        };
        assert_eq!(lrc.active_index(5.0), Some(0));
        assert_eq!(lrc.active_index(10.0), Some(1));
        assert_eq!(lrc.active_index(19.9), Some(1));
        assert_eq!(lrc.active_index(100.0), Some(2));
    }

    #[test]
    fn active_index_none_when_untimed() {
        let lrc = Lyrics {
            synced: false,
            timed: vec![],
            plain: "words".into(),
            provider: LyricsProvider::LrcLib,
        };
        assert_eq!(lrc.active_index(5.0), None);
    }

    #[test]
    fn client_uses_lrclib_by_default() {
        assert_eq!(
            LyricsClient::new(LyricsProvider::LrcLib).selected(),
            LyricsProvider::LrcLib
        );
        assert!(LyricsProvider::all().contains(&LyricsProvider::LrcLib));
    }
}
