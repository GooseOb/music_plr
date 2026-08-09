use super::{mpsc, thread, BackendResult, MusicPlayer, Track, ViewData};
use crate::app::ViewKind;
use crate::youtube::SearchTab;

impl MusicPlayer {
    pub fn handle_search_execute(&mut self) {
        if self.search_query.is_empty() {
            return;
        }
        self.run_search(self.search_query.clone(), self.search_scope);
    }

    /// Run a fresh search for `query` at `scope`, replacing the Search view.
    pub fn run_search(&mut self, query: String, scope: crate::youtube::SearchScope) {
        if query.is_empty() {
            return;
        }

        // Switch to Search view. `new_search()` returns an empty, non-loading
        // state for the active scope; flip `loading` on and clear the dropdown.
        // Push as a fresh history slot so the outgoing view survives for Back.
        let mut new_view = ViewData::new_search(query.clone(), scope);
        new_view.loading = true;
        self.push_new_view(new_view);
        let rid = self.request_ids.next();
        self.view_data_mut().request_id = rid;
        self.sync_search_scope();
        self.show_search_history = false;
        self.drag.hovered_track = None;

        self.search_history
            .push(query.clone(), self.config.max_search_history_stored);

        let tx = self.result_tx.clone();
        Self::spawn_search_thread(query, scope, tx, move |tracks, tab| {
            BackendResult::SearchResults(rid, tracks, tab)
        });
    }

    /// Spawn a search that returns `(Vec<Track>, SearchTab)`, then wrap it in a
    /// `BackendResult` for the result channel.
    fn spawn_search_thread<F>(
        query: String,
        scope: crate::youtube::SearchScope,
        tx: mpsc::Sender<BackendResult>,
        make_result: F,
    ) where
        F: FnOnce(Vec<Track>, SearchTab) -> BackendResult + Send + 'static,
    {
        thread::spawn(move || {
            let (tracks, tab) = match crate::youtube::search(&query, scope, 0) {
                Ok(parts) => parts,
                Err(e) => {
                    let _ = tx.send(BackendResult::SearchError(e.to_string()));
                    return;
                }
            };
            let _ = tx.send(make_result(tracks, tab));
        });
    }

    pub fn handle_search_load_more(&mut self) {
        if !matches!(self.view_data_mut().kind, ViewKind::Search { .. }) {
            return;
        }
        let (loading, exhausted, count) = (
            self.view_data_mut().loading,
            self.view_data_mut().exhausted(),
            self.view_data_mut().tracks.len(),
        );
        if loading || exhausted || count == 0 {
            return;
        }

        // Append targets the slot that issued the original search.
        let rid = self.view_data_mut().request_id;
        self.view_data_mut().loading = true;

        let query = self.search_query.clone();
        let scope = self.search_scope;
        let offset = count;
        let tx = self.result_tx.clone();

        thread::spawn(move || {
            let tracks = match crate::youtube::search_more(&query, scope, offset) {
                Ok(tracks) => tracks,
                Err(e) => {
                    let _ = tx.send(BackendResult::SearchError(e.to_string()));
                    return;
                }
            };
            let _ = tx.send(BackendResult::SearchResultsAppend(rid, tracks));
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
        let label = format!("Radio: {song_name}");
        self.push_new_view(ViewData::new_radio(ViewKind::SongRadio(label.clone())));
        let rid = self.request_ids.next();
        self.view_data_mut().request_id = rid;
        self.notify(format!("Generating radio for song: {song_name}..."));

        let tx = self.result_tx.clone();
        Self::spawn_youtube_thread(song_name, tx, crate::youtube::radio_song, move |tracks| {
            BackendResult::RadioResults(rid, label.clone(), tracks)
        });
    }

    pub fn start_artist_radio(&mut self, artist_name: String) {
        let label = format!("Radio: {artist_name}");
        self.push_new_view(ViewData::new_radio(ViewKind::ArtistRadio(label.clone())));
        let rid = self.request_ids.next();
        self.view_data_mut().request_id = rid;
        self.notify(format!("Generating radio for artist: {artist_name}..."));

        let tx = self.result_tx.clone();
        Self::spawn_youtube_thread(
            artist_name,
            tx,
            crate::youtube::radio_artist,
            move |tracks| BackendResult::RadioResults(rid, label.clone(), tracks),
        );
    }

    /// Open an artist/album/playlist drill-down view, fetching its tracks.
    pub fn handle_open_artist(&mut self, browse_id: String, name: &str) {
        self.start_browse(
            &ViewKind::Artist {
                browse_id: browse_id.clone(),
                name: name.to_string(),
            },
            browse_id,
            "artist",
            name,
        );
    }

    pub fn handle_open_album(&mut self, browse_id: String, title: &str) {
        self.start_browse(
            &ViewKind::Album {
                browse_id: browse_id.clone(),
                title: title.to_string(),
            },
            browse_id,
            "album",
            title,
        );
    }

    pub fn handle_open_playlist(&mut self, playlist_id: String, title: &str) {
        self.start_browse(
            &ViewKind::PlaylistView {
                playlist_id: playlist_id.clone(),
                title: title.to_string(),
            },
            playlist_id,
            "playlist",
            title,
        );
    }

    /// Shared drill-down: switch to the given browse view kind (loading),
    /// fetch its tracks via ytmusicapi `browse()`, and send `BrowseResults`.
    fn start_browse(
        &mut self,
        kind: &ViewKind,
        browse_id: String,
        kind_str: &'static str,
        label: &str,
    ) {
        self.push_new_view(ViewData {
            kind: kind.clone(),
            loading: true,
            ..Default::default()
        });
        let rid = self.request_ids.next();
        self.view_data_mut().request_id = rid;
        self.notify(format!("Opening: {label}..."));
        let tx = self.result_tx.clone();
        thread::spawn(move || {
            let tracks = match crate::youtube::browse(&browse_id, kind_str) {
                Ok(videos) => videos.into_iter().map(Track::from).collect(),
                Err(e) => {
                    let _ = tx.send(BackendResult::SearchError(e.to_string()));
                    return;
                }
            };
            let _ = tx.send(BackendResult::BrowseResults(rid, tracks));
        });
    }

    /// Spawn a background thread that calls a `YouTube` search/radio function,
    /// converts the results to `Track`s, and sends a `BackendResult` via the
    /// provided channel. All four search/radio methods share this pattern.
    fn spawn_youtube_thread<F, R>(
        query: String,
        tx: mpsc::Sender<BackendResult>,
        search_fn: F,
        make_result: R,
    ) where
        F: FnOnce(&str) -> anyhow::Result<Vec<Track>> + Send + 'static,
        R: FnOnce(Vec<Track>) -> BackendResult + Send + 'static,
    {
        thread::spawn(move || match search_fn(&query) {
            Ok(tracks) => {
                let _ = tx.send(make_result(tracks));
            }
            Err(e) => {
                let _ = tx.send(BackendResult::SearchError(e.to_string()));
            }
        });
    }
}
