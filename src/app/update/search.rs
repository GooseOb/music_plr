use super::{BackendResult, MusicPlayer, Track, View};

impl MusicPlayer {
    pub fn handle_search_execute(&mut self) {
        if self.search_query.is_empty() {
            return;
        }

        // If switching from another view, the current entry already serves as
        // the back-target — no need to push it again. Just switch to Search.
        self.current_view = View::Search;
        self.show_search_history = false;

        self.search_loading = true;
        self.search_exhausted = false;
        self.notify(format!("Searching for \"{}\"...", self.search_query));
        self.search_results.clear();
        self.selected_indices.clear();
        self.focused_list_index = 0;

        self.search_history.push(
            self.search_query.clone(),
            self.config.max_search_history_stored,
        );

        let query = self.search_query.clone();
        let tx = self.result_tx.clone();

        std::thread::spawn(move || {
            let result = crate::youtube::search(&query, 0);
            match result {
                Ok(videos) => {
                    let tracks: Vec<Track> =
                        videos.into_iter().map(std::convert::Into::into).collect();
                    let _ = tx.send(BackendResult::SearchResults(tracks));
                }
                Err(e) => {
                    let _ = tx.send(BackendResult::SearchError(e.to_string()));
                }
            }
        });
    }

    pub fn handle_search_load_more(&mut self) {
        // search_exhausted is set true when a page returned fewer than a full
        // SEARCH_PAGE_SIZE, so there is nothing left to fetch.
        // (defined in theme.rs, shared with youtube.rs)
        if self.search_loading || self.search_exhausted || self.search_results.is_empty() {
            return;
        }
        self.search_loading = true;

        let query = self.search_query.clone();
        let offset = self.search_results.len();
        let tx = self.result_tx.clone();

        std::thread::spawn(move || {
            let result = crate::youtube::search_more(&query, offset);
            match result {
                Ok(videos) => {
                    let tracks: Vec<Track> =
                        videos.into_iter().map(std::convert::Into::into).collect();
                    let _ = tx.send(BackendResult::SearchResultsAppend(tracks));
                }
                Err(e) => {
                    let _ = tx.send(BackendResult::SearchError(e.to_string()));
                }
            }
        });
    }

    pub fn handle_search_history_select(&mut self, index: usize) {
        if index < self.last_filtered_history.len() {
            self.search_query = self.last_filtered_history[index].clone();
            self.show_search_history = false;
            self.handle_search_execute();
        }
    }

    pub fn handle_delete_search_history(&mut self, index: usize) {
        if index < self.last_filtered_history.len() {
            let query = self.last_filtered_history[index].clone();
            self.search_history.remove(&query);
            self.update_search_history();
        }
    }

    pub fn update_search_history(&mut self) {
        let query_lower = self.search_query.to_lowercase();
        self.last_filtered_history = self.search_history.filtered(&query_lower);
        if self.last_filtered_history.len() > self.config.max_search_history_visible {
            self.last_filtered_history
                .truncate(self.config.max_search_history_visible);
        }
    }

    pub fn start_song_radio(&mut self, song_name: String) {
        self.radio_label = format!("Radio: {song_name}");
        self.search_loading = true;
        self.notify(format!("Generating radio for song: {song_name}..."));

        self.current_view = View::SongRadio;
        self.show_search_history = false;
        self.clear_selection();
        self.focused_list_index = 0;

        let tx = self.result_tx.clone();
        let label = self.radio_label.clone();
        std::thread::spawn(move || {
            let result = crate::youtube::radio_song(&song_name);
            match result {
                Ok(videos) => {
                    let tracks: Vec<Track> =
                        videos.into_iter().map(std::convert::Into::into).collect();
                    let _ = tx.send(BackendResult::RadioResults(label, tracks));
                }
                Err(e) => {
                    let _ = tx.send(BackendResult::SearchError(e.to_string()));
                }
            }
        });
    }

    pub fn start_artist_radio(&mut self, artist_name: String) {
        self.radio_label = format!("Radio: {artist_name}");
        self.search_loading = true;
        self.notify(format!("Generating radio for artist: {artist_name}..."));

        self.current_view = View::ArtistRadio;
        self.show_search_history = false;
        self.clear_selection();
        self.focused_list_index = 0;

        let tx = self.result_tx.clone();
        let label = self.radio_label.clone();
        std::thread::spawn(move || {
            let result = crate::youtube::radio_artist(&artist_name);
            match result {
                Ok(videos) => {
                    let tracks: Vec<Track> =
                        videos.into_iter().map(std::convert::Into::into).collect();
                    let _ = tx.send(BackendResult::RadioResults(label, tracks));
                }
                Err(e) => {
                    let _ = tx.send(BackendResult::SearchError(e.to_string()));
                }
            }
        });
    }
}
