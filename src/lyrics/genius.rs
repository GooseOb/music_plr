//! Genius is keyless: the same `genius.com/api` endpoints the website itself
//! uses answer without a token. Lyrics are not in the API, so the song page
//! HTML is scraped (`data-lyrics-container` blocks); per-line annotations come
//! from `referents` and the song blurb from `songs/{id}`, both mapped onto
//! `LyricLine.description` notes.

use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
};

use anyhow::{Context, Result};
use serde::Deserialize;

use super::{
    http::agent, matcher, plain_of, LyricLine, Lyrics, LyricsProvider, LyricsRequest,
    TranslatedLyrics,
};
use crate::util::urlencode;

const GENIUS_BASE: &str = "https://genius.com/api";

const GENIUS_USER_AGENT: &str = concat!(
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 ",
    "(KHTML, like Gecko) Chrome/126.0 Safari/537.36 goosemusic/0.1"
);

pub(super) fn fetch(req: &LyricsRequest) -> Result<Option<Lyrics>> {
    let query = format!("{} {}", req.artist.trim(), req.title.trim());
    let query = query.trim();
    if query.is_empty() {
        return Ok(None);
    }
    let url = format!(
        "{GENIUS_BASE}/search/multi?per_page=5&q={}",
        urlencode(query)
    );
    let Some(search) = get_json::<GeniusSearchResponse>(&url, "Genius")? else {
        return Ok(None);
    };
    let songs = search
        .response
        .sections
        .iter()
        .flat_map(|s| s.hits.iter())
        .filter_map(|h| h.result.as_ref())
        .filter(|r| r.kind == "song");
    let Some((hit, _)) = songs
        .filter_map(|h| score_hit(req, h).map(|score| (h, score)))
        .max_by_key(|(_, score)| *score)
    else {
        return Ok(None);
    };

    let Some(html) = get_html(&hit.url, "Genius")? else {
        return Ok(None);
    };
    let mut lines = scrape_lyrics(&html);
    if lines.is_empty() {
        return Ok(None);
    }

    let annotations = fetch_annotations(hit.id).unwrap_or_default();
    if !annotations.is_empty() {
        for line in &mut lines {
            if let Some(notes) = annotations.get(&matcher::normalize(&line.text)) {
                line.description = notes.join("\n\n");
            }
        }
    }
    let mut translations = Vec::new();
    if let Ok(Some(details)) = fetch_song_details(&hit.api_path) {
        if let Some(blurb) = details.blurb {
            if let Some(first) = lines.first_mut() {
                if first.description.is_empty() {
                    first.description = format!("About this song:\n{blurb}");
                } else {
                    first.description.push_str("\n\nAbout this song:\n");
                    first.description.push_str(&blurb);
                }
            }
        }
        translations = details.translations;
    }

    let plain = plain_of(&lines);
    Ok(Some(Lyrics {
        lines,
        plain,
        provider: LyricsProvider::Genius,
        translations,
    }))
}

/// Scrape a translation page (a separate Genius song) into a loaded entry.
pub(super) fn fetch_translation(language: &str, url: &str) -> Result<Option<TranslatedLyrics>> {
    let Some(html) = get_html(url, "Genius")? else {
        return Ok(None);
    };
    let lines = scrape_lyrics(&html);
    if lines.is_empty() {
        return Ok(None);
    }
    let plain = plain_of(&lines);
    Ok(Some(TranslatedLyrics {
        language: language.to_string(),
        lyrics: Lyrics {
            lines,
            plain,
            provider: LyricsProvider::Genius,
            translations: Vec::new(),
        },
        source: None,
    }))
}

/// Lyrics text lines from a Genius song page. The server-rendered
/// `data-lyrics-container` blocks are sometimes truncated to a preview, so
/// the embedded `__PRELOADED_STATE__` (`songPage.lyricsData.body.html`, the
/// same markup the client renders) wins when present; the containers are the
/// fallback for pages without it.
fn scrape_lyrics(html: &str) -> Vec<LyricLine> {
    let mut texts = Vec::new();
    if let Some(body) = preloaded_body(html) {
        texts.extend(html_to_lines(&body));
    }
    if texts.is_empty() {
        for block in containers(html) {
            if block.contains("ContributorsCredit") || block.contains("LyricsHeader") {
                continue;
            }
            texts.extend(html_to_lines(block));
        }
    }
    texts
        .into_iter()
        .map(|text| LyricLine {
            time: None,
            text,
            description: String::new(),
        })
        .collect()
}

