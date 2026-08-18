use super::{JsonStore, StoreLocation};
use crate::types::Track;
use serde::{Deserialize, Serialize};

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

    pub fn insert_track_at(&mut self, playlist_idx: usize, track: &Track, pos: usize) {
        self.insert_tracks_at(playlist_idx, std::slice::from_ref(track), pos);
    }

    /// Insert multiple tracks at once, writing to disk only once.
    /// `pos` is clamped per-track to the growing list length.
    pub fn insert_tracks_at(&mut self, playlist_idx: usize, tracks: &[Track], pos: usize) {
        let Some(pl) = self.playlists.get_mut(playlist_idx) else {
            return;
        };
        let mut insert_pos = pos;
        for track in tracks {
            if !pl.tracks.iter().any(|t| t.url == track.url) {
                insert_pos = insert_pos.min(pl.tracks.len());
                pl.tracks.insert(insert_pos, track.clone());
                insert_pos += 1;
            }
        }
        self.save();
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

    fn make_store(tracks: &[&str]) -> PlaylistStore {
        let playlist = Playlist {
            name: "Test".to_string(),
            tracks: tracks
                .iter()
                .map(|s| Track {
                    id: s.to_string(),
                    title: s.to_string(),
                    artist: crate::types::TrackArtist {
                        name: String::new(),
                        id: None,
                    },
                    duration: 0,
                    url: s.to_string(),
                    source: crate::types::TrackSource::YouTube,
                    thumbnail: String::new(),
                    download_path: None,
                    album: None,
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
        let new_track = Track {
            id: "new".to_string(),
            title: "new".to_string(),
            artist: crate::types::TrackArtist {
                name: String::new(),
                id: None,
            },
            duration: 0,
            url: "new".to_string(),
            source: crate::types::TrackSource::YouTube,
            thumbnail: String::new(),
            download_path: None,
            album: None,
        };
        store.insert_track_at(0, &new_track, 0);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["new", "a", "b", "c"]
        );
    }

    #[test]
    fn insert_track_at_position() {
        let mut store = make_store(&["a", "b", "c"]);
        let new_track = Track {
            id: "new".to_string(),
            title: "new".to_string(),
            artist: crate::types::TrackArtist {
                name: String::new(),
                id: None,
            },
            duration: 0,
            url: "new".to_string(),
            source: crate::types::TrackSource::YouTube,
            thumbnail: String::new(),
            download_path: None,
            album: None,
        };
        store.insert_track_at(0, &new_track, 2);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "new", "c"]
        );
    }

    #[test]
    fn insert_track_at_clamps_position() {
        let mut store = make_store(&["a", "b", "c"]);
        let new_track = Track {
            id: "new".to_string(),
            title: "new".to_string(),
            artist: crate::types::TrackArtist {
                name: String::new(),
                id: None,
            },
            duration: 0,
            url: "new".to_string(),
            source: crate::types::TrackSource::YouTube,
            thumbnail: String::new(),
            download_path: None,
            album: None,
        };
        store.insert_track_at(0, &new_track, 100);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "b", "c", "new"]
        );
    }

    #[test]
    fn insert_track_at_dedup_ignored() {
        let mut store = make_store(&["a", "b", "c"]);
        let dup_track = Track {
            id: "a".to_string(),
            title: "a".to_string(),
            artist: crate::types::TrackArtist {
                name: String::new(),
                id: None,
            },
            duration: 0,
            url: "a".to_string(),
            source: crate::types::TrackSource::YouTube,
            thumbnail: String::new(),
            download_path: None,
            album: None,
        };
        store.insert_track_at(0, &dup_track, 0);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.id.as_str())
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
