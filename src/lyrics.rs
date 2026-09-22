//! Lyrics fetching, backed by a handful of free, no-API-key providers.

use std::{collections::HashMap, fmt::Write as _};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::util::urlencode;

pub const LRCLIB_BASE: &str = "https://lrclib.net/api";
pub const LRCMUX_BASE: &str = "https://lrcmux.dev/api";
pub const LYRICS_OVH_BASE: &str = "https://api.lyrics.ovh/v1";
pub const GENIUS_BASE: &str = "https://genius.com/api";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, serde::Serialize, serde::Deserialize)]
pub enum LyricsProvider {
    #[default]
    #[serde(rename = "lrclib")]
    LrcLib,
    #[serde(rename = "lrcmux")]
    LrcMux,
    #[serde(rename = "lyrics_ovh")]
    LyricsOvh,
    #[serde(rename = "genius")]
    Genius,
    #[serde(rename = "custom")]
    Custom,
}

impl LyricsProvider {
    pub fn name(self) -> &'static str {
        match self {
            LyricsProvider::LrcLib => "LRCLib",
            LyricsProvider::LrcMux => "LrcMux",
            LyricsProvider::LyricsOvh => "Lyrics.ovh",
            LyricsProvider::Genius => "Genius",
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
            LyricsProvider::Genius,
            LyricsProvider::LyricsOvh,
        ]
    }

    pub fn fetch(self, req: &LyricsRequest) -> Result<Option<Lyrics>> {
        match self {
            LyricsProvider::LrcLib => fetch_lrclib(req),
            LyricsProvider::LrcMux => fetch_lrcmux(req),
            LyricsProvider::LyricsOvh => fetch_lyrics_ovh(req),
            LyricsProvider::Genius => fetch_genius(req),
            LyricsProvider::Custom => Ok(None),
        }
    }

    /// Fetch one lazy translation by its opaque `source` key from an
    /// unloaded [`TranslatedLyrics`]. Only Genius stores translations, so
    /// every other provider returns `Ok(None)`.
    pub fn fetch_translation(
        self,
        language: &str,
        source: &str,
    ) -> Result<Option<TranslatedLyrics>> {
        match self {
            LyricsProvider::Genius => fetch_genius_translation(language, source),
            _ => Ok(None),
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

#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Lyrics {
    pub lines: Vec<LyricLine>,
    pub plain: String,
    pub provider: LyricsProvider,
    #[serde(default)]
    pub translations: Vec<TranslatedLyrics>,
}

/// A translated version of a track's lyrics. `lyrics` is empty while
/// `source` holds the opaque provider fetch key; selecting the language
/// lazy-loads the content and clears `source`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct TranslatedLyrics {
    pub language: String,
    pub lyrics: Lyrics,
    #[serde(default)]
    pub source: Option<String>,
}

impl TranslatedLyrics {
    pub fn is_loaded(&self) -> bool {
        self.source.is_none()
    }

    pub fn unloaded(language: &str, source: &str) -> Self {
        Self {
            language: language.to_string(),
            lyrics: Lyrics {
                lines: Vec::new(),
                plain: String::new(),
                provider: LyricsProvider::Genius,
                translations: Vec::new(),
            },
            source: Some(source.to_string()),
        }
    }
}

