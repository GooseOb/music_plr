use super::{Message, MusicPlayer, Task, ViewData};
use crate::app::{
    interaction::{TrackListKind, TrackPos},
    pane::{PaneId, SplitDir, MAX_PANES},
    ui::track_list_id,
    ViewKind,
};

impl MusicPlayer {
    pub fn can_navigate_back(&self, pane: PaneId) -> bool {
        self.pane(pane).can_navigate_back()
    }

    pub fn can_navigate_forward(&self, pane: PaneId) -> bool {
        self.pane(pane).can_navigate_forward()
    }

    fn sync_search_query(&mut self, pane: PaneId) {
        if let ViewKind::Search(s) = &self.view_data_in(pane).kind {
            let query = s.query.clone();
            self.pane_mut(pane).search_query = query;
        }
    }

    pub(super) fn sync_search_scope(&mut self, pane: PaneId) {
        if let ViewKind::Search(s) = &self.view_data_in(pane).kind {
            let scope = s.tab.scope();
            self.pane_mut(pane).search_scope = scope;
        }
    }

    pub(super) fn sync_search_provider(&mut self, pane: PaneId) {
        if let ViewKind::Search(s) = &self.view_data_in(pane).kind {
            let provider = s.provider;
            self.pane_mut(pane).search_provider = provider;
        }
    }

    pub(super) fn restore_nav_entry(&mut self, pane: PaneId, data: ViewData) -> Task<Message> {
        // Scroll position is stored relative to the main track_list scrollable.
        // (Queue view uses a different Id and is not navigated via history.)
        let y = data.scroll;
        *self.view_data_in_mut(pane) = data;
        self.sync_search_query(pane);
        self.sync_search_scope(pane);
        self.sync_search_provider(pane);
        self.sync_downloads_view(pane);

        iced::widget::operation::scroll_to::<Message>(
            track_list_id(pane),
            iced::widget::operation::AbsoluteOffset { x: 0.0, y },
        )
    }

    pub(super) fn sync_downloads_view(&mut self, pane: PaneId) {
        if matches!(self.view_data_in(pane).kind, ViewKind::Downloads) {
            let tracks = self.download_registry.clone_tracks();
            self.view_data_in_mut(pane).set_tracks(tracks);
        }
    }

    /// Replace the current view, recording the destination as a *new* history
    /// slot while leaving `nav_history[pos]` (the outgoing view) intact. This
    /// mirrors the old design where `view_data` was a separate field from the
    /// history clone: overwriting the live slot must not clobber the entry we
    /// can navigate Back to.
    pub(super) fn push_new_view(&mut self, pane: PaneId, data: ViewData) -> Task<Message> {
        // Showing content in a pane focuses it (click-to-focus for drill-downs,
        // search, radio, and playlist jumps).
        self.focused_pane_id = pane;
        let p = self.pane_mut(pane);
        p.nav_history.truncate(p.nav_history_pos + 1);
        p.nav_history.push(data);
        if p.nav_history.len() > 20 {
            p.nav_history.remove(0);
        }
        p.nav_history_pos = p.nav_history.len() - 1;
        self.capture_bounds_task()
    }

    pub fn handle_navigate_to(&mut self, pane: PaneId, data: ViewData) -> Task<Message> {
        if self.view_data_in(pane).same_kind(&data) {
            return Task::none();
        }
        // Capture the live query into the outgoing `Search` entry (if any) so
        // Back navigation restores it.
        let live_query = self.pane(pane).search_query.clone();
        if let ViewKind::Search(s) = &mut self.view_data_in_mut(pane).kind {
            s.query = live_query;
        }
        self.drag.cleanup();

        // Push the destination as a fresh slot; the outgoing view stays at the
        // previous position.
        let nav_task = self.push_new_view(pane, data);
        self.sync_search_query(pane);
        self.sync_search_scope(pane);
        self.sync_search_provider(pane);
        self.sync_downloads_view(pane);

        let view = self.view_data_in(pane).clone();
        self.seed_view_thumbnails(&view);
        self.save_session();
        nav_task
    }

    pub fn handle_navigate_back(&mut self, pane: PaneId) -> Task<Message> {
        if self.pane(pane).can_navigate_back() {
            self.pane_mut(pane).nav_history_pos -= 1;
            return self.sync_navigation(pane);
        }
        Task::none()
    }

