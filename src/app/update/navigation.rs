use super::{Message, MusicPlayer, NavEntry, Task, View, ViewSnapshot};

impl MusicPlayer {
    pub const fn can_navigate_back(&self) -> bool {
        self.nav_history_pos > 0
    }

    pub const fn can_navigate_forward(&self) -> bool {
        self.nav_history_pos + 1 < self.nav_history.len()
    }

    pub(super) fn snapshot_current(&self) -> ViewSnapshot {
        let scroll = self.get_current_list_scroll();
        match self.current_view {
            View::Search => ViewSnapshot::Search {
                query: self.search_query.clone(),
                results: self.search_results.clone(),
                selection: self.selected_indices.clone(),
                scroll,
            },
            View::SongRadio | View::ArtistRadio => ViewSnapshot::Radio {
                label: self.radio_label.clone(),
                tracks: self.radio_tracks.clone(),
                selection: self.selected_indices.clone(),
                scroll,
            },
            View::Playlist | View::Downloads => ViewSnapshot::TrackList {
                playlist: self.selected_playlist,
                playlist_name: self.selected_playlist_name.clone(),
                selection: self.selected_indices.clone(),
                scroll,
            },
        }
    }

    pub(super) fn restore_nav_entry(&mut self, entry: &NavEntry) -> Task<Message> {
        self.current_view = entry.view.clone();
        match &entry.snapshot {
            ViewSnapshot::Search {
                query,
                results,
                selection,
                scroll,
            } => {
                self.search_query.clone_from(query);
                self.search_results.clone_from(results);
                self.selected_indices.clone_from(selection);
                self.search_list_scroll = *scroll;
            }
            ViewSnapshot::Radio {
                label,
                tracks,
                selection,
                scroll,
            } => {
                self.radio_label.clone_from(label);
                self.radio_tracks.clone_from(tracks);
                self.selected_indices.clone_from(selection);
                self.search_list_scroll = *scroll;
            }
            ViewSnapshot::TrackList {
                playlist,
                playlist_name,
                selection,
                scroll,
            } => {
                self.selected_playlist = *playlist;
                self.selected_playlist_name.clone_from(playlist_name);
                self.selected_indices.clone_from(selection);
                self.playlist_list_scroll = *scroll;
            }
        }

        // Scroll position is stored relative to the main track_list scrollable.
        // (Queue view uses a different Id and is not navigated via history.)
        iced::widget::operation::scroll_to::<Message>(
            iced::widget::Id::new("track_list"),
            iced::widget::operation::AbsoluteOffset {
                x: 0.0,
                y: entry.snapshot.scroll(),
            },
        )
    }

    pub fn handle_navigate_to(&mut self, view: View) {
        self.nav_history.truncate(self.nav_history_pos + 1);
        self.show_search_history = false;
        self.cleanup_drag_state();

        let back_entry = NavEntry {
            view: self.current_view.clone(),
            snapshot: self.snapshot_current(),
        };
        self.nav_history.push(back_entry);

        self.current_view = view;
        self.selected_indices.clear();
        self.focused_list_index = 0;

        let new_entry = NavEntry {
            view: self.current_view.clone(),
            snapshot: self.snapshot_current(),
        };
        self.nav_history.push(new_entry);

        if self.nav_history.len() > 20 {
            self.nav_history.remove(0);
            self.nav_history_pos = self.nav_history_pos.saturating_sub(1);
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
        let entry = NavEntry {
            view: self.current_view.clone(),
            snapshot: self.snapshot_current(),
        };
        self.nav_history.truncate(self.nav_history_pos + 1);
        self.nav_history.push(entry);
        if self.nav_history.len() > 20 {
            self.nav_history.remove(0);
            self.nav_history_pos = self.nav_history_pos.saturating_sub(1);
        }
        self.nav_history_pos = self.nav_history.len() - 1;
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
}
