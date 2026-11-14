//! `NetEase` Cloud Music, a keyless CJK catalog. Search is a form POST to
//! `cloudsearch/pc`; the lyric endpoint then returns the synchronized
//! original plus optional Chinese translation (`tlyric`) and romanization
//! (`romalrc`).
//!
//! Both requests are form POSTs with a `Referer`: `NetEase` answers overseas
//! GET search requests with an encrypted payload instead of JSON.

use anyhow::{Context, Result};
use serde::Deserialize;

use super::{
    http::{agent, USER_AGENT},
    lrc::parse_lrc,
    matcher, plain_of, LyricLine, Lyrics, LyricsProvider, LyricsRequest, TranslatedLyrics,
};

const BASE: &str = "https://music.163.com/api";
const WEB_REFERER: &str = "https://music.163.com/";

pub(super) fn fetch(req: &LyricsRequest) -> Result<Option<Lyrics>> {
    let Some(id) = find_song(req)? else {
        return Ok(None);
    };
    let resp: LyricResponse = post_json(
        "song/lyric",
        &[
            ("id", id.to_string()),
            ("lv", "-1".to_string()),
            ("kv", "-1".to_string()),
            ("tv", "-1".to_string()),
            ("rv", "-1".to_string()),
            ("csrf_token", String::new()),
        ],
        "NetEase",
    )?;
    let Some(lines) = body_lines(resp.lrc) else {
        return Ok(None);
    };
    let translations = [
        translated(resp.tlyric, "zh"),
        translated(resp.romalrc, "und-Latn"),
    ]
    .into_iter()
    .flatten()
    .collect();
    Ok(Some(Lyrics {
        plain: plain_of(&lines),
        lines,
        provider: LyricsProvider::NetEase,
        translations,
    }))
}

fn find_song(req: &LyricsRequest) -> Result<Option<u64>> {
    let query = format!("{} {}", req.artist.trim(), req.title.trim());
    let query = query.trim();
    if query.is_empty() {
        return Ok(None);
    }
    let resp: SearchResponse = post_json(
        "cloudsearch/pc",
        &[
            ("s", query.to_string()),
            ("type", "1".to_string()),
            ("limit", "30".to_string()),
            ("offset", "0".to_string()),
            ("csrf_token", String::new()),
        ],
        "NetEase",
    )?;
    Ok(resp
        .result
        .songs
        .iter()
        .filter_map(|song| {
            matcher::score(req, &song.name, &song.artist_names(), song.duration_secs())
                .map(|score| (song, score))
        })
        .max_by_key(|(_, score)| *score)
        .map(|(song, _)| song.id))
}

fn post_json<T: serde::de::DeserializeOwned>(
    path: &str,
    form: &[(&str, String)],
    what: &str,
) -> Result<T> {
    let url = format!("{BASE}/{path}");
    let mut response = agent()
        .post(&url)
        .header("User-Agent", USER_AGENT)
        .header("Referer", WEB_REFERER)
        .send_form(form.iter().map(|(key, value)| (*key, value.as_str())))
        .with_context(|| format!("{what} request failed"))?;
    response
        .body_mut()
        .read_json()
        .with_context(|| format!("{what} response was not valid JSON"))
}

fn body_lines(body: Option<LyricBody>) -> Option<Vec<LyricLine>> {
    let text = body.map(|body| body.lyric).unwrap_or_default();
    let lines: Vec<LyricLine> = parse_lrc(&text)
        .into_iter()
        .map(|(time, text)| LyricLine {
            time: Some(time),
            text,
            description: String::new(),
        })
        .collect();
    lines
        .iter()
        .any(|line| !line.text.trim().is_empty())
        .then_some(lines)
}

fn translated(body: Option<LyricBody>, language: &str) -> Option<TranslatedLyrics> {
    let lines = body_lines(body)?;
    Some(TranslatedLyrics {
        language: language.to_string(),
        lyrics: Lyrics {
            plain: plain_of(&lines),
            lines,
            provider: LyricsProvider::NetEase,
            translations: Vec::new(),
        },
        source: None,
    })
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SearchResponse {
    result: SearchResult,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SearchResult {
    songs: Vec<SearchSong>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SearchSong {
    id: u64,
    name: String,
    #[serde(rename = "ar")]
    artists: Vec<SearchArtist>,
    #[serde(rename = "dt")]
    duration_ms: u64,
}

impl SearchSong {
    fn artist_names(&self) -> String {
        self.artists
            .iter()
            .map(|artist| artist.name.as_str())
            .collect::<Vec<_>>()
            .join(" ")
    }

    fn duration_secs(&self) -> Option<u32> {
        (self.duration_ms > 0).then_some((self.duration_ms / 1000) as u32)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct SearchArtist {
    name: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LyricResponse {
    lrc: Option<LyricBody>,
    tlyric: Option<LyricBody>,
    romalrc: Option<LyricBody>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct LyricBody {
    lyric: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn body(text: &str) -> LyricBody {
        LyricBody {
            lyric: text.to_string(),
        }
    }

    #[test]
    fn parses_timed_lines_and_keeps_blank_ones() {
        let lines = body_lines(Some(body(
            "[00:01.00]沈むように\n[00:02.00]溶けてゆくように\n[00:03.00]",
        )))
        .unwrap();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0].time, Some(1.0));
        assert_eq!(lines[0].text, "沈むように");
        assert_eq!(lines[2].text, "");
    }

    #[test]
    fn a_body_with_only_metadata_or_blank_lines_is_dropped() {
        assert!(body_lines(Some(body("[by:K8SAY]\n[00:03.00]"))).is_none());
        assert!(body_lines(Some(body(""))).is_none());
        assert!(body_lines(None).is_none());
    }

    #[test]
    fn translations_carry_their_language_and_lines() {
        let translation = translated(Some(body("[00:01.00]像是沉溺溶化一般\n")), "zh").unwrap();
        assert_eq!(translation.language, "zh");
        assert!(translation.is_loaded());
        assert_eq!(translation.lyrics.lines[0].text, "像是沉溺溶化一般");
        assert_eq!(translation.lyrics.provider, LyricsProvider::NetEase);
        assert!(translated(None, "zh").is_none());
    }

    #[test]
    fn search_song_maps_artists_and_millisecond_duration() {
        let song: SearchSong = serde_json::from_str(
            r#"{"id":1409311773,"name":"夜に駆ける","dt":261013,"ar":[{"name":"YOASOBI"}]}"#,
        )
        .unwrap();
        assert_eq!(song.id, 1_409_311_773);
        assert_eq!(song.artist_names(), "YOASOBI");
        assert_eq!(song.duration_secs(), Some(261));
    }
}