    pub fn handle_navigate_forward(&mut self, pane: PaneId) -> Task<Message> {
        if self.pane(pane).can_navigate_forward() {
            self.pane_mut(pane).nav_history_pos += 1;
            return self.sync_navigation(pane);
        }
        Task::none()
    }

    fn sync_navigation(&mut self, pane: PaneId) -> Task<Message> {
        let entry = self.pane(pane).nav_history[self.pane(pane).nav_history_pos].clone();
        let task = self.restore_nav_entry(pane, entry);
        self.save_session();
        task.chain(self.capture_bounds_task())
    }

    pub fn handle_reveal_now_playing(&mut self) -> Task<Message> {
        let pane = self.focused_pane_id;
        let Some(mut origin) = self.now_playing_from.clone() else {
            return Task::none();
        };
        let Some(track) = self.queue.current().cloned() else {
            return Task::none();
        };
        // Zero the id so an in-flight response for the live view can never
        // be routed into the restored snapshot.
        origin.request_id = 0;
        let nav_task = self.handle_navigate_to(pane, origin);
        let key = track.cache_key();
        let index = self
            .view_tracks_in(pane)
            .iter()
            .position(|t| t.cache_key() == key);
        let Some(index) = index else {
            return nav_task;
        };
        nav_task.chain(self.move_hovered(TrackPos::new(index, TrackListKind::Active, pane)))
    }

    pub(super) fn slot_for_request(&self, rid: u64) -> Option<(PaneId, usize)> {
        for (id, pane) in &self.panes {
            if let Some(idx) = pane.nav_history.iter().position(|v| v.request_id == rid) {
                return Some((*id, idx));
            }
        }
        None
    }

    /// Split `pane` into two independently navigable panes, duplicating its
    /// current state (navigation history fork). `SplitDir::Horizontal` places
    /// the new pane to the right, `SplitDir::Vertical` stacks it below. Focus
    /// moves to the new pane. The fork keeps the lyrics overlay open; transient
    /// dropdown state starts closed.
    pub fn split_pane(&mut self, pane: PaneId, dir: SplitDir) -> Task<Message> {
        if !self.split_root.contains(pane) {
            return Task::none();
        }
        if self.split_root.leaf_count() >= MAX_PANES {
            self.notify(self.strings.max_panes_reached);
            return Task::none();
        }
        let Some(source) = self.panes.get(&pane).cloned() else {
            return Task::none();
        };
        let new_id = self.next_pane_id;
        self.next_pane_id += 1;
        let scroll_y = source.view_data().scroll;
        let mut fork = source;
        fork.id = new_id;
        fork.show_search_history = false;
        fork.last_filtered_history = Vec::new();
        self.panes.insert(new_id, fork);
        self.split_root.split_leaf(pane, new_id, dir);
        self.focused_pane_id = new_id;
        self.save_session();
        // Splitting reveals the pane header, which reshapes the widget tree
        // and drops the scrollables' offsets — capture the new geometry first,
        // then restore both panes' positions in order.
        let capture: Task<Message> = self.capture_bounds_task();
        capture
            .chain(iced::widget::operation::scroll_to::<Message>(
                track_list_id(pane),
                iced::widget::operation::AbsoluteOffset {
                    x: 0.0,
                    y: scroll_y,
                },
            ))
            .chain(iced::widget::operation::scroll_to::<Message>(
                track_list_id(new_id),
                iced::widget::operation::AbsoluteOffset {
                    x: 0.0,
                    y: scroll_y,
                },
            ))
    }

