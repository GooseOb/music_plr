//! Lyrics fetching, backed by a handful of free, no-API-key providers.

use std::fmt::Write as _;

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::util::urlencode;

pub const LRCLIB_BASE: &str = "https://lrclib.net/api";
pub const LRCMUX_BASE: &str = "https://lrcmux.dev/api";
pub const LYRICS_OVH_BASE: &str = "https://api.lyrics.ovh/v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum LyricsProvider {
    #[default]
    #[serde(rename = "lrclib")]
    LrcLib,
    #[serde(rename = "lrcmux")]
    LrcMux,
    #[serde(rename = "lyrics_ovh")]
    LyricsOvh,
}

impl LyricsProvider {
    pub fn name(self) -> &'static str {
        match self {
            LyricsProvider::LrcLib => "LRCLib",
            LyricsProvider::LrcMux => "LrcMux",
            LyricsProvider::LyricsOvh => "Lyrics.ovh",
        }
    }

    pub fn all() -> &'static [LyricsProvider] {
        &[
            LyricsProvider::LrcLib,
            LyricsProvider::LrcMux,
            LyricsProvider::LyricsOvh,
        ]
    }

    pub fn fetch(self, req: &LyricsRequest) -> Result<Option<Lyrics>> {
        match self {
            LyricsProvider::LrcLib => fetch_lrclib(req),
            LyricsProvider::LrcMux => fetch_lrcmux(req),
            LyricsProvider::LyricsOvh => fetch_lyrics_ovh(req),
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
    fetch_lrclib_compat(req, LRCLIB_BASE, LyricsProvider::LrcLib)
}

fn fetch_lrcmux(req: &LyricsRequest) -> Result<Option<Lyrics>> {
    fetch_lrclib_compat(req, LRCMUX_BASE, LyricsProvider::LrcMux)
}

fn fetch_lrclib_compat(
    req: &LyricsRequest,
    base: &str,
    provider: LyricsProvider,
) -> Result<Option<Lyrics>> {
    if let Some(lyrics) = get_lrclib(req, base, provider)? {
        return Ok(Some(lyrics));
    }
    search_lrclib(req, base, provider)
}

/// Shared `ureq` agent with connect + overall timeouts so a dead lyrics
/// provider can't hang a background thread indefinitely.
fn agent() -> &'static ureq::Agent {
    static AGENT: std::sync::OnceLock<ureq::Agent> = std::sync::OnceLock::new();
    AGENT.get_or_init(|| {
        ureq::config::Config::builder()
            .timeout_connect(Some(std::time::Duration::from_secs(15)))
            .timeout_global(Some(std::time::Duration::from_secs(15)))
            .build()
            .new_agent()
    })
}

fn get_json_opt<T: serde::de::DeserializeOwned>(url: &str, what: &str) -> Result<Option<T>> {
    match agent()
        .get(url)
        .header(
            "User-Agent",
            "music_plr/0.1 (https://github.com/gooseob/music_plr)",
        )
        .call()
    {
        Ok(mut r) => {
            Ok(Some(r.body_mut().read_json().with_context(|| {
                format!("{what} response was not valid JSON")
            })?))
        }
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(e) => Err(e).with_context(|| format!("{what} request failed")),
    }
}

// Lyrics.ovh is plain-text only (no synced lyrics) and uses a different URL
// shape: `/v1/{artist}/{title}`. It returns `{"lyrics": ...}` or 404.
#[derive(Deserialize)]
struct OvhBody {
    lyrics: String,
}

fn fetch_lyrics_ovh(req: &LyricsRequest) -> Result<Option<Lyrics>> {
    let url = format!(
        "{}/{}/{}",
        LYRICS_OVH_BASE,
        urlencode(&req.artist),
        urlencode(&req.title)
    );
    let resp: OvhBody = match get_json_opt(&url, "Lyrics.ovh")? {
        Some(body) => body,
        None => return Ok(None),
    };
    let plain = resp.lyrics.trim().to_string();
    if plain.is_empty() {
        return Ok(None);
    }
    Ok(Some(Lyrics {
        timed: vec![],
        plain,
        provider: LyricsProvider::LyricsOvh,
    }))
}

#[derive(Debug, Deserialize)]
struct LrcLibRecord {
    #[serde(rename = "syncedLyrics", default)]
    synced_lyrics: Option<String>,
    #[serde(rename = "plainLyrics", default)]
    plain_lyrics: Option<String>,
}

fn get_lrclib(req: &LyricsRequest, base: &str, provider: LyricsProvider) -> Result<Option<Lyrics>> {
    let mut url = format!(
        "{}/get?artist_name={}&track_name={}",
        base,
        urlencode(&req.artist),
        urlencode(&req.title)
    );
    if !req.album.is_empty() {
        url.push_str("&album_name=");
        let _ = std::write!(url, "{}", urlencode(&req.album));
    }
    if req.duration > 0 {
        url.push_str("&duration=");
        let _ = std::write!(url, "{}", req.duration);
    }

    let resp: LrcLibRecord = match get_json_opt(&url, "LRCLib-compatible")? {
        Some(body) => body,
        None => return Ok(None),
    };

    Ok(Some(record_to_lyrics(resp, provider)))
}

fn search_lrclib(
    req: &LyricsRequest,
    base: &str,
    provider: LyricsProvider,
) -> Result<Option<Lyrics>> {
    let url = format!(
        "{}/search?artist_name={}&track_name={}",
        base,
        urlencode(&req.artist),
        urlencode(&req.title)
    );

    let resp: Vec<LrcLibRecord> = match get_json_opt(&url, "LRCLib-compatible")? {
        Some(body) => body,
        None => return Ok(None),
    };

    Ok(resp
        .into_iter()
        .next()
        .map(|rec| record_to_lyrics(rec, provider)))
}

fn record_to_lyrics(rec: LrcLibRecord, provider: LyricsProvider) -> Lyrics {
    let synced = rec.synced_lyrics.filter(|s| !s.trim().is_empty());
    let plain = rec.plain_lyrics.filter(|s| !s.trim().is_empty());
    let plain = plain.unwrap_or_default();
    let timed = synced.as_deref().map(parse_lrc).unwrap_or_default();
    Lyrics {
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
            timed: vec![],
            plain: "words".into(),
            provider: LyricsProvider::LrcLib,
        };
        assert_eq!(lrc.active_index(5.0), None);
    }

    #[test]
    fn client_uses_lrclib_by_default() {
        assert_eq!(LyricsProvider::default(), LyricsProvider::LrcLib);
        assert!(LyricsProvider::all().contains(&LyricsProvider::LrcLib));
        assert_eq!(LyricsProvider::all().len(), 3);
    }
}
