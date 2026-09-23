//! On-disk lyrics cache keyed by provider-namespaced track key
//! (`slug:id`, matching the stream cache index).

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
/// Version 2 also switched keys from bare song ids to provider-namespaced
/// `slug:id` keys (matching the stream cache index) so ids from different
/// providers can no longer collide.
const LYRICS_CACHE_VERSION: u32 = 2;

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
            self.namespace_bare_custom_keys();
            self.version = LYRICS_CACHE_VERSION;
            self.save();
        }
    }

    /// One-time rewrite of pre-v2 bare-id custom keys to provider-namespaced
    /// keys. The old keys stored no provider, so they are assumed to be
    /// `YouTube` (the default provider and source of nearly all tracks);
    /// keys already carrying a known slug are left untouched.
    fn namespace_bare_custom_keys(&mut self) {
        use crate::providers::ProviderId;
        let mut remapped: HashMap<String, Vec<CustomLyricsEntry>> =
            HashMap::with_capacity(self.custom.len());
        for (old_key, entries) in self.custom.drain() {
            let namespaced = match old_key.split_once(':') {
                Some((prefix, _)) if ProviderId::from_slug(prefix).is_some() => old_key,
                _ => ProviderId::YouTube.cache_key(&old_key),
            };
            remapped.entry(namespaced).or_default().extend(entries);
        }
        self.custom = remapped;
    }
}

impl LyricsCache {
    /// Look up the cached lyrics for a specific provider, if present.
    /// `track_key` is the provider-namespaced `slug:id` key (see
    /// [`key_for`]); pre-v2 bare-id entries are not consulted for fetched
    /// lyrics since the v2 migration drops them.
    pub fn get_for(&self, track_key: &str, provider: LyricsProvider) -> Option<Lyrics> {
        self.entries
            .get(track_key)
            .and_then(|list| list.iter().find(|e| e.provider == provider))
            .map(CachedLyrics::to_lyrics)
    }

    /// Store lyrics for a track key, upserting the per-provider entry (the
    /// same provider's prior entry is replaced; other providers are
    /// preserved). `track_key` must be the provider-namespaced `slug:id` key.
    pub fn insert(&mut self, track_key: &str, lyrics: &Lyrics) {
        let list = self.entries.entry(track_key.to_string()).or_default();
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
    pub fn custom_names(&self, track_key: &str) -> Vec<String> {
        self.custom
            .get(track_key)
            .map(|list| list.iter().map(|e| e.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Look up one named custom entry for a track, if present.
    pub fn get_custom(&self, track_key: &str, name: &str) -> Option<Lyrics> {
        self.custom
            .get(track_key)?
            .iter()
            .find(|e| e.name == name)
            .map(CustomLyricsEntry::to_lyrics)
    }

    /// Store a named custom entry, upserting on the name.
    pub fn insert_custom(&mut self, track_key: &str, name: &str, lyrics: &Lyrics) {
        self.insert_custom_inner(track_key, name, lyrics);
        self.save();
    }

    fn insert_custom_inner(&mut self, track_key: &str, name: &str, lyrics: &Lyrics) {
        let list = self.custom.entry(track_key.to_string()).or_default();
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
    pub fn remove_custom(&mut self, track_key: &str, name: &str) -> bool {
        let removed = self.custom.get_mut(track_key).is_some_and(|list| {
            let before = list.len();
            list.retain(|e| e.name != name);
            before != list.len()
        });
        if removed {
            if self.custom.get(track_key).is_some_and(Vec::is_empty) {
                self.custom.remove(track_key);
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
        use crate::providers::ProviderId;
        let mut cache = LyricsCache::default();
        cache.insert("t1", &custom_lyrics("la"));
        cache.insert_custom("t1", "Mine", &custom_lyrics("mine"));
        assert!(cache.get_for("t1", LyricsProvider::Custom).is_some());
        cache.version = 0;
        cache.migrate();
        assert!(cache.get_for("t1", LyricsProvider::Custom).is_none());
        let namespaced = ProviderId::YouTube.cache_key("t1");
        assert_eq!(cache.custom_names(&namespaced), vec!["Mine"]);
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

    #[test]
    fn keys_are_provider_namespaced() {
        use crate::providers::ProviderId;
        assert_eq!(ProviderId::YouTube.cache_key("abc"), "youtube:abc");
        assert_eq!(ProviderId::SoundCloud.cache_key("abc"), "soundcloud:abc");
        assert_ne!(
            ProviderId::YouTube.cache_key("abc"),
            ProviderId::SoundCloud.cache_key("abc")
        );
    }

    #[test]
    fn same_id_on_different_providers_does_not_collide() {
        use crate::providers::ProviderId;
        let mut cache = LyricsCache::default();
        let yt_key = ProviderId::YouTube.cache_key("abc");
        let sc_key = ProviderId::SoundCloud.cache_key("abc");
        cache.insert_custom(&yt_key, "Mine", &custom_lyrics("yt version"));
        assert_eq!(cache.custom_names(&sc_key), Vec::<String>::new());
        assert!(cache.get_custom(&sc_key, "Mine").is_none());
        assert_eq!(
            cache.get_custom(&yt_key, "Mine").unwrap().plain,
            "yt version"
        );
    }

    #[test]
    fn migration_namespaces_bare_custom_keys_as_youtube() {
        use crate::providers::ProviderId;
        let mut cache = LyricsCache::default();
        cache.insert_custom_inner("abc", "Mine", &custom_lyrics("legacy"));
        cache.version = 0;
        cache.migrate();
        let namespaced = ProviderId::YouTube.cache_key("abc");
        assert_eq!(cache.custom_names(&namespaced), vec!["Mine"]);
        assert_eq!(
            cache.get_custom(&namespaced, "Mine").unwrap().plain,
            "legacy"
        );
        assert!(!cache.custom.contains_key("abc"));
    }

    #[test]
    fn migration_keeps_colons_in_bare_ids() {
        use crate::providers::ProviderId;
        let mut cache = LyricsCache::default();
        cache.insert_custom_inner("1:2:a", "Mine", &custom_lyrics("legacy"));
        cache.version = 0;
        cache.migrate();
        let namespaced = ProviderId::YouTube.cache_key("1:2:a");
        assert_eq!(cache.custom_names(&namespaced), vec!["Mine"]);
    }

    #[test]
    fn migration_leaves_namespaced_custom_keys_untouched() {
        use crate::providers::ProviderId;
        let mut cache = LyricsCache::default();
        let sc_key = ProviderId::SoundCloud.cache_key("abc");
        cache.insert_custom(&sc_key, "Mine", &custom_lyrics("sc version"));
        cache.version = 0;
        cache.migrate();
        assert_eq!(cache.custom_names(&sc_key), vec!["Mine"]);
        assert_eq!(
            cache.get_custom(&sc_key, "Mine").unwrap().plain,
            "sc version"
        );
        assert!(cache
            .custom_names(&ProviderId::YouTube.cache_key("abc"))
            .is_empty());
    }
}
