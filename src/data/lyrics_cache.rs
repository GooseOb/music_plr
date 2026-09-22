//! On-disk lyrics cache keyed by track id.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{JsonStore, StoreLocation};
use crate::lyrics::{LyricLine, Lyrics, LyricsProvider, TranslatedLyrics};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CachedLyrics {
    pub plain: String,
    pub lines: Vec<LyricLine>,
    pub provider: LyricsProvider,
    #[serde(default)]
    pub translations: Vec<TranslatedLyrics>,
}

impl CachedLyrics {
    pub fn to_lyrics(&self) -> Lyrics {
        Lyrics {
            lines: self.lines.clone(),
            plain: self.plain.clone(),
            provider: self.provider,
            translations: self.translations.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CustomLyricsEntry {
    pub name: String,
    pub plain: String,
    pub lines: Vec<LyricLine>,
}

impl CustomLyricsEntry {
    pub fn to_lyrics(&self) -> Lyrics {
        Lyrics {
            lines: self.lines.clone(),
            plain: self.plain.clone(),
            provider: LyricsProvider::Custom,
            translations: Vec::new(),
        }
    }
}

/// Bumped whenever fetched entries may be stale or malformed so one load
/// drops them; user-authored `custom` entries are irreplaceable and survive.
const LYRICS_CACHE_VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LyricsCache {
    #[serde(default)]
    version: u32,
    entries: HashMap<String, Vec<CachedLyrics>>,
    #[serde(default)]
    custom: HashMap<String, Vec<CustomLyricsEntry>>,
}

impl JsonStore for LyricsCache {
    const FILE: &'static str = "lyrics_cache.json";
    const LOCATION: StoreLocation = StoreLocation::Cache;
}

impl LyricsCache {
    /// Load the cache, once dropping pre-`version` fetched entries (e.g.
    /// Genius lyrics scraped truncated before the embedded-state fix) while
    /// keeping user custom lyrics.
    pub fn load_migrated() -> Self {
        let mut cache = Self::load();
        cache.migrate();
        cache
    }

    fn migrate(&mut self) {
        if self.version < LYRICS_CACHE_VERSION {
            self.entries.clear();
            self.version = LYRICS_CACHE_VERSION;
            self.save();
        }
    }
}

impl LyricsCache {
    /// Look up the cached lyrics for a specific provider, if present.
    pub fn get_for(&self, track_id: &str, provider: LyricsProvider) -> Option<Lyrics> {
        self.entries
            .get(track_id)
            .and_then(|list| list.iter().find(|e| e.provider == provider))
            .map(CachedLyrics::to_lyrics)
    }

    /// Store lyrics for a track id, upserting the per-provider entry (the same
    /// provider's prior entry is replaced; other providers are preserved).
    pub fn insert(&mut self, track_id: &str, lyrics: &Lyrics) {
        let list = self.entries.entry(track_id.to_string()).or_default();
        if let Some(slot) = list.iter_mut().find(|e| e.provider == lyrics.provider) {
            slot.plain.clone_from(&lyrics.plain);
            slot.lines.clone_from(&lyrics.lines);
            slot.translations.clone_from(&lyrics.translations);
        } else {
            list.push(CachedLyrics {
                plain: lyrics.plain.clone(),
                lines: lyrics.lines.clone(),
                provider: lyrics.provider,
                translations: lyrics.translations.clone(),
            });
        }
        self.save();
    }

    /// Names of the user-added custom lyrics for a track, in creation order.
    pub fn custom_names(&self, track_id: &str) -> Vec<String> {
        self.custom
            .get(track_id)
            .map(|list| list.iter().map(|e| e.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Look up one named custom entry for a track, if present.
    pub fn get_custom(&self, track_id: &str, name: &str) -> Option<Lyrics> {
        self.custom
            .get(track_id)?
            .iter()
            .find(|e| e.name == name)
            .map(CustomLyricsEntry::to_lyrics)
    }

    /// Store a named custom entry, upserting on the name.
    pub fn insert_custom(&mut self, track_id: &str, name: &str, lyrics: &Lyrics) {
        self.insert_custom_inner(track_id, name, lyrics);
        self.save();
    }

    fn insert_custom_inner(&mut self, track_id: &str, name: &str, lyrics: &Lyrics) {
        let list = self.custom.entry(track_id.to_string()).or_default();
        if let Some(slot) = list.iter_mut().find(|e| e.name == name) {
            slot.plain.clone_from(&lyrics.plain);
            slot.lines.clone_from(&lyrics.lines);
        } else {
            list.push(CustomLyricsEntry {
                name: name.to_string(),
                plain: lyrics.plain.clone(),
                lines: lyrics.lines.clone(),
            });
        }
    }

    /// Delete one named custom entry; true when something was removed.
    pub fn remove_custom(&mut self, track_id: &str, name: &str) -> bool {
        let removed = self.custom.get_mut(track_id).is_some_and(|list| {
            let before = list.len();
            list.retain(|e| e.name != name);
            before != list.len()
        });
        if removed {
            if self.custom.get(track_id).is_some_and(Vec::is_empty) {
                self.custom.remove(track_id);
            }
            self.save();
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn custom_lyrics(text: &str) -> Lyrics {
        Lyrics::from_custom_text(text).unwrap()
    }

    #[test]
    fn migration_drops_fetched_entries_but_keeps_custom() {
        let mut cache = LyricsCache::default();
        cache.insert("t1", &custom_lyrics("la"));
        cache.insert_custom("t1", "Mine", &custom_lyrics("mine"));
        assert!(cache.get_for("t1", LyricsProvider::Custom).is_some());
        cache.version = 0;
        cache.migrate();
        assert!(cache.get_for("t1", LyricsProvider::Custom).is_none());
        assert_eq!(cache.custom_names("t1"), vec!["Mine"]);
        assert_eq!(cache.version, LYRICS_CACHE_VERSION);
    }

    #[test]
    fn migration_is_noop_when_current() {
        let mut cache = LyricsCache::default();
        cache.insert("t1", &custom_lyrics("la"));
        cache.version = LYRICS_CACHE_VERSION;
        cache.migrate();
        assert!(cache.get_for("t1", LyricsProvider::Custom).is_some());
    }

    #[test]
    fn legacy_cache_without_version_parses() {
        let json = r#"{"entries":{},"custom":{}}"#;
        let cache: LyricsCache = serde_json::from_str(json).unwrap();
        assert_eq!(cache.version, 0);
    }

    #[test]
    fn custom_round_trip_per_name() {
        let mut cache = LyricsCache::default();
        cache.insert_custom("t1", "Studio", &custom_lyrics("la la"));
        cache.insert_custom("t1", "Live", &custom_lyrics("[00:01.00]la"));
        assert_eq!(cache.custom_names("t1"), vec!["Studio", "Live"]);
        let live = cache.get_custom("t1", "Live").unwrap();
        assert_eq!(live.timed_count(), 1);
        assert_eq!(live.provider, LyricsProvider::Custom);
        assert!(cache.get_custom("t1", "Missing").is_none());
        assert!(cache.custom_names("other").is_empty());
    }

    #[test]
    fn custom_round_trip_preserves_notes() {
        let mut cache = LyricsCache::default();
        cache.insert_custom(
            "t1",
            "Annotated",
            &custom_lyrics("[00:01.00]la\n# first\n# second\nplain"),
        );
        let got = cache.get_custom("t1", "Annotated").unwrap();
        assert_eq!(got.lines[0].description, "first\nsecond");
        assert_eq!(got.plain, "la\nplain");
    }

    #[test]
    fn custom_upsert_replaces_same_name() {
        let mut cache = LyricsCache::default();
        cache.insert_custom("t1", "Mine", &custom_lyrics("v1"));
        cache.insert_custom("t1", "Mine", &custom_lyrics("v2"));
        assert_eq!(cache.custom_names("t1"), vec!["Mine"]);
        assert_eq!(cache.get_custom("t1", "Mine").unwrap().plain, "v2");
    }

    #[test]
    fn custom_remove_cleans_up() {
        let mut cache = LyricsCache::default();
        cache.insert_custom("t1", "A", &custom_lyrics("a"));
        cache.insert_custom("t1", "B", &custom_lyrics("b"));
        assert!(cache.remove_custom("t1", "A"));
        assert_eq!(cache.custom_names("t1"), vec!["B"]);
        assert!(!cache.remove_custom("t1", "A"));
        assert!(cache.remove_custom("t1", "B"));
        assert!(cache.custom_names("t1").is_empty());
    }
}
