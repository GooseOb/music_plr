use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{JsonStore, StoreLocation};
use crate::types::Track;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TrashStore {
    pub tracks: Vec<Track>,
}

impl JsonStore for TrashStore {
    const FILE: &'static str = "trashbin.json";
    const LOCATION: StoreLocation = StoreLocation::Data;
}

impl TrashStore {
    pub fn contains(&self, track: &Track) -> bool {
        let key = track.cache_key();
        self.tracks.iter().any(|t| t.cache_key() == key)
    }

    pub fn len(&self) -> usize {
        self.tracks.len()
    }

    pub fn add_tracks<'a, I>(&mut self, tracks: I) -> usize
    where
        I: IntoIterator<Item = &'a Track>,
    {
        let mut seen: HashSet<String> = self.tracks.iter().map(Track::cache_key).collect();
        let batch: Vec<Track> = tracks
            .into_iter()
            .filter_map(|track| {
                let key = track.cache_key();
                if seen.contains(&key) {
                    return None;
                }
                seen.insert(key);
                Some(track.clone())
            })
            .collect();
        let inserted = batch.len();
        if inserted > 0 {
            self.tracks.splice(0..0, batch);
            self.save();
        }
        inserted
    }

    /// Remove every track whose cache key is in `keys`. Key-based (rather
    /// than positional) so removal works from any view showing the track,
    /// not just the trashbin view itself.
    pub fn remove_keys(&mut self, keys: &std::collections::HashSet<String>) -> usize {
        if keys.is_empty() {
            return 0;
        }
        let before = self.tracks.len();
        self.tracks.retain(|t| !keys.contains(&t.cache_key()));
        let removed = before - self.tracks.len();
        if removed > 0 {
            self.save();
        }
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{providers::ProviderId, types::ProviderTrack};

    fn mk(id: &str) -> Track {
        let mut providers = std::collections::HashMap::new();
        providers.insert(
            ProviderId::YouTube,
            ProviderTrack {
                id: id.to_string(),
                url: id.to_string(),
                artist_id: None,
                duration: 0,
                thumbnail: String::new(),
                album: None,
                play_count: 0,
            },
        );
        Track {
            title: id.to_string(),
            artist: String::new(),
            source: ProviderId::YouTube,
            providers,
        }
    }

    #[test]
    fn add_dedups_by_cache_key() {
        let mut store = TrashStore::default();
        assert_eq!(store.add_tracks([mk("a")].iter()), 1);
        assert_eq!(store.add_tracks([mk("a")].iter()), 0);
        assert_eq!(store.len(), 1);
        assert!(store.contains(&mk("a")));
        assert!(!store.contains(&mk("b")));
    }

    #[test]
    fn remove_keys_drops_matching_tracks() {
        let mut store = TrashStore::default();
        store.add_tracks([mk("a"), mk("b"), mk("c")].iter());
        let keys: std::collections::HashSet<String> = ["youtube:a", "youtube:c"]
            .iter()
            .map(ToString::to_string)
            .collect();
        assert_eq!(store.remove_keys(&keys), 2);
        assert_eq!(store.len(), 1);
        assert!(store.contains(&mk("b")));
        assert_eq!(store.remove_keys(&std::collections::HashSet::new()), 0);
    }
}
