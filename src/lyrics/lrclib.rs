//! `LRCLib` and its `LrcMux` mirror, which share the same API shape: a direct
//! `get` lookup first, then a `search` fallback returning the best match.

use anyhow::Result;
use serde::Deserialize;

use super::{http::get_json_opt, lrc::parse_lrc, LyricLine, Lyrics, LyricsProvider, LyricsRequest};
use crate::util::urlencode;

const LRCLIB_BASE: &str = "https://lrclib.net/api";
const LRCMUX_BASE: &str = "https://lrcmux.dev/api";

pub(super) fn fetch_lrclib(req: &LyricsRequest) -> Result<Option<Lyrics>> {
    fetch_lrclib_compat(req, LRCLIB_BASE, LyricsProvider::LrcLib)
}

pub(super) fn fetch_lrcmux(req: &LyricsRequest) -> Result<Option<Lyrics>> {
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

fn get_lrclib(req: &LyricsRequest, base: &str, provider: LyricsProvider) -> Result<Option<Lyrics>> {
    let mut url = format!(
        "{}/get?artist_name={}&track_name={}",
        base,
        urlencode(&req.artist),
        urlencode(&req.title)
    );
    if !req.album.is_empty() {
        url.push_str("&album_name=");
        url.push_str(&urlencode(&req.album));
    }
    if req.duration > 0 {
        url.push_str("&duration=");
        url.push_str(&req.duration.to_string());
    }

    let Some(resp) = get_json_opt(&url, "LRCLib-compatible")? else {
        return Ok(None);
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

    let Some(resp) = get_json_opt::<Vec<LrcLibRecord>>(&url, "LRCLib-compatible")? else {
        return Ok(None);
    };

    Ok(resp
        .into_iter()
        .next()
        .map(|rec| record_to_lyrics(rec, provider)))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LrcLibRecord {
    #[serde(rename = "syncedLyrics")]
    synced_lyrics: Option<String>,
    #[serde(rename = "plainLyrics")]
    plain_lyrics: Option<String>,
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
        translations: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn plain_only_record_has_editable_text() {
        let rec = LrcLibRecord {
            synced_lyrics: None,
            plain_lyrics: Some("first line\nsecond line".into()),
        };
        let lyrics = record_to_lyrics(rec, LyricsProvider::LrcLib);
        assert!(lyrics.lines.is_empty());
        assert_eq!(lyrics.to_edit_text(), "first line\nsecond line");
    }
}
