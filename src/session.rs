use crate::{
    app::ViewSnapshot,
    types::{PlayQueue, View},
};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub view: View,
    pub snapshot: ViewSnapshot,
    pub queue: PlayQueue,
    pub show_queue: bool,
    pub volume: f32,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            view: View::Search,
            snapshot: ViewSnapshot::default(),
            queue: PlayQueue::new(),
            show_queue: false,
            volume: 0.8,
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
        assert_eq!(state.view, View::Search);
        assert!(state.queue.tracks.is_empty());
        assert!(!state.show_queue);
        assert_eq!(state.volume, 0.8);
    }

    #[test]
    fn session_state_round_trip() {
        let state = SessionState {
            view: View::SongRadio,
            snapshot: ViewSnapshot::Radio {
                label: "Test Radio".into(),
                tracks: Vec::new(),
                selection: vec![2],
                scroll: 42.0,
            },
            queue: PlayQueue::default(),
            show_queue: true,
            volume: 0.5,
        };
        let json = serde_json::to_string(&state).unwrap();
        let restored: SessionState = serde_json::from_str(&json).unwrap();
        assert_eq!(restored.view, View::SongRadio);
        assert!(restored.show_queue);
        assert_eq!(restored.volume, 0.5);
        if let ViewSnapshot::Radio {
            label,
            selection,
            scroll,
            ..
        } = restored.snapshot
        {
            assert_eq!(label, "Test Radio");
            assert_eq!(selection, vec![2]);
            assert_eq!(scroll, 42.0);
        } else {
            panic!("expected Radio snapshot");
        }
    }
}