/// Display name for an ISO 639 language code; unknown codes fall back to
/// the uppercased code itself.
pub fn language_name(code: &str) -> String {
    match code {
        "af" => "Afrikaans",
        "am" => "Amharic",
        "ar" => "Arabic",
        "az" => "Azerbaijani",
        "be" => "Belarusian",
        "bg" => "Bulgarian",
        "bn" => "Bengali",
        "ca" => "Catalan",
        "cs" => "Czech",
        "cy" => "Welsh",
        "da" => "Danish",
        "de" => "German",
        "el" => "Greek",
        "en" => "English",
        "es" => "Spanish",
        "et" => "Estonian",
        "eu" => "Basque",
        "fa" => "Persian",
        "fi" => "Finnish",
        "fr" => "French",
        "ga" => "Irish",
        "gl" => "Galician",
        "gu" => "Gujarati",
        "ha" => "Hausa",
        "he" | "iw" => "Hebrew",
        "hi" => "Hindi",
        "hr" => "Croatian",
        "hu" => "Hungarian",
        "hy" => "Armenian",
        "id" => "Indonesian",
        "is" => "Icelandic",
        "it" => "Italian",
        "ja" => "Japanese",
        "ka" => "Georgian",
        "kk" => "Kazakh",
        "km" => "Khmer",
        "kn" => "Kannada",
        "ko" => "Korean",
        "ky" => "Kyrgyz",
        "lo" => "Lao",
        "lt" => "Lithuanian",
        "lv" => "Latvian",
        "mk" => "Macedonian",
        "ml" => "Malayalam",
        "mn" => "Mongolian",
        "mr" => "Marathi",
        "ms" => "Malay",
        "my" => "Burmese",
        "nb" | "no" => "Norwegian",
        "ne" => "Nepali",
        "nl" => "Dutch",
        "pa" => "Punjabi",
        "pl" => "Polish",
        "pt" => "Portuguese",
        "ro" => "Romanian",
        "ru" => "Russian",
        "si" => "Sinhala",
        "sk" => "Slovak",
        "sl" => "Slovenian",
        "sr" => "Serbian",
        "sv" => "Swedish",
        "sw" => "Swahili",
        "ta" => "Tamil",
        "te" => "Telugu",
        "tg" => "Tajik",
        "th" => "Thai",
        "tk" => "Turkmen",
        "tl" => "Tagalog",
        "tr" => "Turkish",
        "uk" => "Ukrainian",
        "ur" => "Urdu",
        "uz" => "Uzbek",
        "vi" => "Vietnamese",
        "yo" => "Yoruba",
        "zh" => "Chinese",
        "zu" => "Zulu",
        _ => return code.to_ascii_uppercase(),
    }
    .to_string()
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
            translations: Vec::new(),
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
        translations: Vec::new(),
    }))
}

