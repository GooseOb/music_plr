use crate::types::{PlayQueue, View};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub current_view: View,
    pub queue: PlayQueue,
    pub is_playing: bool,
    pub selected_playlist: Option<usize>,
    pub selected_playlist_name: String,
    pub show_queue: bool,
}

impl Default for SessionState {
    fn default() -> Self {
        SessionState {
            current_view: View::Search(String::new()),
            queue: PlayQueue::new(),
            is_playing: false,
            selected_playlist: None,
            selected_playlist_name: String::new(),
            show_queue: false,
        }
    }
}

impl SessionState {
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
}

fn store_path() -> PathBuf {
    if let Some(dirs) = directories::ProjectDirs::from("", "", "music_plr") {
        dirs.config_dir().join("session.json")
    } else {
        PathBuf::from("session.json")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_state_default() {
        let state = SessionState::default();
        assert_eq!(state.current_view, View::Search(String::new()));
        assert!(state.queue.tracks.is_empty());
        assert!(!state.is_playing);
        assert_eq!(state.selected_playlist, None);
        assert_eq!(state.selected_playlist_name, "");
        assert!(!state.show_queue);
    }

    #[test]
    fn session_state_round_trip() {
        let state = SessionState {
            current_view: View::SongRadio("test song".into()),
            queue: PlayQueue::default(),
            is_playing: true,
            selected_playlist: Some(2),
            selected_playlist_name: "Test".into(),
            show_queue: true,
        };
        let json = serde_json::to_string(&state).unwrap();
        let restored: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.current_view, View::SongRadio("test song".into()));
        assert!(restored.is_playing);
        assert_eq!(restored.selected_playlist, Some(2));
        assert_eq!(restored.selected_playlist_name, "Test");
        assert!(restored.show_queue);
    }
}
