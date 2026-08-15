use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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
        self.tracks.insert(track.url.clone(), track);
        self.save();
    }

    pub fn remove(&mut self, url: &str) -> Option<Track> {
        let result = self.tracks.remove(url);
        self.save();
        result
    }

    pub fn contains(&self, url: &str) -> bool {
        self.tracks.contains_key(url)
    }

    /// Returns the on-disk path of the downloaded audio file for `url`, if the
    /// track is registered and was downloaded to a known location.
    pub fn path_for(&self, url: &str) -> Option<String> {
        self.tracks.get(url).and_then(|t| t.download_path.clone())
    }

    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn clone_tracks(&self) -> Vec<Track> {
        self.tracks.values().cloned().collect()
    }
}