/// Full-lyrics markup from the page's embedded JSON state, or `None` when
/// the page carries none (or only a placeholder, e.g. instrumentals).
fn preloaded_body(html: &str) -> Option<String> {
    const MARKER: &str = "__PRELOADED_STATE__ = JSON.parse('";
    let start = html.find(MARKER)? + MARKER.len();
    let bytes = html.as_bytes();
    let mut i = start;
    let end = loop {
        let byte = *bytes.get(i)?;
        if byte == b'\\' {
            i += 2;
            continue;
        }
        if byte == b'\'' && bytes.get(i + 1) == Some(&b')') {
            break i;
        }
        i += 1;
    };
    let state: serde_json::Value = serde_json::from_str(&js_unescape(&html[start..end])).ok()?;
    let lyrics = state.pointer("/songPage/lyricsData")?;
    if !lyrics
        .pointer("/lyricsPlaceholderReason")
        .is_none_or(serde_json::Value::is_null)
    {
        return None;
    }
    let body = lyrics.pointer("/body/html")?.as_str()?;
    if body.trim().is_empty() {
        return None;
    }
    Some(body.to_string())
}

/// Unescape a single-quoted JS string literal into JSON text. Unknown
/// escapes resolve JS-style (the char itself); octal escapes and any
/// produced control characters are emitted as `\uXXXX` so the result stays
/// valid JSON.
fn js_unescape(payload: &str) -> String {
    let mut unescaped = String::with_capacity(payload.len());
    let mut chars = payload.chars();
    while let Some(c) = chars.next() {
        if c != '\\' {
            unescaped.push(c);
            continue;
        }
        let Some(q) = chars.next() else {
            unescaped.push('\\');
            break;
        };
        match q {
            'n' => unescaped.push('\n'),
            'r' => unescaped.push('\r'),
            't' => unescaped.push('\t'),
            'b' => unescaped.push('\u{8}'),
            'f' => unescaped.push('\u{c}'),
            'u' => push_js_unicode(&mut chars, &mut unescaped),
            '0'..='7' => push_js_octal(q, &mut chars, &mut unescaped),
            _ => unescaped.push(q),
        }
    }
    let mut safe = String::with_capacity(unescaped.len());
    for c in unescaped.chars() {
        if (c as u32) < 0x20 && !matches!(c, '\t' | '\n' | '\r') {
            let _ = write!(safe, "\\u{:04x}", c as u32);
        } else {
            safe.push(c);
        }
    }
    safe
}

fn push_js_unicode(chars: &mut std::str::Chars<'_>, out: &mut String) {
    let hex: String = chars.clone().take(4).collect();
    if hex.len() != 4 || !hex.chars().all(|c| c.is_ascii_hexdigit()) {
        out.push('u');
        return;
    }
    let Ok(high) = u32::from_str_radix(&hex, 16) else {
        out.push('u');
        return;
    };
    for _ in 0..4 {
        chars.next();
    }
    if (0xd800..0xdc00).contains(&high) {
        let tail: String = chars.clone().take(6).collect();
        if let Some(low) = tail
            .strip_prefix("\\u")
            .filter(|h| h.len() == 4 && h.chars().all(|c| c.is_ascii_hexdigit()))
            .and_then(|h| u32::from_str_radix(h, 16).ok())
            .filter(|low| (0xdc00..0xe000).contains(low))
        {
            for _ in 0..6 {
                chars.next();
            }
            let n = 0x1_0000 + ((high - 0xd800) << 10) + (low - 0xdc00);
            if let Some(c) = char::from_u32(n) {
                out.push(c);
                return;
            }
        }
        out.push('\u{fffd}');
        return;
    }
    match char::from_u32(high) {
        Some(c) => out.push(c),
        None => out.push('\u{fffd}'),
    }
}

