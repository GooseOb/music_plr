use super::{spawn_thumbnail_download_thread, BackendResult, RustTrack, SearchState, View};
use crate::config;
use crate::youtube;
use slint::ComponentHandle;
use tracing::error;

impl super::Backend {
    pub fn handle_search_execute(&mut self) {
        let query = self.search_query.clone();
        if query.trim().is_empty() {
            return;
        }
        self.selected_playlist = None;
        self.selected_playlist_name.clear();
        self.clear_selection();
        self.current_view = View::Search;
        self.update_nav_ui();
        self.save_session();
        self.loading = true;
        self.notify(format!("Searching: {query}"));

        self.config.search_history.retain(|h| h != &query);
        self.config.search_history.insert(0, query.clone());
        self.config
            .search_history
            .truncate(self.config.max_search_history_stored);
        self.config.last_search_query = query.clone();
        config::save_config(&self.config);

        let result_tx = self.result_tx.clone();
        std::thread::spawn(move || match youtube::search(&query, 0) {
            Ok(videos) => {
                let tracks: Vec<RustTrack> = videos.into_iter().map(RustTrack::from).collect();
                spawn_thumbnail_download_thread(&tracks, result_tx.clone());
                let _ = result_tx.send(BackendResult::SearchResults(tracks));
            }
            Err(e) => {
                error!("Search error: {}", e);
                let _ = result_tx.send(BackendResult::SearchError(e.to_string()));
            }
        });
    }

    pub fn handle_search_load_more(&mut self) {
        let query = self.search_query.clone();
        let offset = self.search_offset;
        self.loading = true;
        self.notify(format!("Loading more: {query}"));
        let result_tx = self.result_tx.clone();
        std::thread::spawn(move || match youtube::search(&query, offset) {
            Ok(videos) => {
                let tracks: Vec<RustTrack> = videos.into_iter().map(RustTrack::from).collect();
                spawn_thumbnail_download_thread(&tracks, result_tx.clone());
                let _ = result_tx.send(BackendResult::SearchResultsAppend(tracks));
            }
            Err(e) => {
                error!("Search load more error: {}", e);
                let _ = result_tx.send(BackendResult::SearchError(e.to_string()));
            }
        });
    }

    pub fn handle_search_history_select(&mut self, index: usize) {
        let items: Vec<String> = self
            .filtered_history()
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        if let Some(selected) = items.get(index) {
            self.search_query = selected.clone();
            if let Some(w) = self.ui.upgrade() {
                let search_state = w.global::<SearchState>();
                search_state.set_search_input_text(selected.clone().into());
                search_state.set_show_search_history(false);
            }
            self.handle_search_execute();
        }
    }

    pub fn handle_delete_search_history(&mut self, index: usize) {
        let items: Vec<usize> = self
            .filtered_history()
            .into_iter()
            .map(|(i, _)| i)
            .collect();
        if let Some(&real_idx) = items.get(index) {
            self.config.search_history.remove(real_idx);
            config::save_config(&self.config);
            if let Some(w) = self.ui.upgrade() {
                self.update_search_history(&w);
            }
        }
    }

    pub fn update_search_history(&mut self, window: &super::AppWindow) {
        let filtered: Vec<String> = self
            .filtered_history()
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        if filtered != self.last_filtered_history {
            self.last_filtered_history = filtered.clone();
            let slint_items: Vec<slint::SharedString> =
                filtered.iter().map(|s| s.as_str().into()).collect();
            window.global::<SearchState>().set_search_history_items(
                std::rc::Rc::new(slint::VecModel::from(slint_items)).into(),
            );
        }
    }

    fn filtered_history(&self) -> Vec<(usize, String)> {
        let history = &self.config.search_history;
        let max_visible = self.config.max_search_history_visible;
        let query = self.search_query.trim().to_lowercase();
        if query.is_empty() {
            history
                .iter()
                .take(max_visible)
                .enumerate()
                .map(|(i, s)| (i, s.clone()))
                .collect()
        } else {
            history
                .iter()
                .enumerate()
                .filter(|(_, s)| config::fuzzy_match(&query, s))
                .take(max_visible)
                .map(|(i, s)| (i, s.clone()))
                .collect()
        }
    }
}