    pub fn close_pane(&mut self, pane: PaneId) -> Task<Message> {
        if self.split_root.leaf_count() <= 1 {
            return Task::none();
        }
        // A dialog anchored to the closing pane (context menu, add-to-playlist
        // picker, track editor) must go with it; its position would otherwise
        // resolve against a missing pane.
        let dialog_pane = match &self.dialog {
            Some(crate::app::Dialog::ContextMenu(m)) => Some(m.pos.pane),
            Some(crate::app::Dialog::Picker(p)) => Some(p.pane),
            Some(crate::app::Dialog::Edit(e)) => Some(e.pos.pane),
            _ => None,
        };
        if dialog_pane == Some(pane) {
            self.dialog = None;
        }
        if self
            .drag
            .hovered_track()
            .is_some_and(|pos| pos.list.is_main() && pos.pane == pane)
        {
            self.drag.hovered = None;
        }
        if self
            .track_list_search
            .as_ref()
            .is_some_and(|fs| fs.pane == pane)
        {
            self.track_list_search = None;
        }
        self.bounds.tracks.remove(&pane);
        self.bounds.search_history.remove(&pane);
        self.bounds.search_inputs.remove(&pane);
        self.drag.forget_pane(pane);
        let survivor = self.split_root.remove_leaf(pane);
        self.panes.remove(&pane);
        if let Some(survivor) = survivor {
            if self.focused_pane_id == pane || !self.panes.contains_key(&self.focused_pane_id) {
                self.focused_pane_id = survivor;
            }
        }
        self.save_session();
        // Closing reflows the layout, which can drop the scrollables'
        // offsets — capture the new geometry first, then restore the focused
        // pane's position.
        let focused = self.focused_pane_id;
        let scroll_y = self
            .panes
            .get(&focused)
            .map_or(0.0, |p| p.view_data().scroll);
        let capture: Task<Message> = self.capture_bounds_task();
        capture.chain(iced::widget::operation::scroll_to::<Message>(
            track_list_id(focused),
            iced::widget::operation::AbsoluteOffset {
                x: 0.0,
                y: scroll_y,
            },
        ))
    }

    pub fn focus_pane(&mut self, pane: PaneId) {
        if self.focused_pane_id == pane {
            return;
        }
        if self.panes.contains_key(&pane) {
            self.focused_pane_id = pane;
            self.save_session();
        }
    }

    /// Move keyboard focus to the pane adjacent to the focused one in `dir`,
    /// wrapping to the far edge past the last pane (like track navigation
    /// wraps at list ends).
    pub fn focus_neighbor(&mut self, dir: crate::app::pane::PaneDir) {
        let next = self
            .split_root
            .neighbor(self.focused_pane_id, dir)
            .unwrap_or_else(|| self.split_root.wrap_edge(self.focused_pane_id, dir));
        self.focus_pane(next);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{message::BackendResult, ViewKind},
        data::config,
        providers::{ProviderId, SearchScope},
    };

    fn player() -> MusicPlayer {
        // `new_with` inits media controls (spawns a thread, no-ops if D-Bus is absent),
        // so it is safe to construct headlessly in tests. The nav history is
        // reset to a deterministic Playlist view so the navigation tests
        // don't depend on on-disk session state.
        let mut p = MusicPlayer::new_with(config::Config::default());
        p.reset_test_pane(vec![ViewData::new_playlist(0, String::new())]);
        p
    }

    #[test]
    fn sidebar_search_restores_last_search_view() {
        let mut p = player();
        let mut last = ViewData::new_search("foo".into(), ProviderId::YouTube, SearchScope::Songs);
        last.set_tracks(vec![crate::types::Track::from_provider(
            ProviderId::YouTube,
            "id1".into(),
            "https://example.com".into(),
            "Song",
            "Artist",
            180,
            "",
            None,
            None,
        )]);
        p.last_search_view = Some(last.clone());
        let pane = p.focused_pane_id;
        let _ = p.handle_navigate_to(pane, ViewData::new_playlist(3, "List".into()));

        // Sidebar Search restores the last search view as a fresh history
        // slot, syncs the search bar, and keeps Back working.
        let _ = p.handle_sidebar_search();
        assert!(matches!(p.view_data().kind, ViewKind::Search { .. }));
        assert_eq!(p.view_data().tracks().len(), 1);
        assert_eq!(p.pane(pane).search_query, "foo");

        // Clicking again while the search view is active is a no-op.
        let _ = p.handle_sidebar_search();
        assert_eq!(p.pane(pane).nav_history.len(), 3);

        // Back returns to the playlist that was active before.
        let _ = p.handle_navigate_back(pane);
        assert!(matches!(p.view_data().kind, ViewKind::Playlist(_)));
    }

    #[test]
    fn stamped_ids_resolve_to_their_own_slot() {
        let mut p = player();
        let first = p.request_ids.next();
        p.view_data_mut().request_id = first;

        let pane = p.focused_pane_id;
        let _ = p.handle_navigate_to(pane, ViewData::new_playlist(1, "Other".into()));
        let second = p.request_ids.next();
        p.view_data_mut().request_id = second;

        assert_ne!(p.slot_for_request(first), p.slot_for_request(second));
        assert_eq!(
            p.slot_for_request(second),
            Some((pane, p.pane(pane).nav_history_pos))
        );
    }

