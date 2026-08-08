use super::{Message, MusicPlayer, NavEntry, Task, ViewData};
use crate::app::ui::TRACK_LIST_ID;
use crate::app::ViewKind;

impl MusicPlayer {
    pub const fn can_navigate_back(&self) -> bool {
        self.nav_history_pos > 0
    }

    pub const fn can_navigate_forward(&self) -> bool {
        self.nav_history_pos + 1 < self.nav_history.len()
    }

    /// Snapshot the current view data by cloning it into the nav entry.
    pub(super) fn snapshot_current(&self) -> ViewData {
        self.view_data.clone()
    }

    /// Sync the global `search_query` from the active `Search` view's stored
    /// query. Used whenever `view_data` is replaced (navigate / restore) so the
    /// always-visible search bar reflects the view being shown.
    fn sync_search_query(&mut self) {
        self.search_query = self.view_data.search_query().to_string();
    }

    pub(super) fn restore_nav_entry(&mut self, entry: &NavEntry) -> Task<Message> {
        // Scroll position is stored relative to the main track_list scrollable.
        // (Queue view uses a different Id and is not navigated via history.)
        let y = entry.data.scroll;
        self.view_data = entry.data.clone();
        self.sync_search_query();

        iced::widget::operation::scroll_to::<Message>(
            TRACK_LIST_ID,
            iced::widget::operation::AbsoluteOffset { x: 0.0, y },
        )
    }

    pub fn handle_navigate_to(&mut self, data: ViewData) {
        if self.view_data.same_kind(&data) {
            return;
        }
        // Capture the live query into the outgoing `Search` entry (if any) so
        // Back navigation restores it.
        if let ViewKind::Search { query, .. } = &mut self.view_data.kind {
            query.clone_from(&self.search_query);
        }
        self.nav_history.truncate(self.nav_history_pos + 1);
        self.cleanup_drag_state();
        self.drag.hovered_track = None;

        self.view_data = data;
        self.sync_search_query();

        // Push the new state as a single entry. The previous entry (preserved
        // by truncate) already serves as the back-target for Back navigation.
        self.nav_history.push(NavEntry {
            data: self.view_data.clone(),
        });

        if self.nav_history.len() > 20 {
            self.nav_history.remove(0);
        }
        self.nav_history_pos = self.nav_history.len() - 1;

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

    pub(super) fn push_nav_entry(&mut self) {
        self.nav_history.truncate(self.nav_history_pos + 1);
        self.nav_history.push(NavEntry {
            data: self.snapshot_current(),
        });
        if self.nav_history.len() > 20 {
            self.nav_history.remove(0);
        }
        self.nav_history_pos = self.nav_history.len() - 1;
    }

    /// Updates the current nav entry in-place with the current state if it
    /// matches the current view. Returns true if updated, false if the current
    /// entry doesn't match the current view.
    pub(super) fn update_current_snapshot(&mut self) -> bool {
        let pos = self.nav_history_pos;
        if !self
            .nav_history
            .get(pos)
            .is_some_and(|e| e.data.same_kind(&self.view_data))
        {
            return false;
        }
        // Build a fresh snapshot (clone of the live data), then replace the
        // entry's data.
        let snapshot = self.snapshot_current();
        if let Some(entry) = self.nav_history.get_mut(pos) {
            entry.data = snapshot;
            true
        } else {
            false
        }
    }
}
