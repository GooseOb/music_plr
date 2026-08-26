use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use super::{JsonStore, StoreLocation};
use crate::types::Track;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct DownloadRegistry {
    tracks: HashMap<String, Track>,
}

impl JsonStore for DownloadRegistry {
    const FILE: &'static str = "downloads.json";
    const LOCATION: StoreLocation = StoreLocation::Data;
}

impl DownloadRegistry {
    pub fn register(&mut self, track: Track) {
        self.tracks.insert(track.cache_key(), track);
        self.save();
    }

    pub fn remove(&mut self, key: &str) -> Option<Track> {
        let result = self.tracks.remove(key);
        self.save();
        result
    }

    pub fn contains(&self, key: &str) -> bool {
        self.tracks.contains_key(key)
    }

    /// Returns the on-disk path of the downloaded audio file for `key`, if the
    /// track is registered and was downloaded to a known location.
    pub fn path_for(&self, key: &str) -> Option<String> {
        self.tracks.get(key).and_then(|t| t.download_path.clone())
    }

    pub fn clone_tracks(&self) -> Vec<Track> {
        self.tracks.values().cloned().collect()
    }
}
