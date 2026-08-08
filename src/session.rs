#[cfg(test)]
use crate::app::ViewKind;
use crate::{app::ViewData, types::PlayQueue};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub data: ViewData,
    pub queue: PlayQueue,
    pub show_queue: bool,
    pub volume: f32,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            data: ViewData::default(),
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
        assert!(matches!(state.data.kind, ViewKind::Search { .. }));
        assert!(state.queue.tracks.is_empty());
        assert!(!state.show_queue);
        assert!((state.volume - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn session_state_round_trip() {
        let state = SessionState {
            data: ViewData {
                kind: ViewKind::ArtistRadio("Test Radio".into()),
                tracks: Vec::new(),
                loading: false,
                selection: vec![2],
                scroll: 42.0,
                bounds: None,
            },
            queue: PlayQueue::default(),
            show_queue: true,
            volume: 0.5,
        };
        let json = serde_json::to_string(&state).unwrap();
        let restored: SessionState = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            restored.data.kind,
            ViewKind::SongRadio(_) | ViewKind::ArtistRadio(_)
        ));
        assert!(restored.show_queue);
        assert!((restored.volume - 0.5).abs() < f32::EPSILON);
        if let ViewKind::ArtistRadio(label) = &restored.data.kind {
            assert_eq!(label, "Test Radio");
            assert_eq!(restored.data.selection, vec![2]);
            assert!((restored.data.scroll - 42.0).abs() < f32::EPSILON);
        } else {
            panic!("expected Radio data");
        }
    }
}