// Genius is keyless: the same `genius.com/api` endpoints the website
// itself uses answer without a token. Lyrics are not in the API, so the
// song page HTML is scraped (`data-lyrics-container` blocks); per-line
// annotations come from `referents` and the song blurb from `songs/{id}`,
// both mapped onto `LyricLine.description` notes.
fn fetch_genius(req: &LyricsRequest) -> Result<Option<Lyrics>> {
    let query = format!("{} {}", req.artist.trim(), req.title.trim());
    let query = query.trim();
    if query.is_empty() {
        return Ok(None);
    }
    let url = format!(
        "{GENIUS_BASE}/search/multi?per_page=5&q={}",
        urlencode(query)
    );
    let search: GeniusSearchResponse = match get_genius_json(&url, "Genius")? {
        Some(body) => body,
        None => return Ok(None),
    };
    let songs: Vec<&GeniusSongHit> = search
        .response
        .sections
        .iter()
        .flat_map(|s| s.hits.iter())
        .filter_map(|h| h.result.as_ref())
        .filter(|r| r.kind == "song")
        .collect();
    let Some(hit) = songs
        .iter()
        .filter_map(|h| score_genius_hit(req, h).map(|score| (*h, score)))
        .max_by_key(|(_, score)| *score)
        .map(|(hit, _)| hit)
    else {
        return Ok(None);
    };

    let Some(html) = get_genius_html(&hit.url, "Genius")? else {
        return Ok(None);
    };
    let mut lines = scrape_genius_lyrics(&html);
    if lines.is_empty() {
        return Ok(None);
    }

    let annotations = fetch_genius_annotations(hit.id).unwrap_or_default();
    if !annotations.is_empty() {
        for line in &mut lines {
            if let Some(notes) = annotations.get(&normalize_lyric(&line.text)) {
                line.description = notes.join("\n\n");
            }
        }
    }
    let mut translations = Vec::new();
    if let Ok(Some(details)) = fetch_genius_song_details(&hit.api_path) {
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
/// Only Genius stores translations; other providers return `Ok(None)`.
fn fetch_genius_translation(language: &str, url: &str) -> Result<Option<TranslatedLyrics>> {
    let Some(html) = get_genius_html(url, "Genius")? else {
        return Ok(None);
    };
    let lines = scrape_genius_lyrics(&html);
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

fn plain_of(lines: &[LyricLine]) -> String {
    lines
        .iter()
        .map(|line| line.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Lyrics text lines from a Genius song page: header blocks skipped, `<br>`s
/// become line breaks, annotated fragments unwrapped.
fn scrape_genius_lyrics(html: &str) -> Vec<LyricLine> {
    let mut texts = Vec::new();
    for block in genius_containers(html) {
        if block.contains("ContributorsCredit") || block.contains("LyricsHeader") {
            continue;
        }
        texts.extend(html_to_lines(block));
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

fn get_genius_json<T: serde::de::DeserializeOwned>(url: &str, what: &str) -> Result<Option<T>> {
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

fn get_genius_html(url: &str, what: &str) -> Result<Option<String>> {
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

const GENIUS_USER_AGENT: &str = concat!(
    "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 ",
    "(KHTML, like Gecko) Chrome/126.0 Safari/537.36 goosemusic/0.1"
);

/// Relevance score for a Genius song hit, or `None` when it is unusable
/// (instrumental, lyrics missing) or matches the wrong song. Title must
/// overlap either way; a non-empty artist that overlaps neither way rejects
/// the hit so same-title covers don't win.
fn score_genius_hit(req: &LyricsRequest, hit: &GeniusSongHit) -> Option<u32> {
    if hit.instrumental {
        return None;
    }
    if !hit.lyrics_state.is_empty() && hit.lyrics_state != "complete" {
        return None;
    }
    let title = normalize_lyric(&req.title);
    let artist = normalize_lyric(&req.artist);
    let hit_title = normalize_lyric(&hit.title);
    let hit_artist = normalize_lyric(&hit.artist_names);
    if title.is_empty() || hit_title.is_empty() {
        return None;
    }
    if !hit_title.contains(&title) && !title.contains(&hit_title) {
        return None;
    }
    let mut score = 2;
    if !artist.is_empty() && !hit_artist.is_empty() {
        if hit_artist == artist {
            score += 3;
        } else if hit_artist.contains(&artist) || artist.contains(&hit_artist) {
            score += 2;
        } else {
            return None;
        }
    }
    if hit_title == title {
        score += 2;
    }
    Some(score)
}

/// Annotation bodies keyed by normalized lyric line. Referent fragments span
/// one or more lines, so each fragment line maps to the same body.
fn fetch_genius_annotations(song_id: u64) -> Result<HashMap<String, Vec<String>>> {
    let mut out: HashMap<String, Vec<String>> = HashMap::new();
    for page in 1..=3 {
        let url = format!(
            "{GENIUS_BASE}/referents?song_id={song_id}&text_format=plain&per_page=50&page={page}"
        );
        let resp: GeniusReferentsResponse = match get_genius_json(&url, "Genius")? {
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
                let key = normalize_lyric(line);
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

fn fetch_genius_song_details(api_path: &str) -> Result<Option<GeniusSongDetails>> {
    let url = format!("{GENIUS_BASE}{api_path}?text_format=plain");
    let resp: GeniusSongResponse = match get_genius_json(&url, "Genius")? {
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
    let mut seen = std::collections::HashSet::new();
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

#[derive(Debug, Deserialize)]
struct GeniusSearchResponse {
    #[serde(default)]
    response: GeniusSearchInner,
}

#[derive(Debug, Deserialize, Default)]
struct GeniusSearchInner {
    #[serde(default)]
    sections: Vec<GeniusSection>,
}

#[derive(Debug, Deserialize)]
struct GeniusSection {
    #[serde(default)]
    hits: Vec<GeniusHit>,
}

#[derive(Debug, Deserialize)]
struct GeniusHit {
    #[serde(default)]
    result: Option<GeniusSongHit>,
}

#[derive(Debug, Deserialize, Default)]
struct GeniusSongHit {
    #[serde(rename = "_type", default)]
    kind: String,
    #[serde(default)]
    id: u64,
    #[serde(default)]
    title: String,
    #[serde(default)]
    artist_names: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    api_path: String,
    #[serde(default)]
    lyrics_state: String,
    #[serde(default)]
    instrumental: bool,
}

#[derive(Debug, Deserialize)]
struct GeniusReferentsResponse {
    #[serde(default)]
    response: GeniusReferentsInner,
}

#[derive(Debug, Deserialize, Default)]
struct GeniusReferentsInner {
    #[serde(default)]
    referents: Vec<GeniusReferent>,
    #[serde(default)]
    next_page: Option<u32>,
}

#[derive(Debug, Deserialize, Default)]
struct GeniusReferent {
    #[serde(default)]
    range: GeniusRange,
    #[serde(default)]
    annotations: Vec<GeniusAnnotation>,
}

#[derive(Debug, Deserialize, Default)]
struct GeniusRange {
    #[serde(default)]
    content: String,
}

#[derive(Debug, Deserialize)]
struct GeniusAnnotation {
    #[serde(default)]
    body: Option<GeniusText>,
}

#[derive(Debug, Deserialize, Default)]
struct GeniusText {
    #[serde(default)]
    plain: String,
}

#[derive(Debug, Deserialize)]
struct GeniusSongResponse {
    #[serde(default)]
    response: GeniusSongInner,
}

#[derive(Debug, Deserialize, Default)]
struct GeniusSongInner {
    #[serde(default)]
    song: GeniusSong,
}

#[derive(Debug, Deserialize, Default)]
struct GeniusSong {
    #[serde(default)]
    description: Option<GeniusText>,
    #[serde(default)]
    language: String,
    #[serde(default)]
    translation_songs: Vec<GeniusTranslationHit>,
}

#[derive(Debug, Deserialize, Default)]
struct GeniusTranslationHit {
    #[serde(default)]
    language: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    lyrics_state: String,
    #[serde(default)]
    hidden: bool,
}

/// Inner HTML of every `data-lyrics-container` block on a Genius song page.
/// Depth-counted so nested `<div>`s don't cut the block short.
fn genius_containers(html: &str) -> Vec<&str> {
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

/// Lowercase, unify quotes, and collapse whitespace so Genius fragments
/// (curly apostrophes, `"\n "` separators) match scraped lyric lines.
fn normalize_lyric(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '\u{2018}' | '\u{2019}' | '\u{201a}' | '\u{201b}' | '\u{2032}' | '\u{2035}' | '`'
            | '\u{b4}' => out.push('\''),
            '\u{201c}' | '\u{201d}' | '\u{201e}' | '\u{201f}' => out.push('"'),
            _ => out.extend(ch.to_lowercase()),
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
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
        translations: Vec::new(),
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
            translations: Vec::new(),
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
            translations: Vec::new(),
        };
        assert_eq!(lrc.active_index(5.0), None);
    }

    #[test]
    fn client_uses_lrclib_by_default() {
        assert_eq!(LyricsProvider::default(), LyricsProvider::LrcLib);
        assert!(LyricsProvider::all().contains(&LyricsProvider::LrcLib));
        assert!(LyricsProvider::all().contains(&LyricsProvider::Genius));
        assert!(!LyricsProvider::all().contains(&LyricsProvider::Custom));
        assert_eq!(LyricsProvider::all().len(), 4);
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

    fn genius_req(artist: &str, title: &str) -> LyricsRequest {
        LyricsRequest {
            artist: artist.into(),
            title: title.into(),
            album: String::new(),
            duration: 0,
        }
    }

    fn genius_hit(title: &str, artist: &str) -> GeniusSongHit {
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
    fn genius_hit_scoring_accepts_match() {
        let req = genius_req("Nirvana", "Smells Like Teen Spirit");
        let hit = genius_hit("Smells Like Teen Spirit", "Nirvana");
        assert!(score_genius_hit(&req, &hit).is_some());
    }

    #[test]
    fn genius_hit_scoring_accepts_featured_artist_overlap() {
        let req = genius_req("Nirvana", "Smells Like Teen Spirit (Official Video)");
        let hit = genius_hit("Smells Like Teen Spirit", "Nirvana");
        assert!(score_genius_hit(&req, &hit).is_some());
    }

    #[test]
    fn genius_hit_scoring_rejects_wrong_artist_and_title() {
        let req = genius_req("Nirvana", "Smells Like Teen Spirit");
        assert!(score_genius_hit(&req, &genius_hit("Smells Like Teen Spirit", "Weezer")).is_none());
        assert!(score_genius_hit(&req, &genius_hit("Come As You Are", "Nirvana")).is_none());
    }

    #[test]
    fn genius_hit_scoring_skips_instrumental_and_incomplete() {
        let req = genius_req("Nirvana", "Smells Like Teen Spirit");
        let mut hit = genius_hit("Smells Like Teen Spirit", "Nirvana");
        hit.instrumental = true;
        assert!(score_genius_hit(&req, &hit).is_none());
        hit.instrumental = false;
        hit.lyrics_state = "unreleased".into();
        assert!(score_genius_hit(&req, &hit).is_none());
    }

    #[test]
    fn genius_hit_deserializes_search_result() {
        let json = r#"{"result":{"_type":"song","id":52968,"title":"Smells Like Teen Spirit","artist_names":"Nirvana","url":"https://genius.com/Nirvana-smells-like-teen-spirit-lyrics","api_path":"/songs/52968","lyrics_state":"complete","instrumental":false}}"#;
        let hit: GeniusHit = serde_json::from_str(json).unwrap();
        let song = hit.result.unwrap();
        assert_eq!(song.kind, "song");
        assert_eq!(song.id, 52968);
        let req = genius_req("Nirvana", "Smells Like Teen Spirit");
        assert!(score_genius_hit(&req, &song).is_some());
    }

    #[test]
    fn genius_normalize_unifies_quotes_and_space() {
        assert_eq!(
            normalize_lyric("With the lights out, it’s  less\ndangerous"),
            "with the lights out, it's less dangerous"
        );
        assert_eq!(normalize_lyric("  HELLO   World "), "hello world");
    }

    #[test]
    fn genius_decode_entities_handles_named_and_numeric() {
        assert_eq!(
            decode_entities("it&#x27;s &amp; &quot;us&quot; &#39;ok&#39;"),
            "it's & \"us\" 'ok'"
        );
        assert_eq!(decode_entities("a &unknown; b & c"), "a &unknown; b & c");
    }

    #[test]
    fn genius_containers_extract_nested_blocks() {
        let html = concat!(
            r#"<div data-lyrics-container="true" class="x">First<br/>Second<span>nested<div>deep</div></span></div>"#,
            r#"<div data-lyrics-container="true" class="x"><br/>[Chorus]<br/>Line</div>"#,
        );
        let blocks = genius_containers(html);
        assert_eq!(blocks.len(), 2);
        let lines = html_to_lines(blocks[0]);
        assert_eq!(lines, vec!["First", "Secondnesteddeep"]);
        assert_eq!(html_to_lines(blocks[1]), vec!["[Chorus]", "Line"]);
    }

    #[test]
    fn genius_html_to_lines_strips_annotated_links() {
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
    fn language_names_cover_genius_codes() {
        assert_eq!(language_name("ru"), "Russian");
        assert_eq!(language_name("iw"), "Hebrew");
        assert_eq!(language_name("he"), "Hebrew");
        assert_eq!(language_name("pt"), "Portuguese");
        assert_eq!(language_name("xx"), "XX");
    }

    #[test]
    fn translated_lyrics_loading_flag_follows_source() {
        let unloaded = TranslatedLyrics::unloaded("ru", "https://genius.com/x");
        assert!(!unloaded.is_loaded());
        assert!(unloaded.lyrics.lines.is_empty());
        let loaded = TranslatedLyrics {
            language: "ru".into(),
            lyrics: Lyrics::from_custom_text("la").unwrap(),
            source: None,
        };
        assert!(loaded.is_loaded());
    }

    #[test]
    fn lyrics_with_translations_serde_round_trip() {
        let mut lyrics = Lyrics::from_custom_text("la").unwrap();
        lyrics
            .translations
            .push(TranslatedLyrics::unloaded("ru", "https://genius.com/x"));
        let json = serde_json::to_string(&lyrics).unwrap();
        let back: Lyrics = serde_json::from_str(&json).unwrap();
        assert_eq!(back, lyrics);
        assert!(!back.translations[0].is_loaded());
    }

    #[test]
    fn lyrics_without_translations_field_still_parse() {
        let json = r#"{"lines":[],"plain":"la","provider":"lrclib"}"#;
        let lyrics: Lyrics = serde_json::from_str(json).unwrap();
        assert!(lyrics.translations.is_empty());
    }

    #[test]
    fn genius_song_deserializes_translation_songs() {
        let json = r#"{"response":{"song":{"language":"en","description":{"plain":"blurb"},"translation_songs":[{"language":"ru","url":"https://genius.com/x-lyrics","lyrics_state":"complete","hidden":false},{"language":"","url":"https://genius.com/y","lyrics_state":"complete","hidden":false}]}}}"#;
        let resp: GeniusSongResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.response.song.translation_songs.len(), 2);
        assert_eq!(resp.response.song.translation_songs[0].language, "ru");
    }

    #[test]
    fn genius_annotation_key_matches_despite_quote_style() {
        let fragment = "With the lights out, it’s less dangerous";
        let line = "With the lights out, it's less dangerous";
        assert_eq!(normalize_lyric(fragment), normalize_lyric(line));
        let multi = "Hello, hello, hello, how low\n Hello, hello, hello";
        let keys: Vec<String> = multi.lines().map(normalize_lyric).collect();
        assert_eq!(
            keys,
            vec!["hello, hello, hello, how low", "hello, hello, hello"]
        );
    }
}
