use super::*;

impl MusicPlayer {
    pub fn can_navigate_back(&self) -> bool {
        self.nav_history_pos > 0
    }

    pub fn can_navigate_forward(&self) -> bool {
        self.nav_history_pos + 1 < self.nav_history.len()
    }

    pub(super) fn snapshot_current(&self) -> ViewSnapshot {
        match self.current_view {
            View::Search => ViewSnapshot::Search {
                query: self.search_query.clone(),
                results: self.search_results.clone(),
                selection: self.selected_indices.clone(),
            },
            View::SongRadio | View::ArtistRadio => ViewSnapshot::Radio {
                label: self.radio_label.clone(),
                tracks: self.radio_tracks.clone(),
                selection: self.selected_indices.clone(),
            },
            View::Playlist | View::Downloads => ViewSnapshot::Playlist {
                playlist: self.selected_playlist,
                playlist_name: self.selected_playlist_name.clone(),
                selection: self.selected_indices.clone(),
            },
        }
    }

    fn restore_nav_entry(&mut self, entry: &NavEntry) {
        self.current_view = entry.view.clone();
        match &entry.snapshot {
            ViewSnapshot::Search {
                query,
                results,
                selection,
            } => {
                self.search_query = query.clone();
                self.search_results = results.clone();
                self.selected_indices = selection.clone();
            }
            ViewSnapshot::Radio {
                label,
                tracks,
                selection,
            } => {
                self.radio_label = label.clone();
                self.radio_tracks = tracks.clone();
                self.selected_indices = selection.clone();
            }
            ViewSnapshot::Playlist {
                playlist,
                playlist_name,
                selection,
            } => {
                self.selected_playlist = *playlist;
                self.selected_playlist_name = playlist_name.clone();
                self.selected_indices = selection.clone();
            }
        }
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

    pub fn handle_navigate_back(&mut self) {
        if self.nav_history_pos > 0 {
            self.nav_history_pos -= 1;
            let entry = self.nav_history[self.nav_history_pos].clone();
            self.restore_nav_entry(&entry);
            self.save_session();
        }
    }

    pub fn handle_navigate_forward(&mut self) {
        if self.nav_history_pos + 1 < self.nav_history.len() {
            self.nav_history_pos += 1;
            let entry = self.nav_history[self.nav_history_pos].clone();
            self.restore_nav_entry(&entry);
            self.save_session();
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
