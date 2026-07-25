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

    pub fn save(&self) {
        let path = store_path();
        if let Some(dir) = path.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let _ = std::fs::write(&path, s);
        }
    }

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

    pub fn remove_track(&mut self, playlist_idx: usize, track_idx: usize) {
        if let Some(pl) = self.playlists.get_mut(playlist_idx) {
            if track_idx < pl.tracks.len() {
                pl.tracks.remove(track_idx);
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