    #[test]
    fn navigate_back_restores_outgoing_view() {
        let mut p = player();
        // Default view is Search. Navigate to a Playlist as the outgoing view.
        let pane = p.focused_pane_id;
        let _ = p.handle_navigate_to(pane, ViewData::new_playlist(2, "My List".into()));
        assert_eq!(p.pane(pane).nav_history.len(), 2);
        assert!(p.can_navigate_back(pane));

        // Navigate to a Search view (simulates `run_search` pushing a slot).
        let _ = p.handle_navigate_to(
            pane,
            ViewData::new_search("song".into(), ProviderId::YouTube, SearchScope::Songs),
        );
        assert_eq!(p.pane(pane).nav_history.len(), 3);
        assert!(matches!(p.view_data().kind, ViewKind::Search { .. }));

        // Back must restore the Playlist(2) view without clobbering it.
        let _ = p.handle_navigate_back(pane);
        assert!(matches!(p.view_data().kind, ViewKind::Playlist(_)));
        let active = match &p.view_data().kind {
            ViewKind::Playlist(entry) => entry.index,
            _ => unreachable!(),
        };
        assert_eq!(active, 2);
        // The outgoing entry is preserved as a distinct history slot.
        assert_eq!(p.pane(pane).nav_history.len(), 3);
    }

    #[test]
    fn replacing_view_keeps_outgoing_slot() {
        let mut p = player();
        let pane = p.focused_pane_id;
        let _ = p.handle_navigate_to(pane, ViewData::new_playlist(0, "A".into()));
        let _ = p.handle_navigate_to(pane, ViewData::new_playlist(1, "B".into()));
        // Three slots: initial Playlist (from PlaylistStore), Playlist(0), Playlist(1).
        assert_eq!(p.pane(pane).nav_history.len(), 3);
        let _ = p.handle_navigate_back(pane);
        assert_eq!(
            match &p.view_data().kind {
                ViewKind::Playlist(e) => Some(e.index),
                _ => None,
            },
            Some(0)
        );
        let _ = p.handle_navigate_back(pane);
        // Returns to the initial (pre-navigation) view, not clobbered.
        assert!(matches!(p.view_data().kind, ViewKind::Playlist(_)));
        assert_eq!(p.pane(pane).nav_history.len(), 3);
    }

    #[test]
    fn search_results_land_in_requesting_slot_not_active() {
        let mut p = player();
        // Replicates `run_search`'s slot stamping; the threaded version can't
        // run here.
        let pane = p.focused_pane_id;
        let _ = p.handle_navigate_to(
            pane,
            ViewData::new_search("song".into(), ProviderId::YouTube, SearchScope::Songs),
        );
        let rid = p.request_ids.next();
        p.view_data_mut().request_id = rid;

        // Navigate away to a different view before results arrive.
        let _ = p.handle_navigate_to(pane, ViewData::new_playlist(5, "Other".into()));
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
            source: crate::providers::ProviderId::YouTube,
            providers,
        };
        let _ = p.process_result(BackendResult::SearchResults(
            rid,
            vec![track],
            crate::providers::SearchTab::Songs,
        ));

        // The active (Playlist) slot must be untouched.
        assert!(matches!(p.view_data().kind, ViewKind::Playlist(_)));
        assert!(p.view_data().tracks().is_empty());

