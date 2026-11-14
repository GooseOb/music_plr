use std::collections::HashMap;

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::{
    library::{Card, LibraryKind},
    JsonStore, StoreLocation,
};
use crate::{
    app::{pane::PaneId, Pane, SplitNode, ViewData, ViewKind},
    load_state::LoadState,
    providers::{ArtistPage, CardData, ProviderId, SearchTab, SectionContent},
    types::PlayQueue,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionState {
    #[serde(
        serialize_with = "serialize_panes",
        deserialize_with = "deserialize_panes"
    )]
    pub panes: HashMap<PaneId, Pane>,
    pub root: SplitNode,
    pub focused: PaneId,
    pub queue: PlayQueue,
    pub show_queue: bool,
    pub volume: f32,
    pub repeat: bool,
    pub library_expanded: bool,
    /// The view the currently playing track came from, so "reveal now playing"
    /// survives a restart.
    #[serde(default)]
    pub now_playing_from: Option<std::sync::Arc<ViewData>>,
}

impl Default for SessionState {
    fn default() -> Self {
        Self {
            panes: HashMap::from([(0, Pane::home(0))]),
            root: SplitNode::Leaf(0),
            focused: 0,
            queue: PlayQueue::new(),
            show_queue: false,
            volume: 0.8,
            repeat: false,
            library_expanded: true,
            now_playing_from: None,
        }
    }
}

fn serialize_panes<S>(panes: &HashMap<PaneId, Pane>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let mut sorted: Vec<&Pane> = panes.values().collect();
    sorted.sort_by_key(|pane| pane.id);
    sorted.serialize(serializer)
}

fn deserialize_panes<'de, D>(deserializer: D) -> Result<HashMap<PaneId, Pane>, D::Error>
where
    D: Deserializer<'de>,
{
    let panes = Vec::<Pane>::deserialize(deserializer)?;
    Ok(panes.into_iter().map(|pane| (pane.id, pane)).collect())
}

/// v1.5.4 cards carried no kind or provider; fill them from the view that
/// owns the card. Current sessions already agree with their context.
fn backfill_card_identities(state: &mut SessionState) {
    for pane in state.panes.values_mut() {
        for view in &mut pane.nav_history {
            backfill_view(std::sync::Arc::make_mut(view));
        }
    }
    if let Some(view) = state.now_playing_from.as_mut() {
        backfill_view(std::sync::Arc::make_mut(view));
    }
}

fn backfill_view(view: &mut ViewData) {
    match &mut view.kind {
        ViewKind::Search(data) => {
            let provider = data.provider;
            match &mut data.tab {
                SearchTab::Artists(cards) => {
                    backfill_card_data(cards, LibraryKind::Artist, provider);
                }
                SearchTab::Albums(cards) => {
                    backfill_card_data(cards, LibraryKind::Album, provider);
                }
                SearchTab::Playlists(cards) => {
                    backfill_card_data(cards, LibraryKind::Playlist, provider);
                }
                SearchTab::Songs | SearchTab::Videos => {}
            }
        }
        ViewKind::Artist(entry) => {
            for (provider, cached) in &mut entry.page.pages {
                backfill_page(&mut cached.page, *provider);
            }
            for section in &mut entry.page.sections {
                let Some(provider) = section.provider else {
                    continue;
                };
                let LoadState::Ready(content) = &mut section.state else {
                    continue;
                };
                backfill_section(content, provider);
            }
        }
        _ => {}
    }
}

fn backfill_page(page: &mut ArtistPage, provider: ProviderId) {
    for album in &mut page.albums {
        backfill_card(&mut album.card, LibraryKind::Album, provider);
    }
    backfill_card_data(&mut page.playlists, LibraryKind::Playlist, provider);
    for related in &mut page.related {
        backfill_card(&mut related.card, LibraryKind::Artist, provider);
    }
}

fn backfill_section(content: &mut SectionContent, provider: ProviderId) {
    match content {
        SectionContent::Albums(albums) => {
            for album in albums {
                backfill_card(&mut album.card, LibraryKind::Album, provider);
            }
        }
        SectionContent::Playlists(cards) => {
            backfill_card_data(cards, LibraryKind::Playlist, provider);
        }
        SectionContent::Related(related) => {
            for card in related {
                backfill_card(&mut card.card, LibraryKind::Artist, provider);
            }
        }
        SectionContent::Header(_) | SectionContent::Tracks(_) => {}
    }
}

fn backfill_card_data(cards: &mut [CardData], kind: LibraryKind, provider: ProviderId) {
    for data in cards {
        backfill_card(&mut data.card, kind, provider);
    }
}

