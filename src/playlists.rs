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

    pub fn add_track(&mut self, playlist_idx: usize, track: &Track) {
        if let Some(pl) = self.playlists.get_mut(playlist_idx) {
            if !pl.tracks.iter().any(|t| t.url == track.url) {
                pl.tracks.push(track.clone());
                self.save();
            }
        }
    }

    pub fn insert_track_at(&mut self, playlist_idx: usize, track: &Track, pos: usize) {
        if let Some(pl) = self.playlists.get_mut(playlist_idx) {
            if !pl.tracks.iter().any(|t| t.url == track.url) {
                let pos = pos.min(pl.tracks.len());
                pl.tracks.insert(pos, track.clone());
                self.save();
            }
        }
    }

    pub fn remove_track(&mut self, playlist_idx: usize, track_idx: usize) {
        if let Some(pl) = self.playlists.get_mut(playlist_idx) {
            if track_idx < pl.tracks.len() {
                pl.tracks.remove(track_idx);
                self.save();
            }
        }
    }

    pub fn move_tracks(&mut self, playlist_idx: usize, from_indices: &[usize], to_idx: usize) {
        if let Some(pl) = self.playlists.get_mut(playlist_idx) {
            let mut indices: Vec<usize> = from_indices.to_vec();
            indices.sort_unstable();
            indices.dedup();

            if indices.is_empty() {
                return;
            }

            let mut moved: Vec<Track> = Vec::new();
            for &i in indices.iter().rev() {
                if i < pl.tracks.len() {
                    moved.push(pl.tracks.remove(i));
                }
            }
            moved.reverse();

            let removed_before = indices.iter().filter(|&&i| i < to_idx).count();
            let target = (to_idx.saturating_sub(removed_before)).min(pl.tracks.len());

            for (offset, track) in moved.into_iter().enumerate() {
                pl.tracks.insert(target + offset, track);
            }

            self.save();
        }
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
                    artist: "".to_string(),
                    duration: 0,
                    url: s.to_string(),
                    source: crate::types::TrackSource::YouTube,
                    thumbnail: "".to_string(),
                })
                .collect(),
        };
        PlaylistStore {
            playlists: vec![playlist],
        }
    }

    #[test]
    fn move_tracks_same_position() {
        let mut store = make_store(&["a", "b", "c"]);
        store.move_tracks(0, &[0], 0);
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
    fn move_tracks_to_end() {
        let mut store = make_store(&["a", "b", "c", "d"]);
        store.move_tracks(0, &[0], 4);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "c", "d", "a"]
        );
    }

    #[test]
    fn move_tracks_insert_before() {
        let mut store = make_store(&["a", "b", "c", "d"]);
        store.move_tracks(0, &[0], 3);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "c", "a", "d"]
        );
    }

    #[test]
    fn move_tracks_backward() {
        let mut store = make_store(&["a", "b", "c", "d"]);
        store.move_tracks(0, &[3], 0);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["d", "a", "b", "c"]
        );
    }

    #[test]
    fn move_tracks_multiple() {
        let mut store = make_store(&["a", "b", "c", "d", "e"]);
        store.move_tracks(0, &[0, 2], 4);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "d", "a", "c", "e"]
        );
    }

    #[test]
    fn move_tracks_out_of_bounds_clamped() {
        let mut store = make_store(&["a", "b", "c"]);
        store.move_tracks(0, &[0], 100);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["b", "c", "a"]
        );
    }

    #[test]
    fn remove_track_from_middle() {
        let mut store = make_store(&["a", "b", "c"]);
        store.remove_track(0, 1);
        assert_eq!(
            store.playlists[0]
                .tracks
                .iter()
                .map(|t| t.id.as_str())
                .collect::<Vec<_>>(),
            vec!["a", "c"]
        );
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
            artist: "".to_string(),
            duration: 0,
            url: "new".to_string(),
            source: crate::types::TrackSource::YouTube,
            thumbnail: "".to_string(),
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
            artist: "".to_string(),
            duration: 0,
            url: "new".to_string(),
            source: crate::types::TrackSource::YouTube,
            thumbnail: "".to_string(),
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
            artist: "".to_string(),
            duration: 0,
            url: "new".to_string(),
            source: crate::types::TrackSource::YouTube,
            thumbnail: "".to_string(),
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
            artist: "".to_string(),
            duration: 0,
            url: "a".to_string(),
            source: crate::types::TrackSource::YouTube,
            thumbnail: "".to_string(),
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
}
