use super::{JsonStore, StoreLocation};
use crate::types::Track;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub name: String,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlaylistStore {
    pub playlists: Vec<Playlist>,
}

impl JsonStore for PlaylistStore {
    const FILE: &'static str = "playlists.json";
    const LOCATION: StoreLocation = StoreLocation::Data;
}

impl PlaylistStore {
    pub fn create(&mut self, name: &str) {
        if !name.trim().is_empty() && !self.playlists.iter().any(|p| p.name == name.trim()) {
            self.playlists.push(Playlist {
                name: name.trim().to_string(),
                tracks: Vec::new(),
            });
            self.save();
        }
    }

    /// Produce a playlist name unique within the store by appending
    /// " (n)" when `base` is already taken.
    fn unique_name(&self, base: &str) -> String {
        let base = base.trim();
        let base = if base.is_empty() { "Playlist" } else { base };
        if !self.playlists.iter().any(|p| p.name == base) {
            return base.to_string();
        }
        let mut n = 2;
        loop {
            let candidate = format!("{base} ({n})");
            if !self.playlists.iter().any(|p| p.name == candidate) {
                return candidate;
            }
            n += 1;
        }
    }

    /// Create a playlist with a unique name at `pos` (clamped), returning its
    /// final index. Unlike [`Self::create`], this always creates (renaming on
    /// collision) so a dragged card reliably becomes a new playlist.
    pub fn create_at(&mut self, name: &str, pos: usize) -> usize {
        let name = self.unique_name(name);
        let pos = pos.min(self.playlists.len());
        self.playlists.insert(
            pos,
            Playlist {
                name: name.clone(),
                tracks: Vec::new(),
            },
        );
        self.save();
        pos
    }

    pub fn delete(&mut self, index: usize) {
        if index < self.playlists.len() {
            self.playlists.remove(index);
            self.save();
        }
    }

    /// Insert multiple tracks at once, writing to disk only once.
    /// `pos` is clamped per-track to the growing list length.
    /// Returns the number of tracks actually inserted.
    pub fn insert_tracks_at<'a, I>(&mut self, playlist_idx: usize, tracks: I, pos: usize) -> usize
    where
        I: IntoIterator<Item = &'a Track>,
    {
        let Some(pl) = self.playlists.get_mut(playlist_idx) else {
            return 0;
        };
        let mut seen: HashSet<String> = pl
            .tracks
            .iter()
            .map(super::super::types::Track::cache_key)
            .collect();
        let insert_pos = pos.min(pl.tracks.len());
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
        let inserted_count = batch.len();
        if inserted_count > 0 {
            pl.tracks.splice(insert_pos..insert_pos, batch);
            self.save();
        }
        inserted_count
    }

    /// Remove tracks at the given indices (in any order), writing to disk
    /// only once. Indices that are out of bounds are silently skipped.
    /// Returns the number of tracks actually removed.
    pub fn remove_tracks_at(&mut self, playlist_idx: usize, indices: &[usize]) -> usize {
        let Some(pl) = self.playlists.get_mut(playlist_idx) else {
            return 0;
        };
        let removed = crate::util::remove_at(&mut pl.tracks, indices);
        self.save();
        removed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::providers::ProviderId;
    use crate::types::ProviderTrack;

    /// Build a YouTube-source test track from an id (url mirrors id).
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
            download_path: None,
            source: ProviderId::YouTube,
            providers,
        }
    }

    fn make_store(tracks: &[&str]) -> PlaylistStore {
        let playlist = Playlist {
            name: "Test".to_string(),
            tracks: tracks
                .iter()
                .map(|s| {
                    let mut providers = std::collections::HashMap::new();
                    providers.insert(
                        crate::providers::ProviderId::YouTube,
                        crate::types::ProviderTrack {
                            id: s.to_string(),
                            url: s.to_string(),
                            artist_id: None,
                            duration: 0,
                            thumbnail: String::new(),
                            album: None,
                            play_count: 0,
                        },
                    );
                    Track {
                        title: s.to_string(),
                        artist: String::new(),
                        download_path: None,
                        source: crate::providers::ProviderId::YouTube,
                        providers,
                    }
                })
                .collect(),
        };
        PlaylistStore {
            playlists: vec![playlist],
        }
    }

    #[test]
    fn remove_nonexistent_playlist() {
        let mut store = make_store(&["a"]);
        store.delete(99);
        assert_eq!(store.playlists.len(), 1);
    }

    #[test]
    fn create_duplicates_ignored() {
        let mut store = make_store(&[]);
        store.create("Test");
        store.create("Test");
        assert_eq!(store.playlists.len(), 1);
    }

    #[test]
    fn insert_track_at_top() {
        let mut store = make_store(&["a", "b", "c"]);
        let new_track = mk("new");
        store.insert_tracks_at(0, std::iter::once(&new_track), 0);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.provider_id(ProviderId::YouTube).unwrap_or(""))
                .collect::<Vec<_>>(),
            vec!["new", "a", "b", "c"]
        );
    }

    #[test]
    fn insert_track_at_position() {
        let mut store = make_store(&["a", "b", "c"]);
        let new_track = mk("new");
        store.insert_tracks_at(0, std::iter::once(&new_track), 2);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.provider_id(ProviderId::YouTube).unwrap_or(""))
                .collect::<Vec<_>>(),
            vec!["a", "b", "new", "c"]
        );
    }

    #[test]
    fn insert_track_at_clamps_position() {
        let mut store = make_store(&["a", "b", "c"]);
        let new_track = mk("new");
        store.insert_tracks_at(0, std::iter::once(&new_track), 100);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.provider_id(ProviderId::YouTube).unwrap_or(""))
                .collect::<Vec<_>>(),
            vec!["a", "b", "c", "new"]
        );
    }

    #[test]
    fn insert_track_at_dedup_ignored() {
        let mut store = make_store(&["a", "b", "c"]);
        let dup_track = mk("a");
        store.insert_tracks_at(0, std::iter::once(&dup_track), 0);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.provider_id(ProviderId::YouTube).unwrap_or(""))
                .collect::<Vec<_>>(),
            vec!["a", "b", "c"]
        );
    }

    #[test]
    fn remove_tracks_at_nonexistent_playlist() {
        let mut store = make_store(&["a"]);
        store.remove_tracks_at(99, &[0]);
        assert_eq!(store.playlists[0].tracks.len(), 1);
    }

    #[test]
    fn create_at_inserts_at_position_with_unique_name() {
        let mut store = PlaylistStore::default();
        store.create("Mix");
        // Duplicate base name is de-duplicated; inserted at the front.
        let idx = store.create_at("Mix", 0);
        assert_eq!(store.playlists.len(), 2);
        assert_eq!(store.playlists[idx].name, "Mix (2)");
        assert_eq!(idx, 0);
        // A third "Mix" appended at the end gets the next suffix.
        let idx2 = store.create_at("Mix", store.playlists.len());
        assert_eq!(store.playlists[idx2].name, "Mix (3)");
        assert_eq!(idx2, store.playlists.len() - 1);
        // Explicit position is honored.
        let idx3 = store.create_at("Top", 1);
        assert_eq!(idx3, 1);
        assert_eq!(store.playlists[idx3].name, "Top");
    }
}
