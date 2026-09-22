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
    #[serde(rename = "custom")]
    Custom,
}

impl LyricsProvider {
    pub fn name(self) -> &'static str {
        match self {
            LyricsProvider::LrcLib => "LRCLib",
            LyricsProvider::LrcMux => "LrcMux",
            LyricsProvider::LyricsOvh => "Lyrics.ovh",
            LyricsProvider::Custom => "Custom",
        }
    }

    /// Network providers shown as tabs in the lyrics view. `Custom` is
    /// deliberately excluded: it tags user-added lyrics rather than a
    /// fetchable service, and named custom entries render as their own tabs
    /// from `LyricsState::custom_names`.
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
            LyricsProvider::Custom => Ok(None),
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

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct LyricLine {
    pub time: Option<f32>,
    pub text: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    pub plain: String,
    pub provider: LyricsProvider,
}

impl Lyrics {
    pub fn from_custom_text(text: &str) -> Option<Self> {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return None;
        }
        let mut lines = Vec::new();
        for raw in trimmed.lines() {
            if let Some(rest) = raw.strip_prefix("##") {
                lines.push(LyricLine {
                    time: None,
                    text: format!("#{rest}"),
                    description: String::new(),
                });
            } else if let Some(note) = raw.strip_prefix('#') {
                let note = note.strip_prefix(' ').unwrap_or(note);
                if let Some(last) = lines.last_mut() {
                    if !last.description.is_empty() {
                        last.description.push('\n');
                    }
                    last.description.push_str(note);
                }
            } else if let Some((secs, content)) = parse_lrc_line(raw) {
                lines.push(LyricLine {
                    time: Some(secs),
                    text: content,
                    description: String::new(),
                });
            } else {
                lines.push(LyricLine {
                    time: None,
                    text: raw.to_string(),
                    description: String::new(),
                });
            }
        }
        if lines.is_empty() {
            return None;
        }
        let plain = lines
            .iter()
            .map(|line| line.text.as_str())
            .collect::<Vec<_>>()
            .join("\n");
        Some(Self {
            lines,
            plain,
            provider: LyricsProvider::Custom,
        })
    }

    pub fn to_edit_text(&self) -> String {
        let mut out = Vec::new();
        for line in &self.lines {
            match line.time {
                Some(secs) => out.push(format!("[{}]{}", format_timestamp(secs), line.text)),
                None if line.text.starts_with('#') => out.push(format!("#{}", line.text)),
                None => out.push(line.text.clone()),
            }
            if !line.description.is_empty() {
                for note in line.description.split('\n') {
                    if note.is_empty() {
                        out.push("#".to_string());
                    } else {
                        out.push(format!("# {note}"));
                    }
                }
            }
        }
        out.join("\n")
    }

    pub fn has_timed(&self) -> bool {
        self.lines.iter().any(|line| line.time.is_some())
    }

    pub fn timed_count(&self) -> usize {
        self.lines.iter().filter(|line| line.time.is_some()).count()
    }

    pub fn has_notes(&self) -> bool {
        self.lines.iter().any(|line| !line.description.is_empty())
    }

    pub fn active_index(&self, position_secs: f32) -> Option<usize> {
        let mut idx = self.lines.iter().position(|line| line.time.is_some())?;
        for (i, line) in self.lines.iter().enumerate() {
            if let Some(t) = line.time {
                if t <= position_secs {
                    idx = i;
                } else {
                    break;
                }
            }
        }
        Some(idx)
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
            "goosemusic/0.1 (https://github.com/gooseob/music_plr)",
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
        lines: Vec::new(),
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
    let lines = synced
        .as_deref()
        .map(parse_lrc)
        .unwrap_or_default()
        .into_iter()
        .map(|(time, text)| LyricLine {
            time: Some(time),
            text,
            description: String::new(),
        })
        .collect();
    Lyrics {
        lines,
        plain,
        provider,
    }
}

pub(crate) fn format_timestamp(secs: f32) -> String {
    let total = secs.max(0.0);
    let min = (total / 60.0).floor() as u32;
    let sec = total - min as f32 * 60.0;
    format!("{min:02}:{sec:05.2}")
}

pub(crate) fn parse_lrc(text: &str) -> Vec<(f32, String)> {
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
            lines: [0.0, 10.0, 20.0]
                .into_iter()
                .zip(["a", "b", "c"])
                .map(|(time, text)| LyricLine {
                    time: Some(time),
                    text: text.into(),
                    description: String::new(),
                })
                .collect(),
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
            lines: vec![LyricLine {
                time: None,
                text: "words".into(),
                description: String::new(),
            }],
            plain: "words".into(),
            provider: LyricsProvider::LrcLib,
        };
        assert_eq!(lrc.active_index(5.0), None);
    }

    #[test]
    fn client_uses_lrclib_by_default() {
        assert_eq!(LyricsProvider::default(), LyricsProvider::LrcLib);
        assert!(LyricsProvider::all().contains(&LyricsProvider::LrcLib));
        assert!(!LyricsProvider::all().contains(&LyricsProvider::Custom));
        assert_eq!(LyricsProvider::all().len(), 3);
    }

    #[test]
    fn custom_text_without_timestamps_is_plain() {
        let lyrics = Lyrics::from_custom_text("first line\nsecond line").unwrap();
        assert!(!lyrics.has_timed());
        assert_eq!(lyrics.plain, "first line\nsecond line");
        assert_eq!(lyrics.provider, LyricsProvider::Custom);
    }

    #[test]
    fn custom_text_with_timestamps_is_synced() {
        let lyrics = Lyrics::from_custom_text("[00:12.34]First\n[00:16.80]Second").unwrap();
        assert_eq!(lyrics.timed_count(), 2);
        assert_eq!(lyrics.plain, "First\nSecond");
        assert_eq!(lyrics.to_edit_text(), "[00:12.34]First\n[00:16.80]Second");
    }

    #[test]
    fn custom_text_notes_attach_to_previous_line() {
        let lyrics =
            Lyrics::from_custom_text("[00:12.34]First\n# why it matters\n# second thought\nplain")
                .unwrap();
        assert_eq!(lyrics.lines.len(), 2);
        assert_eq!(
            lyrics.lines[0].description,
            "why it matters\nsecond thought"
        );
        assert!(lyrics.lines[1].description.is_empty());
        assert_eq!(lyrics.plain, "First\nplain");
        assert_eq!(
            lyrics.to_edit_text(),
            "[00:12.34]First\n# why it matters\n# second thought\nplain"
        );
    }

    #[test]
    fn custom_text_leading_notes_are_dropped() {
        let lyrics = Lyrics::from_custom_text("# orphan\n[00:01.00]Real").unwrap();
        assert_eq!(lyrics.lines.len(), 1);
        assert!(lyrics.lines[0].description.is_empty());
    }

    #[test]
    fn custom_text_hash_lyrics_round_trip() {
        let lyrics = Lyrics::from_custom_text("## hashtag opener\n# note").unwrap();
        assert_eq!(lyrics.lines.len(), 1);
        assert_eq!(lyrics.lines[0].text, "# hashtag opener");
        assert_eq!(lyrics.lines[0].description, "note");
        assert_eq!(lyrics.to_edit_text(), "## hashtag opener\n# note");
    }

    #[test]
    fn custom_text_notes_only_is_none() {
        assert!(Lyrics::from_custom_text("# just a note").is_none());
    }

    #[test]
    fn custom_text_empty_is_none() {
        assert!(Lyrics::from_custom_text("  \n ").is_none());
    }
}
