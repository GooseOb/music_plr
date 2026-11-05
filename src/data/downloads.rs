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
        let mut remapped = HashMap::new();
        let mut remapped_times = HashMap::new();
        let mut migrated = false;
        for (old_key, track) in store.tracks.drain() {
            let new_key = track.cache_key();
            let registered = store.registered_at.remove(&old_key).unwrap_or(0);
            migrated |= new_key != old_key;
            remapped.entry(new_key.clone()).or_insert(track);
            remapped_times.entry(new_key).or_insert(registered);
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

    /// Every registered track, in no particular order, for callers that only
    /// need the set (thumbnail seeding).
    pub fn iter(&self) -> impl Iterator<Item = &Track> {
        self.tracks.values()
    }

    /// Every registered track, newest download first. The home rail's row
    /// indices are built from this order, so drawing and playing agree; the
    /// Downloads view keeps it too, so both list the same thing the same way.
    pub fn tracks(&self) -> Vec<&Track> {
        let mut entries: Vec<(&String, &Track)> = self.tracks.iter().collect();
        entries.sort_by(|(a_key, _), (b_key, _)| {
            self.downloaded_at(b_key)
                .cmp(&self.downloaded_at(a_key))
                .then_with(|| a_key.cmp(b_key))
        });
        entries.into_iter().map(|(_, track)| track).collect()
    }

    pub fn clone_tracks(&self) -> Vec<Track> {
        self.tracks().into_iter().cloned().collect()
    }

    fn downloaded_at(&self, key: &str) -> u64 {
        self.registered_at.get(key).copied().unwrap_or(0)
    }
}
