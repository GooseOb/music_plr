use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{JsonStore, StoreLocation};
use crate::types::Track;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DownloadRegistry {
    tracks: HashMap<String, Track>,
    /// When each entry was registered, so the newest-first orders (the home
    /// rail, the palette) have something to sort by. Entries from before this
    /// field existed sort last.
    #[serde(default)]
    registered_at: HashMap<String, u64>,
}

impl JsonStore for DownloadRegistry {
    const FILE: &'static str = "downloads.json";
    const LOCATION: StoreLocation = StoreLocation::Data;
}

impl DownloadRegistry {
    /// Load the registry, rewriting pre-slug `Debug:id` keys (e.g.
    /// `YouTube:…`, `LastFm:…`) to the `slug:id` format used by
    /// [`Track::cache_key`] so old entries stay reachable.
    pub fn load_migrated() -> Self {
        let mut store = Self::load();
        let mut remapped = std::collections::HashMap::new();
        let mut remapped_times = std::collections::HashMap::new();
        let mut migrated = false;
        for (old_key, track) in store.tracks.drain() {
            let new_key = track.cache_key();
            let registered = store.registered_at.remove(&old_key).unwrap_or(0);
            migrated |= new_key != old_key;
            if remapped.insert(new_key.clone(), track).is_none() {
                remapped_times.insert(new_key, registered);
            }
        }
        store.tracks = remapped;
        store.registered_at = remapped_times;
        if migrated {
            store.save();
        }
        store
    }

    pub fn register(&mut self, track: Track) {
        let key = track.cache_key();
        self.tracks.insert(key.clone(), track);
        self.registered_at
            .insert(key, crate::data::home_cache::now_secs());
        self.save();
    }

    pub fn remove(&mut self, key: &str) -> Option<Track> {
        let result = self.tracks.remove(key);
        self.registered_at.remove(key);
        self.save();
        result
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn contains(&self, key: &str) -> bool {
        self.tracks.contains_key(key)
    }

    /// Returns the on-disk path of the downloaded audio file for `key`, if the
    /// track is registered and was downloaded to a known location.
    pub fn path_for(&self, key: &str) -> Option<String> {
        self.tracks.get(key).and_then(Track::local_path)
    }

    pub fn clone_tracks(&self) -> Vec<Track> {
        self.tracks.values().cloned().collect()
    }

    /// Every registered track, newest download first. The home rail's row
    /// indices are built from this order, so drawing and playing agree.
    pub fn tracks(&self) -> Vec<&Track> {
        let mut tracks: Vec<&Track> = self.tracks.values().collect();
        tracks.sort_by(|a, b| {
            let at = |track: &Track| {
                self.registered_at
                    .get(&track.cache_key())
                    .copied()
                    .unwrap_or(0)
            };
            at(b)
                .cmp(&at(a))
                .then_with(|| a.cache_key().cmp(&b.cache_key()))
        });
        tracks
    }
}
