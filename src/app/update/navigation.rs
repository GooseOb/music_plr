use super::{Message, MusicPlayer, Task, ViewData};
use crate::app::ui::TRACK_LIST_ID;
use crate::app::{interaction::TrackListKind, interaction::TrackPos, ViewKind};

impl MusicPlayer {
    pub const fn can_navigate_back(&self) -> bool {
        self.nav_history_pos > 0
    }

    pub const fn can_navigate_forward(&self) -> bool {
        self.nav_history_pos + 1 < self.nav_history.len()
    }

    fn sync_search_query(&mut self) {
        if let ViewKind::Search(s) = &self.view_data().kind {
            self.search_query = s.query.clone();
        }
    }

    pub(super) fn sync_search_scope(&mut self) {
        if let ViewKind::Search(s) = &self.view_data().kind {
            self.search_scope = s.tab.scope();
        }
    }

    pub(super) fn sync_search_provider(&mut self) {
        if let ViewKind::Search(s) = &self.view_data().kind {
            self.search_provider = s.provider;
        }
    }

    pub(super) fn restore_nav_entry(&mut self, data: ViewData) -> Task<Message> {
        // Scroll position is stored relative to the main track_list scrollable.
        // (Queue view uses a different Id and is not navigated via history.)
        let y = data.scroll;
        *self.view_data_mut() = data;
        self.sync_search_query();
        self.sync_search_scope();
        self.sync_search_provider();
        self.sync_downloads_view();

        iced::widget::operation::scroll_to::<Message>(
            TRACK_LIST_ID,
            iced::widget::operation::AbsoluteOffset { x: 0.0, y },
        )
    }

    pub(super) fn sync_downloads_view(&mut self) {
        if matches!(self.view_data().kind, ViewKind::Downloads) {
            let tracks = self.download_registry.clone_tracks();
            self.view_data_mut().set_tracks(tracks);
        }
    }

    /// Replace the current view, recording the destination as a *new* history
    /// slot while leaving `nav_history[pos]` (the outgoing view) intact. This
    /// mirrors the old design where `view_data` was a separate field from the
    /// history clone: overwriting the live slot must not clobber the entry we
    /// can navigate Back to.
    pub(super) fn push_new_view(&mut self, data: ViewData) {
        self.nav_history.truncate(self.nav_history_pos + 1);
        self.nav_history.push(data);
        if self.nav_history.len() > 20 {
            self.nav_history.remove(0);
        }
        self.nav_history_pos = self.nav_history.len() - 1;
    }

    pub fn handle_navigate_to(&mut self, data: ViewData) {
        if self.view_data().same_kind(&data) {
            return;
        }
        // Capture the live query into the outgoing `Search` entry (if any) so
        // Back navigation restores it.
        let live_query = self.search_query.clone();
        if let ViewKind::Search(s) = &mut self.view_data_mut().kind {
            s.query = live_query;
        }
        self.drag.cleanup();

        // Push the destination as a fresh slot; the outgoing view stays at the
        // previous position.
        self.push_new_view(data);
        self.sync_search_query();
        self.sync_search_scope();
        self.sync_search_provider();
        self.sync_downloads_view();

        let view = self.view_data().clone();
        self.seed_view_thumbnails(&view);
        self.save_session();
    }

    pub fn handle_navigate_back(&mut self) -> Task<Message> {
        if self.can_navigate_back() {
            self.nav_history_pos -= 1;
            return self.sync_navigation();
        }
        Task::none()
    }

    pub fn handle_navigate_forward(&mut self) -> Task<Message> {
        if self.can_navigate_forward() {
            self.nav_history_pos += 1;
            return self.sync_navigation();
        }
        Task::none()
    }

    fn sync_navigation(&mut self) -> Task<Message> {
        let entry = self.nav_history[self.nav_history_pos].clone();
        let task = self.restore_nav_entry(entry);
        self.save_session();
        task
    }

    pub fn handle_reveal_now_playing(&mut self) -> Task<Message> {
        let Some(origin) = self.now_playing_from.clone() else {
            return Task::none();
        };
        let Some(track) = self.queue.current().cloned() else {
            return Task::none();
        };
        self.handle_navigate_to(origin);
        let key = track.cache_key();
        let index = self.view_tracks().iter().position(|t| t.cache_key() == key);
        let Some(index) = index else {
            return Task::none();
        };
        self.move_hovered(TrackPos::new(index, TrackListKind::Active))
    }

