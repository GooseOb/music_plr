//! Lyrics fetching, backed by a handful of free, no-API-key providers.

mod genius;
mod http;
mod lrc;
mod lrclib;
mod lyrics_ovh;

use lrc::{format_timestamp, parse_lrc_line};

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

    pub fn fetch(self, req: &LyricsRequest) -> anyhow::Result<Option<Lyrics>> {
        match self {
            LyricsProvider::LrcLib => lrclib::fetch_lrclib(req),
            LyricsProvider::LrcMux => lrclib::fetch_lrcmux(req),
            LyricsProvider::LyricsOvh => lyrics_ovh::fetch(req),
            LyricsProvider::Genius => genius::fetch(req),
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
    ) -> anyhow::Result<Option<TranslatedLyrics>> {
        match self {
            LyricsProvider::Genius => genius::fetch_translation(language, source),
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
        if self.lines.is_empty() {
            return self.plain.clone();
        }
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

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn edit_text_falls_back_to_plain_when_lines_are_missing() {
        let lyrics = Lyrics {
            lines: Vec::new(),
            plain: "first line\nsecond line".into(),
            provider: LyricsProvider::LrcLib,
            translations: Vec::new(),
        };
        assert!(!lyrics.has_timed());
        assert_eq!(lyrics.to_edit_text(), "first line\nsecond line");
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
}
