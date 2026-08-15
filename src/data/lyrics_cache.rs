//! On-disk lyrics cache keyed by track id.

use crate::lyrics::Lyrics;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

use super::{JsonStore, StoreLocation};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CachedLyrics {
    pub plain: String,
    pub timed: Vec<(f32, String)>,
    pub provider: crate::lyrics::LyricsProvider,
}

impl CachedLyrics {
    pub fn to_lyrics(&self) -> Lyrics {
        Lyrics {
            synced: !self.timed.is_empty(),
            timed: self.timed.clone(),
            plain: self.plain.clone(),
            provider: self.provider,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct LyricsCache {
    entries: HashMap<String, CachedLyrics>,
}

impl JsonStore for LyricsCache {
    const FILE: &'static str = "lyrics_cache.json";
    const LOCATION: StoreLocation = StoreLocation::Cache;
}

impl LyricsCache {
    /// Look up cached lyrics for a track id.
    pub fn get(&self, track_id: &str) -> Option<Lyrics> {
        self.entries.get(track_id).map(CachedLyrics::to_lyrics)
    }

    /// Store lyrics for a track id (overwriting any prior entry).
    pub fn insert(&mut self, track_id: &str, lyrics: &Lyrics) {
        self.entries.insert(
            track_id.to_string(),
            CachedLyrics {
                plain: lyrics.plain.clone(),
                timed: lyrics.timed.clone(),
                provider: lyrics.provider,
            },
        );
        self.save();
    }
}
