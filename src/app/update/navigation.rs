use super::{Message, MusicPlayer, Task, ViewData};
use crate::app::ui::TRACK_LIST_ID;
use crate::app::ViewKind;

impl MusicPlayer {
    pub const fn can_navigate_back(&self) -> bool {
        self.nav_history_pos > 0
    }

    pub const fn can_navigate_forward(&self) -> bool {
        self.nav_history_pos + 1 < self.nav_history.len()
    }

    /// Snapshot the current view data by cloning the active history slot.
    pub(super) fn snapshot_current(&self) -> ViewData {
        self.view_data().clone()
    }

    /// Sync the global `search_query` from the active `Search` view's stored
    /// query. Used whenever the active view is replaced (navigate / restore)
    /// so the always-visible search bar reflects the view being shown.
    fn sync_search_query(&mut self) {
        self.search_query = self.view_data().search_query().to_string();
    }

    /// Sync the global `search_scope` from the active `Search` view's tab. The
    /// scope selector (and `run_search`) are driven by `search_scope`, so this
    /// keeps the tab highlight correct when navigating to / between / back to
    /// Search views whose tab differs from the previously selected scope.
    pub(super) fn sync_search_scope(&mut self) {
        if let ViewKind::Search { tab, .. } = &self.view_data().kind {
            self.search_scope = tab.scope();
        }
    }

    pub(super) fn restore_nav_entry(&mut self, data: &ViewData) -> Task<Message> {
        // Scroll position is stored relative to the main track_list scrollable.
        // (Queue view uses a different Id and is not navigated via history.)
        let y = data.scroll;
        *self.view_data_mut() = data.clone();
        self.sync_search_query();
        self.sync_search_scope();

        iced::widget::operation::scroll_to::<Message>(
            TRACK_LIST_ID,
            iced::widget::operation::AbsoluteOffset { x: 0.0, y },
        )
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
        if let ViewKind::Search { query, .. } = &mut self.view_data_mut().kind {
            *query = live_query;
        }
        self.cleanup_drag_state();
        self.drag.hovered_track = None;

        // Push the destination as a fresh slot; the outgoing view stays at the
        // previous position.
        self.push_new_view(data);
        self.sync_search_query();
        self.sync_search_scope();

        let view = self.view_data().clone();
        self.seed_view_thumbnails(&view);
        self.save_session();
    }

    pub fn handle_navigate_back(&mut self) -> Task<Message> {
        if self.nav_history_pos > 0 {
            self.nav_history_pos -= 1;
            let entry = self.nav_history[self.nav_history_pos].clone();
            let task = self.restore_nav_entry(&entry);
            self.save_session();
            task
        } else {
            Task::none()
        }
    }

    pub fn handle_navigate_forward(&mut self) -> Task<Message> {
        if self.nav_history_pos + 1 < self.nav_history.len() {
            self.nav_history_pos += 1;
            let entry = self.nav_history[self.nav_history_pos].clone();
            let task = self.restore_nav_entry(&entry);
            self.save_session();
            task
        } else {
            Task::none()
        }
    }

    /// Find the history slot awaiting the given request id. Returns its index,
    /// or `None` if the slot was truncated away by navigation (stale result).
    pub(super) fn slot_for_request(&self, rid: u64) -> Option<usize> {
        self.nav_history.iter().position(|v| v.request_id == rid)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{message::BackendResult, ViewKind};
    use crate::data::config;
    use crate::youtube::SearchScope;

    fn player() -> MusicPlayer {
        // `new_with` inits MPRIS (spawns a thread, no-ops if D-Bus is absent),
        // so it is safe to construct headlessly in tests.
        MusicPlayer::new_with(config::load_config())
    }

    #[test]
    fn navigate_back_restores_outgoing_view() {
        let mut p = player();
        // Default view is Search. Navigate to a Playlist as the outgoing view.
        p.handle_navigate_to(ViewData::new_playlist(Some(2), "My List".into(), None));
        assert_eq!(p.nav_history.len(), 2);
        assert!(p.can_navigate_back());

        // Navigate to a Search view (simulates `run_search` pushing a slot).
        p.handle_navigate_to(ViewData::new_search("song".into(), SearchScope::Songs));
        assert_eq!(p.nav_history.len(), 3);
        assert!(matches!(p.view_data().kind, ViewKind::Search { .. }));

        // Back must restore the Playlist(2) view without clobbering it.
        let _ = p.handle_navigate_back();
        assert!(matches!(p.view_data().kind, ViewKind::Playlist { .. }));
        assert_eq!(p.view_data().selected_playlist_id(), Some(2));
        // The outgoing entry is preserved as a distinct history slot.
        assert_eq!(p.nav_history.len(), 3);
    }

    #[test]
    fn replacing_view_keeps_outgoing_slot() {
        let mut p = player();
        p.handle_navigate_to(ViewData::new_playlist(Some(0), "A".into(), None));
        p.handle_navigate_to(ViewData::new_playlist(Some(1), "B".into(), None));
        // Three slots: initial Playlist (from PlaylistStore), Playlist(0), Playlist(1).
        assert_eq!(p.nav_history.len(), 3);
        let _ = p.handle_navigate_back();
        assert_eq!(p.view_data().selected_playlist_id(), Some(0));
        let _ = p.handle_navigate_back();
        // Returns to the initial (pre-navigation) view, not clobbered.
        assert!(matches!(p.view_data().kind, ViewKind::Playlist { .. }));
        assert_eq!(p.nav_history.len(), 3);
    }

    #[test]
    fn search_results_land_in_requesting_slot_not_active() {
        let mut p = player();
        // Issue a search (pushes a Search slot). `run_search` stamps the slot
        // with a request id before spawning the bg thread; replicate that here
        // (the test can't use the threaded `run_search`) to exercise the
        // request-id targeting in `process_result`.
        p.handle_navigate_to(ViewData::new_search("song".into(), SearchScope::Songs));
        let rid = 7;
        p.view_data_mut().request_id = rid;

        // Navigate away to a different view before results arrive.
        p.handle_navigate_to(ViewData::new_playlist(Some(5), "Other".into(), None));
        assert!(matches!(p.view_data().kind, ViewKind::Playlist { .. }));
        assert_eq!(p.view_data().request_id, 0);

        // Deliver the search results (simulating the background thread).
        let track = crate::types::Track {
            id: "t1".into(),
            title: "Song".into(),
            artist: "Artist".into(),
            duration: 0,
            url: String::new(),
            source: crate::types::TrackSource::YouTube,
            thumbnail: String::new(),
            download_path: None,
        };
        p.process_result(BackendResult::SearchResults(
            rid,
            vec![track],
            crate::youtube::SearchTab::Songs,
        ));

        // The active (Playlist) slot must be untouched.
        assert!(matches!(p.view_data().kind, ViewKind::Playlist { .. }));
        assert!(p.view_data().tracks.is_empty());

        // Going back to the search slot shows the delivered results.
        let _ = p.handle_navigate_back();
        assert!(matches!(p.view_data().kind, ViewKind::Search { .. }));
        assert_eq!(p.view_data().tracks.len(), 1);
        assert_eq!(
            p.view_data().request_id,
            0,
            "request id cleared after delivery"
        );
    }
}
