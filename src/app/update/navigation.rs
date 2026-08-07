use super::{Message, MusicPlayer, NavEntry, Task, View, ViewSnapshot};
use crate::app::ui::TRACK_LIST_ID;

impl MusicPlayer {
    pub const fn can_navigate_back(&self) -> bool {
        self.nav_history_pos > 0
    }

    pub const fn can_navigate_forward(&self) -> bool {
        self.nav_history_pos + 1 < self.nav_history.len()
    }

    pub(super) fn snapshot_current(&self) -> ViewSnapshot {
        ViewSnapshot::from_player(self)
    }

    pub(super) fn restore_nav_entry(&mut self, entry: &NavEntry) -> Task<Message> {
        self.current_view = entry.view.clone();
        entry.snapshot.apply_to(self);

        // Scroll position is stored relative to the main track_list scrollable.
        // (Queue view uses a different Id and is not navigated via history.)
        iced::widget::operation::scroll_to::<Message>(
            TRACK_LIST_ID,
            iced::widget::operation::AbsoluteOffset {
                x: 0.0,
                y: entry.snapshot.scroll(),
            },
        )
    }

    pub fn handle_navigate_to(&mut self, view: View) {
        if self.current_view == view {
            return;
        }
        self.nav_history.truncate(self.nav_history_pos + 1);
        self.show_search_history = false;
        self.cleanup_drag_state();
        self.drag.hovered_track = None;

        self.current_view = view;
        self.selected_indices.clear();

        if self.current_view == View::Downloads {
            self.selected_playlist = None;
        }

        // Push the new state as a single entry. The previous entry (preserved
        // by truncate) already serves as the back-target for Back navigation.
        self.nav_history.push(NavEntry {
            view: self.current_view.clone(),
            snapshot: self.snapshot_current(),
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
            view: self.current_view.clone(),
            snapshot: self.snapshot_current(),
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
            .is_some_and(|e| e.view == self.current_view)
        {
            return false;
        }
        // Build a fresh snapshot (immutable borrow released before the
        // mutable borrow below), then replace the entry's snapshot.
        let snapshot = self.snapshot_current();
        if let Some(entry) = self.nav_history.get_mut(pos) {
            entry.snapshot = snapshot;
            true
        } else {
            false
        }
    }
}

impl ViewSnapshot {
    pub(super) const fn scroll(&self) -> f32 {
        match self {
            Self::Search { scroll, .. }
            | Self::Radio { scroll, .. }
            | Self::TrackList { scroll, .. } => *scroll,
        }
    }

    pub(super) fn from_player(player: &MusicPlayer) -> Self {
        let scroll = player.get_current_list_scroll();
        match player.current_view {
            View::Search => ViewSnapshot::Search {
                query: player.search_query.clone(),
                results: player.search_results.clone(),
                selection: player.selected_indices.clone(),
                scroll,
            },
            View::SongRadio | View::ArtistRadio => ViewSnapshot::Radio {
                label: player.radio_label.clone(),
                tracks: player.radio_tracks.clone(),
                selection: player.selected_indices.clone(),
                scroll,
            },
            View::Playlist | View::Downloads => ViewSnapshot::TrackList {
                playlist: player.selected_playlist,
                playlist_name: player.selected_playlist_name.clone(),
                selection: player.selected_indices.clone(),
                scroll,
            },
        }
    }

    pub(super) fn apply_to(&self, player: &mut MusicPlayer) {
        match self {
            Self::Search {
                query,
                results,
                selection,
                scroll,
            } => {
                player.search_query.clone_from(query);
                player.search_results.clone_from(results);
                player.selected_indices.clone_from(selection);
                player.search_list_scroll = *scroll;
            }
            Self::Radio {
                label,
                tracks,
                selection,
                scroll,
            } => {
                player.radio_label.clone_from(label);
                player.radio_tracks.clone_from(tracks);
                player.selected_indices.clone_from(selection);
                player.search_list_scroll = *scroll;
            }
            Self::TrackList {
                playlist,
                playlist_name,
                selection,
                scroll,
            } => {
                player.selected_playlist = *playlist;
                player.selected_playlist_name.clone_from(playlist_name);
                player.selected_indices.clone_from(selection);
                player.playlist_list_scroll = *scroll;
            }
        }
    }
}
