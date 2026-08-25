use super::{mpsc, thread, BackendResult, MusicPlayer, ViewData};
use crate::data::library::{LibraryItem, LibraryKind};
use crate::{
    app::{update::operation::CaptureSearchHistoryRows, ViewKind},
    load_state::LoadState,
    providers::ProviderId,
    types::Track,
};

impl MusicPlayer {
    pub fn run_search(&mut self) {
        if self.search_query.is_empty() {
            return;
        }
        let query = self.search_query.clone();
        let scope = self.search_scope;
        let provider = self.search_provider;

        // Switch to Search view. `new_search()` returns an empty, loading
        // state; clear the search-history dropdown.
        // Push as a fresh history slot so the outgoing view survives for Back.
        let new_view = ViewData::new_search(query.clone(), provider, scope);
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
            move || crate::providers::search(provider, &query, scope, 0),
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

    pub fn handle_search_execute(&mut self) {
        if self.show_search_history {
            if let Some(i) = self.drag.hovered_search_history() {
                self.handle_search_history_select(i);
                return;
            }
        }
        self.run_search();
    }

    pub fn handle_search_scope_changed(&mut self, scope: crate::providers::SearchScope) {
        if scope != self.search_scope {
            self.search_scope = scope;
            self.run_search();
        }
    }

    pub fn handle_search_provider_changed(&mut self, provider: crate::providers::ProviderId) {
        if provider != self.search_provider {
            self.search_provider = provider;
            // Clamp the scope to one the new provider supports.
            if !provider.supported_scopes().contains(&self.search_scope) {
                self.search_scope = provider.supported_scopes()[0];
            }
            self.run_search();
        }
    }

    pub fn handle_search_load_more(&mut self) {
        if !matches!(self.view_data_mut().kind, ViewKind::Search(_)) {
            return;
        }
        let vd = self.view_data();
        let ViewKind::Search(search) = &vd.kind else {
            return;
        };
        let count = match &vd.content {
            LoadState::Ready(tracks) => tracks.len(),
            _ => return,
        };
        if search.exhausted || count == 0 || search.append_in_flight {
            return;
        }

        // Append targets the slot that issued the original search.
        let rid = self.view_data_mut().request_id;
        if let ViewKind::Search(s) = &mut self.view_data_mut().kind {
            s.append_in_flight = true;
        }

        let query = self.search_query.clone();
        let offset = count;
        let provider = self.search_provider;
        let tx = self.result_tx.clone();

        thread::spawn(move || {
            let tracks = match crate::providers::search_more(provider, &query, offset) {
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
            self.drag.clear_hovered_search_history();
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

    pub fn activate_search_input(&mut self) -> iced::Task<crate::app::message::Message> {
        self.update_search_history();
        self.show_search_history = true;
        iced_runtime::task::widget(CaptureSearchHistoryRows::new())
    }

    /// Start a song or artist radio seeded by `provider`. When the track
    /// carries no id for the provider, one is resolved by search inside the
    /// spawned thread before querying the radio.
    pub fn start_radio_provider(
        &mut self,
        provider: crate::providers::ProviderId,
        track: &Track,
        artist: bool,
    ) {
        if !provider.capabilities().radio {
            self.notify(format!("{provider:?} does not support radio"));
            return;
        }
        let name = if artist { &track.artist } else { &track.title };
        let label = format!("Radio ({}): {name}", provider.label());
        let kind = if artist {
            ViewKind::ArtistRadio(label.clone())
        } else {
            ViewKind::SongRadio(label.clone())
        };
        self.push_new_view(ViewData::new_radio(kind));
        let rid = self.request_ids.next();
        self.view_data_mut().request_id = rid;
        self.notify(format!(
            "Generating radio for {}: {name}...",
            if artist { "artist" } else { "song" }
        ));
        let id = if artist {
            track.provider_artist_id(provider)
        } else {
            track.provider_id(provider)
        }
        .unwrap_or_default()
        .to_string();
        let name = name.clone();
        let seed = track.clone();
        let tx = self.result_tx.clone();
        let radio_fn: fn(
            crate::providers::ProviderId,
            &str,
        ) -> anyhow::Result<Vec<crate::types::Track>> = if artist {
            crate::providers::radio_artist
        } else {
            crate::providers::radio_song
        };
        Self::spawn_backend_thread(
            move || {
                let id = if id.is_empty() {
                    let resolved = if artist {
                        crate::providers::resolve_artist_id(provider, &name)?
                    } else {
                        crate::providers::resolve_id(provider, &seed)?
                            .and_then(|t| t.provider_id(provider).map(str::to_owned))
                    };
                    match resolved {
                        Some(id) => id,
                        None => anyhow::bail!(format!(
                            "Could not find \"{name}\" on {}",
                            provider.label()
                        )),
                    }
                } else {
                    id
                };
                radio_fn(provider, &id)
            },
            move |tracks| BackendResult::RadioResults(rid, label.clone(), tracks),
            tx,
        );
    }

    /// Shared drill-down: switch to the given browse view kind (loading),
    /// fetch its tracks via the provider's `browse()`, and send
    /// `BrowseResults`. All browse parameters are derived from `kind` via
    /// `ViewKind::browse_params`; the originating `provider` selects which
    /// backend answers the browse (`YouTube` cards vs. `MusicBrainz` `artist`/
    /// `release` pages).
    pub fn handle_browse(&mut self, kind: &ViewKind, provider: crate::providers::ProviderId) {
        let (id, kind_str, label) = kind
            .browse_params()
            .expect("start_browse called with a non-browse ViewKind");
        self.push_new_view(ViewData {
            kind: kind.clone(),
            content: crate::load_state::LoadState::Loading,
            ..Default::default()
        });
        let rid = self.request_ids.next();
        self.view_data_mut().request_id = rid;
        self.notify(format!("Opening: {label}..."));
        let tx = self.result_tx.clone();
        let id = id.to_string();
        Self::spawn_backend_thread(
            move || crate::providers::browse(provider, &id, kind_str),
            move |(tracks, meta)| BackendResult::BrowseResults(rid, tracks, meta),
            tx,
        );
    }

    pub fn current_library_item(&self) -> Option<LibraryItem> {
        match &self.view_data().kind {
            ViewKind::Artist(a) => Some(LibraryItem {
                kind: LibraryKind::Artist,
                id: a.id.clone(),
                title: a.name.clone(),
                thumbnail: String::new(),
                provider: a.source,
            }),
            ViewKind::Album(r) => Some(LibraryItem {
                kind: LibraryKind::Album,
                id: r.id.clone(),
                title: r.name.clone(),
                thumbnail: String::new(),
                provider: ProviderId::YouTube,
            }),
            ViewKind::PlaylistView(r) => Some(LibraryItem {
                kind: LibraryKind::Playlist,
                id: r.id.clone(),
                title: r.name.clone(),
                thumbnail: String::new(),
                provider: ProviderId::YouTube,
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