        // Going back to the search slot shows the delivered results.
        let _ = p.handle_navigate_back(pane);
        assert!(matches!(p.view_data().kind, ViewKind::Search { .. }));
        assert_eq!(p.view_data().tracks().len(), 1);
        assert_eq!(
            p.view_data().request_id,
            0,
            "request id cleared after delivery"
        );
    }

    #[test]
    fn split_forks_history_and_navigates_independently() {
        let mut p = player();
        let pane = p.focused_pane_id;
        let _ = p.handle_navigate_to(pane, ViewData::new_playlist(2, "My List".into()));

        let _ = p.split_pane(pane, SplitDir::Horizontal);
        let fork = p.focused_pane_id;
        assert_ne!(pane, fork);
        assert_eq!(p.split_root.leaf_count(), 2);
        assert!(matches!(p.view_data_in(fork).kind, ViewKind::Playlist(_)));

        let _ = p.handle_navigate_to(fork, ViewData::new_playlist(5, "Other".into()));
        assert!(matches!(p.view_data_in(pane).kind, ViewKind::Playlist(_)));
        assert_eq!(
            match &p.view_data_in(pane).kind {
                ViewKind::Playlist(e) => e.index,
                _ => unreachable!(),
            },
            2
        );

        let _ = p.handle_navigate_back(fork);
        assert!(matches!(p.view_data_in(fork).kind, ViewKind::Playlist(_)));
        let _ = p.close_pane(fork);
        assert_eq!(p.split_root.leaf_count(), 1);
        assert_eq!(p.focused_pane_id, pane);
        assert!(matches!(p.view_data().kind, ViewKind::Playlist(_)));
    }

    #[test]
    fn focus_wraps_past_edge() {
        use crate::app::pane::PaneDir;
        let mut p = player();
        let pane = p.focused_pane_id;
        let _ = p.split_pane(pane, SplitDir::Horizontal);
        let fork = p.focused_pane_id;
        p.focus_neighbor(PaneDir::Right);
        assert_eq!(p.focused_pane_id, pane);
        p.focus_neighbor(PaneDir::Left);
        assert_eq!(p.focused_pane_id, fork);
        p.focus_neighbor(PaneDir::Left);
        assert_eq!(p.focused_pane_id, pane);

        // Stack the left pane: vertical wraps hold the column.
        let _ = p.split_pane(pane, SplitDir::Vertical);
        let bottom = p.focused_pane_id;
        p.focus_neighbor(PaneDir::Down);
        assert_eq!(p.focused_pane_id, pane);
        p.focus_neighbor(PaneDir::Up);
        assert_eq!(p.focused_pane_id, bottom);
    }

    #[test]
    fn split_cap_and_last_pane_guard() {
        let mut p = player();
        for _ in 0..10 {
            let pane = p.focused_pane_id;
            let _ = p.split_pane(pane, SplitDir::Horizontal);
        }
        assert_eq!(p.split_root.leaf_count(), crate::app::pane::MAX_PANES);
        let only = p.focused_pane_id;
        while p.split_root.leaf_count() > 1 {
            let pane = p.focused_pane_id;
            let _ = p.close_pane(pane);
        }
        assert_eq!(p.split_root.leaf_count(), 1);
        let last = p.focused_pane_id;
        let _ = p.close_pane(last);
        assert_eq!(p.split_root.leaf_count(), 1);
        assert!(p.panes.contains_key(&last));
        let _ = only;
    }

    #[test]
    fn lyrics_fetch_applies_only_to_waiting_provider_pane() {
        use crate::{app::LyricsState, load_state::LoadState, lyrics::LyricsProvider as LP};
        let mut p = player();
        let pane = p.focused_pane_id;
        let _ = p.split_pane(pane, SplitDir::Horizontal);
        let fork = p.focused_pane_id;
        p.pane_mut(pane).lyrics = Some(LyricsState::new(LP::LrcLib));
        p.pane_mut(fork).lyrics = Some(LyricsState::new(LP::LrcMux));
        for id in [pane, fork] {
            p.pane_mut(id).lyrics.as_mut().unwrap().track_id = Some("track1".into());
        }
        let lyrics = crate::lyrics::Lyrics {
            timed: vec![],
            plain: "la".into(),
            provider: LP::LrcLib,
        };
        let _ = p.process_result(BackendResult::LyricsFetched(
            Ok(lyrics),
            "track1".into(),
            LP::LrcLib,
        ));
        assert!(matches!(
            p.pane(pane).lyrics.as_ref().unwrap().lyrics,
            LoadState::Ready(_)
        ));
        assert!(matches!(
            p.pane(fork).lyrics.as_ref().unwrap().lyrics,
            LoadState::Loading
        ));
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
        p.queue.set_queue(vec![track], 1000);

        // The live view is a Playlist, so reveal must navigate first.
        assert!(matches!(p.view_data().kind, ViewKind::Playlist(_)));
        let _ = p.handle_reveal_now_playing();

        assert!(matches!(p.view_data().kind, ViewKind::SongRadio(_)));
        assert_eq!(
            p.drag.hovered_track(),
            Some(TrackPos::new(0, TrackListKind::Active, p.focused_pane_id))
        );
    }
}