fn push_js_octal(first: char, chars: &mut std::str::Chars<'_>, out: &mut String) {
    let mut digits = String::from(first);
    for _ in 0..2 {
        if matches!(chars.clone().next(), Some('0'..='7')) {
            digits.push(chars.next().unwrap_or('0'));
        } else {
            break;
        }
    }
    let n = u32::from_str_radix(&digits, 8).unwrap_or(0);
    match char::from_u32(n) {
        Some(c) if (c as u32) >= 0x20 => out.push(c),
        _ => {
            let _ = write!(out, "\\u{n:04x}");
        }
    }
}

fn get_json<T: serde::de::DeserializeOwned>(url: &str, what: &str) -> Result<Option<T>> {
    match agent()
        .get(url)
        .header("Accept", "application/json")
        .header("User-Agent", GENIUS_USER_AGENT)
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

fn get_html(url: &str, what: &str) -> Result<Option<String>> {
    match agent()
        .get(url)
        .header("Accept", "text/html")
        .header("User-Agent", GENIUS_USER_AGENT)
        .call()
    {
        Ok(mut r) => {
            Ok(Some(r.body_mut().read_to_string().with_context(|| {
                format!("{what} response was not valid text")
            })?))
        }
        Err(ureq::Error::StatusCode(404)) => Ok(None),
        Err(e) => Err(e).with_context(|| format!("{what} request failed")),
    }
}

/// Relevance score for a Genius song hit, or `None` when it is unusable
/// (instrumental, lyrics missing) or matches the wrong song. Title must
/// overlap either way; a non-empty artist that overlaps neither way rejects
/// the hit so same-title covers don't win.
fn score_hit(req: &LyricsRequest, hit: &GeniusSongHit) -> Option<u32> {
    if hit.instrumental {
        return None;
    }
    if !hit.lyrics_state.is_empty() && hit.lyrics_state != "complete" {
        return None;
    }
    matcher::score(req, &hit.title, &hit.artist_names, None)
}

/// Annotation bodies keyed by normalized lyric line. Referent fragments span
/// one or more lines, so each fragment line maps to the same body.
fn fetch_annotations(song_id: u64) -> Result<HashMap<String, Vec<String>>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for page in 1..=3 {
        let url = format!(
            "{GENIUS_BASE}/referents?song_id={song_id}&text_format=plain&per_page=50&page={page}"
        );
        let resp: GeniusReferentsResponse = match get_json(&url, "Genius")? {
            Some(body) => body,
            None => break,
        };
        for referent in &resp.response.referents {
            let body = referent
                .annotations
                .iter()
                .filter_map(|a| a.body.as_ref())
                .map(|b| b.plain.trim())
                .find(|b| !b.is_empty());
            let Some(body) = body else { continue };
            for line in referent.range.content.lines() {
                let key = matcher::normalize(line);
                if !key.is_empty() {
                    out.entry(key).or_default().push(body.to_string());
                }
            }
        }
        if resp.response.next_page.is_none() {
            break;
        }
    }
    Ok(out)
}

struct GeniusSongDetails {
    blurb: Option<String>,
    translations: Vec<TranslatedLyrics>,
}

