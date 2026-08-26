//! On-disk lyrics cache keyed by track id.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{JsonStore, StoreLocation};
use crate::lyrics::{Lyrics, LyricsProvider};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CachedLyrics {
    pub plain: String,
    pub timed: Vec<(f32, String)>,
    pub provider: LyricsProvider,
}

impl CachedLyrics {
    pub fn to_lyrics(&self) -> Lyrics {
        Lyrics {
            timed: self.timed.clone(),
            plain: self.plain.clone(),
            provider: self.provider,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LyricsCache {
    entries: HashMap<String, Vec<CachedLyrics>>,
}

impl JsonStore for LyricsCache {
    const FILE: &'static str = "lyrics_cache.json";
    const LOCATION: StoreLocation = StoreLocation::Cache;
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
            slot.timed.clone_from(&lyrics.timed);
        } else {
            list.push(CachedLyrics {
                plain: lyrics.plain.clone(),
                timed: lyrics.timed.clone(),
                provider: lyrics.provider,
            });
        }
        self.save();
    }
}