    pub(super) fn slot_for_request(&self, rid: u64) -> Option<usize> {
        self.nav_history.iter().position(|v| v.request_id == rid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{message::BackendResult, ViewKind};
    use crate::data::config;
    use crate::providers::ProviderId;
    use crate::providers::SearchScope;

    fn player() -> MusicPlayer {
        // `new_with` inits MPRIS (spawns a thread, no-ops if D-Bus is absent),
        // so it is safe to construct headlessly in tests. The nav history is
        // reset to a deterministic Playlist view so the navigation tests
        // don't depend on on-disk session state.
        let mut p = MusicPlayer::new_with(config::Config::default());
        p.nav_history = vec![ViewData::new_playlist(0, String::new())];
        p.nav_history_pos = 0;
        p
    }

    #[test]
    fn stamped_ids_resolve_to_their_own_slot() {
        let mut p = player();
        let first = p.request_ids.next();
        p.view_data_mut().request_id = first;

        p.handle_navigate_to(ViewData::new_playlist(1, "Other".into()));
        let second = p.request_ids.next();
        p.view_data_mut().request_id = second;

        assert_ne!(p.slot_for_request(first), p.slot_for_request(second));
        assert_eq!(p.slot_for_request(second), Some(p.nav_history_pos));
    }

    #[test]
    fn navigate_back_restores_outgoing_view() {
        let mut p = player();
        // Default view is Search. Navigate to a Playlist as the outgoing view.
        p.handle_navigate_to(ViewData::new_playlist(2, "My List".into()));
        assert_eq!(p.nav_history.len(), 2);
        assert!(p.can_navigate_back());

        // Navigate to a Search view (simulates `run_search` pushing a slot).
        p.handle_navigate_to(ViewData::new_search(
            "song".into(),
            ProviderId::YouTube,
            SearchScope::Songs,
        ));
        assert_eq!(p.nav_history.len(), 3);
        assert!(matches!(p.view_data().kind, ViewKind::Search { .. }));

        // Back must restore the Playlist(2) view without clobbering it.
        let _ = p.handle_navigate_back();
        assert!(matches!(p.view_data().kind, ViewKind::Playlist(_)));
        let active = match &p.view_data().kind {
            ViewKind::Playlist(entry) => entry.index,
            _ => unreachable!(),
        };
        assert_eq!(active, 2);
        // The outgoing entry is preserved as a distinct history slot.
        assert_eq!(p.nav_history.len(), 3);
    }

    #[test]
    fn replacing_view_keeps_outgoing_slot() {
        let mut p = player();
        p.handle_navigate_to(ViewData::new_playlist(0, "A".into()));
        p.handle_navigate_to(ViewData::new_playlist(1, "B".into()));
        // Three slots: initial Playlist (from PlaylistStore), Playlist(0), Playlist(1).
        assert_eq!(p.nav_history.len(), 3);
        let _ = p.handle_navigate_back();
        assert_eq!(
            match &p.view_data().kind {
                ViewKind::Playlist(e) => Some(e.index),
                _ => None,
            },
            Some(0)
        );
        let _ = p.handle_navigate_back();
        // Returns to the initial (pre-navigation) view, not clobbered.
        assert!(matches!(p.view_data().kind, ViewKind::Playlist(_)));
        assert_eq!(p.nav_history.len(), 3);
    }

    #[test]
    fn search_results_land_in_requesting_slot_not_active() {
        let mut p = player();
        // Replicates `run_search`'s slot stamping; the threaded version can't
        // run here.
        p.handle_navigate_to(ViewData::new_search(
            "song".into(),
            ProviderId::YouTube,
            SearchScope::Songs,
        ));
        let rid = p.request_ids.next();
        p.view_data_mut().request_id = rid;

        // Navigate away to a different view before results arrive.
        p.handle_navigate_to(ViewData::new_playlist(5, "Other".into()));
        assert!(matches!(p.view_data().kind, ViewKind::Playlist(_)));
        assert_eq!(p.view_data().request_id, 0);

        // Deliver the search results (simulating the background thread).
        let mut providers = std::collections::HashMap::new();
        providers.insert(
            crate::providers::ProviderId::YouTube,
            crate::types::ProviderTrack {
                id: "t1".into(),
                url: String::new(),
                artist_id: None,
                duration: 0,
                thumbnail: String::new(),
                album: None,
                play_count: 0,
            },
        );
        let track = crate::types::Track {
            title: "Song".into(),
            artist: "Artist".into(),
            download_path: None,
            source: crate::providers::ProviderId::YouTube,
            providers,
        };
        p.process_result(BackendResult::SearchResults(
            rid,
            vec![track],
            crate::providers::SearchTab::Songs,
        ));

        // The active (Playlist) slot must be untouched.
        assert!(matches!(p.view_data().kind, ViewKind::Playlist(_)));
        assert!(p.view_data().tracks().is_empty());

        // Going back to the search slot shows the delivered results.
        let _ = p.handle_navigate_back();
        assert!(matches!(p.view_data().kind, ViewKind::Search { .. }));
        assert_eq!(p.view_data().tracks().len(), 1);
        assert_eq!(
            p.view_data().request_id,
            0,
            "request id cleared after delivery"
        );
    }

    #[test]
    fn reveal_now_playing_navigates_to_origin_and_focuses_track() {
        let mut p = player();
        let track = crate::types::Track::from_provider(
            ProviderId::YouTube,
            "id1".into(),
            "https://example.com".into(),
            "Song",
            "Artist",
            180,
            "",
            None,
            None,
        );
        let mut origin = ViewData::new_radio(ViewKind::SongRadio("Radio".into()));
        origin.set_tracks(vec![track.clone()]);
        p.now_playing_from = Some(origin);
        p.queue = crate::types::PlayQueue::new();
        p.queue.enqueue(track);

        // The live view is a Playlist, so reveal must navigate first.
        assert!(matches!(p.view_data().kind, ViewKind::Playlist(_)));
        let _ = p.handle_reveal_now_playing();

        assert!(matches!(p.view_data().kind, ViewKind::SongRadio(_)));
        assert_eq!(
            p.drag.hovered_track(),
            Some(TrackPos::new(0, TrackListKind::Active))
        );
    }
}