fn backfill_card(card: &mut Card, kind: LibraryKind, provider: ProviderId) {
    if card.kind != kind || card.provider != provider {
        let item = std::sync::Arc::make_mut(card);
        item.kind = kind;
        item.provider = provider;
    }
}

/// The pre-split single-view session format. Loaded only to migrate old
/// `session.json` files into a single-pane tree.
#[derive(Debug, Deserialize)]
struct LegacySessionState {
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
}

fn default_volume() -> f32 {
    0.8
}

fn default_expanded() -> bool {
    true
}

impl From<LegacySessionState> for SessionState {
    fn from(old: LegacySessionState) -> Self {
        let mut pane = Pane::new(0);
        pane.nav_history = vec![std::sync::Arc::new(old.data)];
        pane.search_scope = old.search_scope;
        pane.search_provider = old.search_provider;
        Self {
            panes: HashMap::from([(0, pane)]),
            root: SplitNode::Leaf(0),
            focused: 0,
            queue: old.queue,
            show_queue: old.show_queue,
            volume: old.volume,
            repeat: old.repeat,
            library_expanded: old.library_expanded,
            now_playing_from: None,
        }
    }
}

impl SessionState {
    /// Parse a saved session, accepting the v1.5.4 flat card shape and
    /// migrating a pre-split single-view file into a single-pane tree.
    fn from_json(raw: &str) -> Option<Self> {
        if let Ok(mut state) = serde_json::from_str::<SessionState>(raw) {
            if !state.panes.is_empty() {
                for pane in state.panes.values_mut() {
                    pane.normalize_nav_history();
                }
                backfill_card_identities(&mut state);
                return Some(state);
            }
        }
        let mut state = Self::from(serde_json::from_str::<LegacySessionState>(raw).ok()?);
        backfill_card_identities(&mut state);
        Some(state)
    }

