use serde::{Deserialize, Serialize};

use super::{JsonStore, StoreLocation};
#[cfg(test)]
use crate::app::ViewKind;
use crate::{
    app::{pane::PaneId, PaneData, SplitNode, ViewData},
    types::PlayQueue,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    pub panes: Vec<PaneData>,
    pub root: SplitNode,
    pub focused: PaneId,
    pub queue: PlayQueue,
    pub show_queue: bool,
    pub volume: f32,
    pub repeat: bool,
    pub library_expanded: bool,
    pub lyrics_provider: crate::lyrics::LyricsProvider,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            panes: vec![PaneData {
                id: 0,
                nav_history: vec![ViewData::default()],
                nav_history_pos: 0,
                search_query: String::new(),
                search_scope: crate::providers::SearchScope::Songs,
                search_provider: crate::providers::ProviderId::YouTube,
            }],
            root: SplitNode::Leaf(0),
            focused: 0,
            queue: PlayQueue::new(),
            show_queue: false,
            volume: 0.8,
            repeat: false,
            library_expanded: true,
            lyrics_provider: crate::lyrics::LyricsProvider::default(),
        }
    }
}

/// The pre-split single-view session format. Loaded only to migrate old
/// `session.json` files into a single-pane tree.
#[derive(Debug, Deserialize)]
struct LegacySessionState {
    #[serde(default)]
    data: ViewData,
    #[serde(default)]
    queue: PlayQueue,
    #[serde(default)]
    show_queue: bool,
    #[serde(default = "default_volume")]
    volume: f32,
    #[serde(default)]
    repeat: bool,
    #[serde(default = "default_expanded")]
    library_expanded: bool,
    #[serde(default)]
    search_scope: crate::providers::SearchScope,
    #[serde(default)]
    search_provider: crate::providers::ProviderId,
    #[serde(default)]
    lyrics_provider: crate::lyrics::LyricsProvider,
}

fn default_volume() -> f32 {
    0.8
}

fn default_expanded() -> bool {
    true
}

impl From<LegacySessionState> for SessionState {
    fn from(old: LegacySessionState) -> Self {
        Self {
            panes: vec![PaneData {
                id: 0,
                nav_history: vec![old.data],
                nav_history_pos: 0,
                search_query: String::new(),
                search_scope: old.search_scope,
                search_provider: old.search_provider,
            }],
            root: SplitNode::Leaf(0),
            focused: 0,
            queue: old.queue,
            show_queue: old.show_queue,
            volume: old.volume,
            repeat: old.repeat,
            library_expanded: old.library_expanded,
            lyrics_provider: old.lyrics_provider,
        }
    }
}

impl SessionState {
    /// Load the session, migrating a pre-split single-view file into a
    /// single-pane tree when needed. Corrupt or missing files degrade to the
    /// default, like every other [`JsonStore`].
    pub fn load_migrated() -> Self {
        let raw = std::fs::read_to_string(Self::path()).ok();
        let Some(raw) = raw else {
            return Self::default();
        };
        if let Ok(state) = serde_json::from_str::<SessionState>(&raw) {
            if !state.panes.is_empty() {
                return state;
            }
        }
        if let Ok(legacy) = serde_json::from_str::<LegacySessionState>(&raw) {
            return Self::from(legacy);
        }
        Self::default()
    }
}

impl JsonStore for SessionState {
    const FILE: &'static str = "session.json";
    const LOCATION: StoreLocation = StoreLocation::Cache;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn session_state_default() {
        let state = SessionState::default();
        assert!(matches!(
            state.panes[0].nav_history[0].kind,
            ViewKind::Search { .. }
        ));
        assert!(state.queue.tracks.is_empty());
        assert!(!state.show_queue);
        assert!((state.volume - 0.8).abs() < f32::EPSILON);
        assert_eq!(state.lyrics_provider, crate::lyrics::LyricsProvider::LrcLib);
    }

    #[test]
    fn legacy_session_migrates_to_single_pane() {
        let legacy = serde_json::json!({
            "data": {
                "kind": {"Playlist": {"index": 2, "name": "Mine"}},
                "content": [],
                "selection": [],
                "scroll": 0.0,
                "request_id": 0
            },
            "queue": {"tracks": [], "index": 0, "queue_tab": "Queue", "recently_played": []},
            "show_queue": false,
            "volume": 0.5,
            "repeat": false,
            "library_expanded": true,
            "search_scope": "Songs",
            "search_provider": "YouTube",
            "lyrics_provider": "lrclib"
        });
        let raw = serde_json::to_string(&legacy).unwrap();
        assert!(serde_json::from_str::<SessionState>(&raw).is_err());
        let old: LegacySessionState = serde_json::from_str(&raw).unwrap();
        let state = SessionState::from(old);
        assert_eq!(state.panes.len(), 1);
        assert_eq!(state.focused, 0);
        assert!(matches!(state.root, SplitNode::Leaf(0)));
        assert!(matches!(
            state.panes[0].nav_history[0].kind,
            ViewKind::Playlist(_)
        ));
        assert!((state.volume - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn session_state_round_trip() {
        let state = SessionState {
            panes: vec![PaneData {
                id: 0,
                nav_history: vec![ViewData {
                    kind: ViewKind::ArtistRadio("Test Radio".into()),
                    content: crate::load_state::LoadState::Ready(Vec::new()),
                    selection: vec![2],
                    scroll: 42.0,
                    request_id: 0,
                }],
                nav_history_pos: 0,
                search_query: String::new(),
                search_scope: crate::providers::SearchScope::Songs,
                search_provider: crate::providers::ProviderId::YouTube,
            }],
            root: SplitNode::Leaf(0),
            focused: 0,
            queue: PlayQueue::default(),
            show_queue: true,
            volume: 0.5,
            repeat: true,
            library_expanded: false,
            lyrics_provider: crate::lyrics::LyricsProvider::LrcLib,
        };
        let json = serde_json::to_string(&state).unwrap();
        let restored: SessionState = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            restored.panes[0].nav_history[0].kind,
            ViewKind::SongRadio(_) | ViewKind::ArtistRadio(_)
        ));
        assert!(restored.show_queue);
        assert!((restored.volume - 0.5).abs() < f32::EPSILON);
        assert!(restored.repeat);
        assert!(!restored.library_expanded);
        assert_eq!(
            restored.lyrics_provider,
            crate::lyrics::LyricsProvider::LrcLib
        );
        if let ViewKind::ArtistRadio(label) = &restored.panes[0].nav_history[0].kind {
            assert_eq!(label, "Test Radio");
            assert_eq!(restored.panes[0].nav_history[0].selection, vec![2]);
            assert!((restored.panes[0].nav_history[0].scroll - 42.0).abs() < f32::EPSILON);
        } else {
            panic!("expected Radio data");
        }
    }
}
