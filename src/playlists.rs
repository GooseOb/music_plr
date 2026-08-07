use crate::types::Track;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Playlist {
    pub name: String,
    pub tracks: Vec<Track>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct PlaylistStore {
    pub playlists: Vec<Playlist>,
}

impl PlaylistStore {
    pub fn load() -> Self {
        let path = store_path();
        std::fs::read_to_string(&path)
            .ok()
            .and_then(|s| serde_json::from_str(&s).ok())
            .unwrap_or_default()
    }

    #[cfg(not(test))]
    pub fn save(&self) {
        let path = store_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, s);
        }
    }

    #[cfg(test)]
    pub fn save(&self) {}

    pub fn create(&mut self, name: &str) {
        if !name.trim().is_empty() && !self.playlists.iter().any(|p| p.name == name.trim()) {
            self.playlists.push(Playlist {
                name: name.trim().to_string(),
                tracks: Vec::new(),
            });
            self.save();
        }
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
        let mut sorted: Vec<usize> = indices.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        let mut removed = 0;
        for &i in sorted.iter().rev() {
            if i < pl.tracks.len() {
                pl.tracks.remove(i);
                removed += 1;
            }
        }
        self.save();
        removed
    }
}

fn store_path() -> PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("", "", "music_plr") {
        dirs.config_dir().join("playlists.json")
    } else {
        PathBuf::from("playlists.json")
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
                    artist: String::new(),
                    duration: 0,
                    url: s.to_string(),
                    source: crate::types::TrackSource::YouTube,
                    thumbnail: String::new(),
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
            artist: String::new(),
            duration: 0,
            url: "new".to_string(),
            source: crate::types::TrackSource::YouTube,
            thumbnail: String::new(),
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
            artist: String::new(),
            duration: 0,
            url: "new".to_string(),
            source: crate::types::TrackSource::YouTube,
            thumbnail: String::new(),
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
            artist: String::new(),
            duration: 0,
            url: "new".to_string(),
            source: crate::types::TrackSource::YouTube,
            thumbnail: String::new(),
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
            artist: String::new(),
            duration: 0,
            url: "a".to_string(),
            source: crate::types::TrackSource::YouTube,
            thumbnail: String::new(),
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
    fn remove_tracks_at_multiple() {
        let mut store = make_store(&["a", "b", "c", "d", "e"]);
        store.remove_tracks_at(0, &[1, 3]);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "c", "e"]
        );
    }

    #[test]
    fn remove_tracks_at_out_of_bounds_ignored() {
        let mut store = make_store(&["a", "b", "c"]);
        store.remove_tracks_at(0, &[0, 99]);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "c"]
        );
    }

    #[test]
    fn remove_tracks_at_unsorted() {
        let mut store = make_store(&["a", "b", "c", "d", "e"]);
        store.remove_tracks_at(0, &[3, 0, 4]);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "c"]
        );
    }

    #[test]
    fn remove_tracks_at_nonexistent_playlist() {
        let mut store = make_store(&["a"]);
        store.remove_tracks_at(99, &[0]);
        assert_eq!(store.playlists[0].tracks.len(), 1);
    }
}