    /// Load the session, migrating older formats when needed. Corrupt or
    /// missing files degrade to the default, like every other [`JsonStore`].
    pub fn load_migrated() -> Self {
        // `JsonStore::save` is a no-op under `cfg(test)` so tests never write to
        // the real user directories; *reading* the live session would still make
        // them depend on whatever the running app left there (which queue tab was
        // open, how many panes, …), so tests start from the default.
        #[cfg(not(test))]
        {
            let raw = std::fs::read_to_string(Self::path()).ok();
            let Some(raw) = raw else {
                return Self::default();
            };
            if let Some(state) = Self::from_json(&raw) {
                return state;
            }
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
            state.panes[&0].nav_history[0].kind,
            ViewKind::Home(_)
        ));
        assert!(state.queue.tracks.is_empty());
        assert!(!state.show_queue);
        assert!((state.volume - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn session_state_panes_serialize_as_a_sorted_array() {
        let state = SessionState {
            panes: HashMap::from([(2, Pane::new(2)), (0, Pane::new(0))]),
            ..SessionState::default()
        };
        let value = serde_json::to_value(&state).unwrap();
        let panes = value["panes"].as_array().expect("panes stay an array");
        assert_eq!(panes.len(), 2);
        assert_eq!(panes[0]["id"], 0);
        assert_eq!(panes[1]["id"], 2);
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
        let state = SessionState::from_json(&raw).expect("legacy session migrates");
        assert_eq!(state.panes.len(), 1);
        assert_eq!(state.focused, 0);
        assert!(matches!(state.root, SplitNode::Leaf(0)));
        assert!(matches!(
            state.panes[&0].nav_history[0].kind,
            ViewKind::Playlist(_)
        ));
        assert!((state.volume - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn broken_modern_session_is_not_treated_as_legacy() {
        let raw = r#"{
            "panes": [{"id": 0}],
            "root": {"Leaf": 0},
            "focused": 0,
            "queue": {"tracks": [], "recently_played": [], "queue_tab": "Queue"},
            "show_queue": false,
            "volume": 0.8,
            "repeat": false,
            "library_expanded": true,
            "lyrics_provider": "lrclib"
        }"#;
        assert!(serde_json::from_str::<LegacySessionState>(raw).is_err());
        assert!(SessionState::from_json(raw).is_none());
    }

    #[test]
    #[allow(clippy::too_many_lines)]
    fn v154_session_keeps_card_backed_views() {
        let raw = r#"{
            "panes": [
                {
                    "id": 0,
                    "nav_history": [{
                        "kind": {"Search": {
                            "query": "night",
                            "provider": "SoundCloud",
                            "tab": {"Artists": [{
                                "id": "artist-1",
                                "title": "Night Artist",
                                "subtitle": "Artist",
                                "thumbnail": "https://img/artist",
                                "url": "https://soundcloud.com/night"
                            }]},
                            "exhausted": false
                        }},
                        "content": [],
                        "selection": [],
                        "scroll": 0.0
                    }],
                    "nav_history_pos": 0,
                    "search_query": "night",
                    "search_scope": "Artists",
                    "search_provider": "SoundCloud"
                },
                {
                    "id": 1,
                    "nav_history": [{
                        "kind": {"Artist": {
                            "id": "artist-2",
                            "name": "Album Artist",
                            "source": "YouTube",
                            "url": "https://music.youtube.com/channel/artist-2",
                            "thumbnail": "https://img/artist-2",
                            "page": {
                                "provider_ids": {"YouTube": "artist-2"},
                                "pages": {"YouTube": {
                                    "page": {
                                        "header": null,
                                        "popular": [],
                                        "albums": [{
                                            "id": "album-1",
                                            "title": "Album One",
                                            "date": "2024",
                                            "badge": "Album",
                                            "thumbnail": "https://img/album",
                                            "url": "https://music.youtube.com/browse/album-1"
                                        }],
                                        "playlists": [{
                                            "id": "playlist-1",
                                            "title": "Playlist One",
                                            "subtitle": "Playlist",
                                            "thumbnail": "https://img/playlist",
                                            "url": "https://music.youtube.com/playlist?list=playlist-1"
                                        }],
                                        "related": [{
                                            "id": "related-1",
                                            "name": "Related One",
                                            "stat": "1M subscribers",
                                            "thumbnail": "https://img/related",
                                            "url": "https://music.youtube.com/channel/related-1"
                                        }]
                                    },
                                    "fetched": ["Albums", "Playlists", "Related"]
                                }},
                                "header_provider": "YouTube",
                                "header": null,
                                "sections": [
                                    {"provider": "YouTube", "state": null},
                                    {"provider": "YouTube", "state": {"Albums": [{
                                        "id": "album-section",
                                        "title": "Section Album",
                                        "date": "2023",
                                        "badge": "EP",
                                        "thumbnail": "https://img/section-album",
                                        "url": "https://music.youtube.com/browse/album-section"
                                    }]}},
                                    {"provider": "YouTube", "state": {"Playlists": []}},
                                    {"provider": "YouTube", "state": {"Related": []}}
                                ]
                            }
                        }},
                        "content": [],
                        "selection": [],
                        "scroll": 0.0
                    }],
                    "nav_history_pos": 0,
                    "search_query": "",
                    "search_scope": "Songs",
                    "search_provider": "YouTube"
                }
            ],
            "root": {"Row": {"first": {"Leaf": 0}, "second": {"Leaf": 1}}},
            "focused": 1,
            "queue": {"tracks": [], "recently_played": [], "queue_tab": "Queue"},
            "show_queue": true,
            "volume": 0.5,
            "repeat": true,
            "library_expanded": false,
            "lyrics_provider": "lrclib",
            "now_playing_from": {
                "kind": {"Search": {
                    "query": "dawn",
                    "provider": "Bandcamp",
                    "tab": {"Albums": [{
                        "id": "now-album",
                        "title": "Now Album",
                        "subtitle": "Album",
                        "thumbnail": "https://img/now",
                        "url": "https://bandcamp.com/now"
                    }]},
                    "exhausted": true
                }},
                "content": [],
                "selection": [],
                "scroll": 0.0
            }
        }"#;

        let state = SessionState::from_json(raw).expect("v1.5.4 session migrates");
        assert_eq!(state.panes.len(), 2);
        assert!(matches!(state.root, SplitNode::Row { .. }));
        assert_eq!(state.focused, 1);
        assert!(state.show_queue);
        assert!((state.volume - 0.5).abs() < f32::EPSILON);
        assert!(state.repeat);
        assert!(!state.library_expanded);

        let ViewKind::Search(search) = &state.panes[&0].nav_history[0].kind else {
            panic!("expected artists search");
        };
        let SearchTab::Artists(cards) = &search.tab else {
            panic!("expected artist cards");
        };
        let card = &cards[0];
        assert_eq!(card.subtitle, "Artist");
        assert_eq!(card.card.kind, LibraryKind::Artist);
        assert_eq!(card.card.provider, ProviderId::SoundCloud);
        assert_eq!(card.card.id, "artist-1");
        assert_eq!(card.card.title, "Night Artist");
        assert_eq!(card.card.thumbnail, "https://img/artist");
        assert_eq!(card.card.url, "https://soundcloud.com/night");

        let ViewKind::Artist(artist) = &state.panes[&1].nav_history[0].kind else {
            panic!("expected artist page");
        };
        let page = &artist.page.pages[&ProviderId::YouTube].page;
        assert_eq!(page.albums[0].card.kind, LibraryKind::Album);
        assert_eq!(page.albums[0].card.provider, ProviderId::YouTube);
        assert_eq!(page.albums[0].card.id, "album-1");
        assert_eq!(page.albums[0].card.title, "Album One");
        assert_eq!(page.albums[0].badge, "Album");
        assert_eq!(page.albums[0].date, "2024");
        assert_eq!(page.playlists[0].card.kind, LibraryKind::Playlist);
        assert_eq!(page.playlists[0].card.provider, ProviderId::YouTube);
        assert_eq!(page.playlists[0].card.title, "Playlist One");
        assert_eq!(page.related[0].card.kind, LibraryKind::Artist);
        assert_eq!(page.related[0].card.provider, ProviderId::YouTube);
        assert_eq!(page.related[0].card.title, "Related One");
        assert_eq!(page.related[0].stat, "1M subscribers");

        let LoadState::Ready(SectionContent::Albums(section_albums)) =
            &artist.page.sections[1].state
        else {
            panic!("expected a ready albums section");
        };
        assert_eq!(section_albums[0].card.kind, LibraryKind::Album);
        assert_eq!(section_albums[0].card.provider, ProviderId::YouTube);
        assert_eq!(section_albums[0].card.title, "Section Album");

        let now = state.now_playing_from.as_ref().expect("now playing source");
        let ViewKind::Search(search) = &now.kind else {
            panic!("expected a search now-playing source");
        };
        let SearchTab::Albums(cards) = &search.tab else {
            panic!("expected album cards");
        };
        assert_eq!(cards[0].card.kind, LibraryKind::Album);
        assert_eq!(cards[0].card.provider, ProviderId::Bandcamp);
        assert_eq!(cards[0].card.title, "Now Album");

        let json = serde_json::to_string(&state).unwrap();
        let restored = SessionState::from_json(&json).expect("current session round trips");
        let ViewKind::Artist(artist) = &restored.panes[&1].nav_history[0].kind else {
            panic!("expected round-tripped artist page");
        };
        let page = &artist.page.pages[&ProviderId::YouTube].page;
        assert_eq!(page.related[0].card.title, "Related One");
        assert_eq!(page.related[0].card.provider, ProviderId::YouTube);
    }

    #[test]
    fn session_state_round_trip() {
        let state = SessionState {
            panes: HashMap::from([(
                0,
                Pane {
                    nav_history: vec![std::sync::Arc::new(ViewData {
                        kind: ViewKind::ArtistRadio(crate::app::RadioData {
                            title: "Test Radio".into(),
                            ..Default::default()
                        }),
                        content: crate::load_state::LoadState::Ready(Vec::new()),
                        selection: vec![2],
                        scroll: 42.0,
                        request_id: 0,
                        thumbnails_seeded: false,
                    })],
                    search_scope: crate::providers::SearchScope::Songs,
                    search_provider: crate::providers::ProviderId::YouTube,
                    ..Pane::new(0)
                },
            )]),
            root: SplitNode::Leaf(0),
            focused: 0,
            queue: PlayQueue::default(),
            show_queue: true,
            volume: 0.5,
            repeat: true,
            library_expanded: false,
            now_playing_from: None,
        };
        let json = serde_json::to_string(&state).unwrap();
        let restored: SessionState = serde_json::from_str(&json).unwrap();
        assert!(matches!(
            restored.panes[&0].nav_history[0].kind,
            ViewKind::SongRadio(_) | ViewKind::ArtistRadio(_)
        ));
        assert!(restored.show_queue);
        assert!((restored.volume - 0.5).abs() < f32::EPSILON);
        assert!(restored.repeat);
        assert!(!restored.library_expanded);
        if let ViewKind::ArtistRadio(data) = &restored.panes[&0].nav_history[0].kind {
            assert_eq!(data.title, "Test Radio");
            assert_eq!(restored.panes[&0].nav_history[0].selection, vec![2]);
            assert!((restored.panes[&0].nav_history[0].scroll - 42.0).abs() < f32::EPSILON);
        } else {
            panic!("expected Radio data");
        }
    }

    #[test]
    fn legacy_string_radio_kind_migrates_title() {
        let kind: ViewKind =
            serde_json::from_value(serde_json::json!({"ArtistRadio": "Radio (artist): Old"}))
                .unwrap();
        let ViewKind::ArtistRadio(data) = kind else {
            panic!("expected ArtistRadio");
        };
        assert_eq!(data.title, "Radio (artist): Old");
        assert!(data.thumb_id.is_empty());
    }
}
