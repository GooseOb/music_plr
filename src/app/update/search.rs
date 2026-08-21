use super::{mpsc, thread, BackendResult, MusicPlayer, ViewData};
use crate::app::ViewKind;
use crate::data::library::{LibraryItem, LibraryKind};

impl MusicPlayer {
    pub fn run_search(&mut self) {
        if self.search_query.is_empty() {
            return;
        }
        let query = self.search_query.clone();
        let scope = self.search_scope;
        let provider = self.search_provider;

        // Switch to Search view. `new_search()` returns an empty, non-loading
        // state for the active scope; flip `loading` on and clear the dropdown.
        // Push as a fresh history slot so the outgoing view survives for Back.
        let mut new_view = ViewData::new_search(query.clone(), provider, scope);
        new_view.loading = true;
        self.push_new_view(new_view);
        let rid = self.request_ids.next();
        self.view_data_mut().request_id = rid;
        self.sync_search_scope();
        self.show_search_history = false;
        self.drag.clear_hovered_track();

        self.search_history
            .push(query.clone(), self.config.max_search_history_stored);

        let tx = self.result_tx.clone();
        Self::spawn_backend_thread(
            move || crate::provider::search(provider, &query, scope, 0),
            move |(tracks, tab)| BackendResult::SearchResults(rid, tracks, tab),
            tx,
        );
    }

    /// Spawn a background thread that runs `run` (or returns an error), maps
    /// the result into a `BackendResult`, and sends it on `tx`.
    /// All search/radio/browse callers share this one thread body.
    pub(super) fn spawn_backend_thread<T, R, M>(
        run: R,
        make_result: M,
        tx: mpsc::Sender<BackendResult>,
    ) where
        R: FnOnce() -> anyhow::Result<T> + Send + 'static,
        M: FnOnce(T) -> BackendResult + Send + 'static,
    {
        thread::spawn(move || match run() {
            Ok(tracks) => {
                let _ = tx.send(make_result(tracks));
            }
            Err(e) => {
                let _ = tx.send(BackendResult::SearchError(e.to_string()));
            }
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
        let offset = count;
        let provider = self.search_provider;
        let tx = self.result_tx.clone();

        thread::spawn(move || {
            let tracks = match crate::provider::search_more(provider, &query, offset) {
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
            self.run_search();
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

    /// Open a loading radio view, stamp its request id, notify, and fetch its
    /// Start a song radio seeded by `provider` (only providers that support
    /// similarity search offer this; currently `YouTube`). Tracks from other
    /// providers fall back to a `YouTube` radio when a `YouTube` id is present.
    pub fn start_song_radio_provider(
        &mut self,
        provider: crate::provider::ProviderId,
        name: &str,
        id: &str,
    ) {
        if provider.capabilities().radio {
            let label = format!("Radio ({}): {name}", provider.label());
            self.push_new_view(ViewData::new_radio(ViewKind::SongRadio(label.clone())));
            let rid = self.request_ids.next();
            self.view_data_mut().request_id = rid;
            self.notify(format!("Generating radio for song: {name}..."));
            let id = id.to_string();
            let tx = self.result_tx.clone();
            Self::spawn_backend_thread(
                move || crate::provider::radio_song(provider, &id),
                move |tracks| BackendResult::RadioResults(rid, label.clone(), tracks),
                tx,
            );
        } else {
            self.notify(format!("{provider:?} does not support radio"));
        }
    }

    /// Start an artist radio seeded by `provider`. Same fallback rules as
    /// [`Self::start_song_radio_provider`].
    pub fn start_artist_radio_provider(
        &mut self,
        provider: crate::provider::ProviderId,
        name: &str,
        id: &str,
    ) {
        if provider.capabilities().radio {
            let label = format!("Radio ({}): {name}", provider.label());
            self.push_new_view(ViewData::new_radio(ViewKind::ArtistRadio(label.clone())));
            let rid = self.request_ids.next();
            self.view_data_mut().request_id = rid;
            self.notify(format!("Generating radio for artist: {name}..."));
            let id = id.to_string();
            let tx = self.result_tx.clone();
            Self::spawn_backend_thread(
                move || crate::provider::radio_artist(provider, &id),
                move |tracks| BackendResult::RadioResults(rid, label.clone(), tracks),
                tx,
            );
        } else {
            self.notify(format!("{provider:?} does not support radio"));
        }
    }

    /// Shared drill-down: switch to the given browse view kind (loading),
    /// fetch its tracks via ytmusicapi `browse()`, and send `BrowseResults`.
    /// All browse parameters are derived from `kind` via `ViewKind::browse_params`.
    pub fn handle_browse(&mut self, kind: &ViewKind) {
        let (id, kind_str, label) = kind
            .browse_params()
            .expect("start_browse called with a non-browse ViewKind");
        self.push_new_view(ViewData {
            kind: kind.clone(),
            loading: true,
            ..Default::default()
        });
        let rid = self.request_ids.next();
        self.view_data_mut().request_id = rid;
        self.notify(format!("Opening: {label}..."));
        let tx = self.result_tx.clone();
        let id = id.to_string();
        Self::spawn_backend_thread(
            move || crate::youtube::browse(&id, kind_str),
            move |tracks| BackendResult::BrowseResults(rid, tracks),
            tx,
        );
    }

    pub fn current_library_item(&self) -> Option<LibraryItem> {
        match &self.view_data().kind {
            ViewKind::Artist { id, name } => Some(LibraryItem {
                kind: LibraryKind::Artist,
                id: id.clone(),
                title: name.clone(),
                thumbnail: String::new(),
            }),
            ViewKind::Album { id, name } => Some(LibraryItem {
                kind: LibraryKind::Album,
                id: id.clone(),
                title: name.clone(),
                thumbnail: String::new(),
            }),
            ViewKind::PlaylistView { id, name } => Some(LibraryItem {
                kind: LibraryKind::Playlist,
                id: id.clone(),
                title: name.clone(),
                thumbnail: String::new(),
            }),
            _ => None,
        }
    }

    pub fn toggle_library_save(&mut self, item: LibraryItem) -> bool {
        if self.library.contains(item.kind, &item.id) {
            self.library.remove(item.kind, &item.id);
            false
        } else {
            self.library.add(item);
            true
        }
    }
}