fn fetch_song_details(api_path: &str) -> Result<Option<GeniusSongDetails>> {
    let url = format!("{GENIUS_BASE}{api_path}?text_format=plain");
    let resp: GeniusSongResponse = match get_json(&url, "Genius")? {
        Some(body) => body,
        None => return Ok(None),
    };
    let song = resp.response.song;
    let blurb = song
        .description
        .as_ref()
        .map(|d| d.plain.trim())
        .filter(|d| !d.is_empty())
        .map(str::to_string);
    let mut seen = HashSet::new();
    let translations = song
        .translation_songs
        .iter()
        .filter(|t| {
            !t.hidden
                && t.lyrics_state == "complete"
                && !t.language.is_empty()
                && t.language != song.language
                && seen.insert(t.language.clone())
        })
        .map(|t| TranslatedLyrics::unloaded(&t.language, &t.url))
        .collect();
    Ok(Some(GeniusSongDetails {
        blurb,
        translations,
    }))
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeniusSearchResponse {
    response: GeniusSearchInner,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeniusSearchInner {
    sections: Vec<GeniusSection>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeniusSection {
    hits: Vec<GeniusHit>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeniusHit {
    result: Option<GeniusSongHit>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeniusSongHit {
    #[serde(rename = "_type")]
    kind: String,
    id: u64,
    title: String,
    artist_names: String,
    url: String,
    api_path: String,
    lyrics_state: String,
    instrumental: bool,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeniusReferentsResponse {
    response: GeniusReferentsInner,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeniusReferentsInner {
    referents: Vec<GeniusReferent>,
    next_page: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeniusReferent {
    range: GeniusRange,
    annotations: Vec<GeniusAnnotation>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeniusRange {
    content: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeniusAnnotation {
    body: Option<GeniusText>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeniusText {
    plain: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeniusSongResponse {
    response: GeniusSongInner,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeniusSongInner {
    song: GeniusSong,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeniusSong {
    description: Option<GeniusText>,
    language: String,
    translation_songs: Vec<GeniusTranslationHit>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(default)]
struct GeniusTranslationHit {
    language: String,
    url: String,
    lyrics_state: String,
    hidden: bool,
}

/// Inner HTML of every `data-lyrics-container` block on a Genius song page.
/// Depth-counted so nested `<div>`s don't cut the block short.
fn containers(html: &str) -> Vec<&str> {
    const MARKER: &str = "data-lyrics-container=\"true\"";
    let mut out = Vec::new();
    let mut search = 0;
    while let Some(found) = html[search..].find(MARKER) {
        let marker = search + found;
        let Some(tag_end) = html[marker..].find('>').map(|i| marker + i + 1) else {
            break;
        };
        let mut depth = 1;
        let mut i = tag_end;
        let mut end = None;
        while depth > 0 {
            let Some(lt) = html[i..].find('<').map(|p| i + p) else {
                break;
            };
            if html[lt..].starts_with("</div") {
                depth -= 1;
                if depth == 0 {
                    end = Some(lt);
                    break;
                }
                i = lt + 5;
            } else if html[lt..].starts_with("<div") {
                let Some(gt) = html[lt..].find('>').map(|p| lt + p) else {
                    break;
                };
                if !html[lt..=gt].ends_with("/>") {
                    depth += 1;
                }
                i = gt + 1;
            } else {
                i = lt + 1;
            }
        }
        match end {
            Some(e) => {
                out.push(&html[tag_end..e]);
                search = e + 6;
            }
            None => break,
        }
    }
    out
}

/// Flatten a lyrics container to text lines: `<br>`s become newlines, all
/// other tags are stripped, entities decoded, blank lines dropped.
fn html_to_lines(block: &str) -> Vec<String> {
    let mut text = String::with_capacity(block.len());
    let mut i = 0;
    while i < block.len() {
        if block[i..].starts_with('<') {
            if block[i..].starts_with("<br") {
                text.push('\n');
            }
            match block[i..].find('>') {
                Some(p) => i += p + 1,
                None => break,
            }
        } else {
            let ch = block[i..].chars().next().unwrap_or('\0');
            if ch == '\0' {
                break;
            }
            text.push(ch);
            i += ch.len_utf8();
        }
    }
    decode_entities(&text)
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect()
}

fn decode_entities(text: &str) -> String {
    if !text.contains('&') {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    let mut i = 0;
    while i < text.len() {
        if !text[i..].starts_with('&') {
            let ch = text[i..].chars().next().unwrap_or('\0');
            if ch == '\0' {
                break;
            }
            out.push(ch);
            i += ch.len_utf8();
            continue;
        }
        let semi = text[i..].find(';').filter(|&p| p <= 12).map(|p| i + p);
        let Some(semi) = semi else {
            out.push('&');
            i += 1;
            continue;
        };
        let entity = &text[i + 1..semi];
        let decoded = match entity {
            "amp" => Some('&'),
            "lt" => Some('<'),
            "gt" => Some('>'),
            "quot" => Some('"'),
            "apos" | "#39" => Some('\''),
            "nbsp" => Some('\u{a0}'),
            _ if entity.starts_with("#x") || entity.starts_with("#X") => {
                u32::from_str_radix(&entity[2..], 16)
                    .ok()
                    .and_then(char::from_u32)
            }
            _ if entity.starts_with('#') => {
                entity[1..].parse::<u32>().ok().and_then(char::from_u32)
            }
            _ => None,
        };
        if let Some(ch) = decoded {
            out.push(ch);
            i = semi + 1;
        } else {
            out.push('&');
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{
        containers, decode_entities, html_to_lines, js_unescape, matcher, preloaded_body,
        score_hit, scrape_lyrics, GeniusHit, GeniusSongHit, GeniusSongResponse,
    };
    use crate::lyrics::LyricsRequest;

    fn req(artist: &str, title: &str) -> LyricsRequest {
        LyricsRequest {
            artist: artist.into(),
            title: title.into(),
            album: String::new(),
            duration: 0,
        }
    }

    fn hit(title: &str, artist: &str) -> GeniusSongHit {
        GeniusSongHit {
            kind: "song".into(),
            id: 1,
            title: title.into(),
            artist_names: artist.into(),
            url: "https://genius.com/x".into(),
            api_path: "/songs/1".into(),
            lyrics_state: "complete".into(),
            instrumental: false,
        }
    }

    #[test]
    fn hit_scoring_accepts_match() {
        let req = req("Nirvana", "Smells Like Teen Spirit");
        let hit = hit("Smells Like Teen Spirit", "Nirvana");
        assert!(score_hit(&req, &hit).is_some());
    }

    #[test]
    fn hit_scoring_accepts_featured_artist_overlap() {
        let req = req("Nirvana", "Smells Like Teen Spirit (Official Video)");
        let hit = hit("Smells Like Teen Spirit", "Nirvana");
        assert!(score_hit(&req, &hit).is_some());
    }

    #[test]
    fn hit_scoring_rejects_wrong_artist_and_title() {
        let req = req("Nirvana", "Smells Like Teen Spirit");
        assert!(score_hit(&req, &hit("Smells Like Teen Spirit", "Weezer")).is_none());
        assert!(score_hit(&req, &hit("Come As You Are", "Nirvana")).is_none());
    }

    #[test]
    fn hit_scoring_skips_instrumental_and_incomplete() {
        let req = req("Nirvana", "Smells Like Teen Spirit");
        let mut hit = hit("Smells Like Teen Spirit", "Nirvana");
        hit.instrumental = true;
        assert!(score_hit(&req, &hit).is_none());
        hit.instrumental = false;
        hit.lyrics_state = "unreleased".into();
        assert!(score_hit(&req, &hit).is_none());
    }

    #[test]
    fn hit_deserializes_search_result() {
        let json = r#"{"result":{"_type":"song","id":52968,"title":"Smells Like Teen Spirit","artist_names":"Nirvana","url":"https://genius.com/Nirvana-smells-like-teen-spirit-lyrics","api_path":"/songs/52968","lyrics_state":"complete","instrumental":false}}"#;
        let hit: GeniusHit = serde_json::from_str(json).unwrap();
        let song = hit.result.unwrap();
        assert_eq!(song.kind, "song");
        assert_eq!(song.id, 52968);
        let req = req("Nirvana", "Smells Like Teen Spirit");
        assert!(score_hit(&req, &song).is_some());
    }

    #[test]
    fn decode_entities_handles_named_and_numeric() {
        assert_eq!(
            decode_entities("it&#x27;s &amp; &quot;us&quot; &#39;ok&#39;"),
            "it's & \"us\" 'ok'"
        );
        assert_eq!(decode_entities("a &unknown; b & c"), "a &unknown; b & c");
    }

    #[test]
    fn containers_extract_nested_blocks() {
        let html = concat!(
            r#"<div data-lyrics-container="true" class="x">First<br/>Second<span>nested<div>deep</div></span></div>"#,
            r#"<div data-lyrics-container="true" class="x"><br/>[Chorus]<br/>Line</div>"#,
        );
        let blocks = containers(html);
        assert_eq!(blocks.len(), 2);
        let lines = html_to_lines(blocks[0]);
        assert_eq!(lines, vec!["First", "Secondnesteddeep"]);
        assert_eq!(html_to_lines(blocks[1]), vec!["[Chorus]", "Line"]);
    }

    #[test]
    fn js_unescape_handles_quotes_and_unicode() {
        assert_eq!(js_unescape("it\\'s \\u00e9"), "it's é");
        assert_eq!(js_unescape("a\\\"b\\\\c\\/d"), "a\"b\\c/d");
        assert_eq!(js_unescape("line\\nbreak"), "line\nbreak");
        assert_eq!(js_unescape("\\$5"), "$5");
        assert_eq!(js_unescape("\\2"), "\\u0002");
        assert_eq!(js_unescape("\\uD83D\\uDE00"), "😀");
        assert_eq!(js_unescape("trailing\\"), "trailing\\");
    }

    fn preloaded_page(body_html: &str) -> String {
        let mut state = String::from(
            r#"{"songPage":{"lyricsData":{"lyricsPlaceholderReason":null,"body":{"html":"#,
        );
        // `serde_json` escapes `body_html` exactly like the page's `JSON.parse` payload.
        state.push_str(&serde_json::to_string(body_html).unwrap());
        state.push_str(r"}}}}");
        let escaped = state.replace('\\', "\\\\").replace('\'', "\\'");
        ["x __PRELOADED_STATE__ = JSON.parse('", &escaped, "'); y"].concat()
    }

    #[test]
    fn preloaded_body_extracts_embedded_lyrics() {
        let html = preloaded_page("<p>First o'ne<br/>\nSecond &amp; last</p>");
        let body = preloaded_body(&html).unwrap();
        assert!(body.contains("First o'ne"), "{body}");
        let lines = html_to_lines(&body);
        assert_eq!(lines, vec!["First o'ne", "Second & last"]);
    }

    #[test]
    fn preloaded_body_rejects_placeholders_and_missing_state() {
        assert!(preloaded_body("<html>no state</html>").is_none());
        let state = r#"{"songPage":{"lyricsData":{"lyricsPlaceholderReason":"instrumental","body":{"html":"<p>nope</p>"}}}}"#
            .replace('\\', "\\\\")
            .replace('\'', "\\'");
        let html = format!("__PRELOADED_STATE__ = JSON.parse('{state}');");
        assert!(preloaded_body(&html).is_none());
    }

    #[test]
    fn scrape_prefers_preloaded_body_over_truncated_containers() {
        let mut html =
            String::from(r#"<div data-lyrics-container="true" class="x">Preview<br/>Only</div>"#);
        html.push_str(&preloaded_page("<p>Full<br/>Song<br/>Here</p>"));
        let lines = scrape_lyrics(&html);
        assert_eq!(
            lines.iter().map(|l| l.text.as_str()).collect::<Vec<_>>(),
            vec!["Full", "Song", "Here"]
        );
    }

    #[test]
    fn html_to_lines_strips_annotated_links() {
        let block = concat!(
            "[Chorus]<br/>With the lights out, it&#x27;s less dangerous<br/>",
            r#"<a href="/x" class="y"><span>Here we are now, entertain us<br/>I feel stupid</span></a>"#,
        );
        assert_eq!(
            html_to_lines(block),
            vec![
                "[Chorus]",
                "With the lights out, it's less dangerous",
                "Here we are now, entertain us",
                "I feel stupid",
            ]
        );
    }

    #[test]
    fn song_deserializes_translation_songs() {
        let json = r#"{"response":{"song":{"language":"en","description":{"plain":"blurb"},"translation_songs":[{"language":"ru","url":"https://genius.com/x-lyrics","lyrics_state":"complete","hidden":false},{"language":"","url":"https://genius.com/y","lyrics_state":"complete","hidden":false}]}}}"#;
        let resp: GeniusSongResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.response.song.translation_songs.len(), 2);
        assert_eq!(resp.response.song.translation_songs[0].language, "ru");
    }

    #[test]
    fn annotation_key_matches_despite_quote_style() {
        let fragment = "With the lights out, it’s less dangerous";
        let line = "With the lights out, it's less dangerous";
        assert_eq!(matcher::normalize(fragment), matcher::normalize(line));
        let multi = "Hello, hello, hello, how low\n Hello, hello, hello";
        let keys: Vec<String> = multi.lines().map(matcher::normalize).collect();
        assert_eq!(
            keys,
            vec!["hello, hello, hello, how low", "hello, hello, hello"]
        );
    }
}
