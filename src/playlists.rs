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
        if let Some(pl) = self.playlists.get_mut(playlist_idx) {
            if !pl.tracks.iter().any(|t| t.url == track.url) {
                let pos = pos.min(pl.tracks.len());
                pl.tracks.insert(pos, track.clone());
                self.save();
            }
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
