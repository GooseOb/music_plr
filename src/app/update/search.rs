use super::{mpsc, thread, BackendResult, Message, MusicPlayer, Task, ViewData};
use crate::{
    app::{update::operation::CaptureSearchHistoryRows, ViewKind},
    data::library::{LibraryItem, LibraryKind},
    load_state::LoadState,
    providers::ProviderId,
    types::Track,
};

impl MusicPlayer {
    pub fn run_search(&mut self) -> Task<Message> {
        if self.search_query.is_empty() {
            return Task::none();
        }
        let query = self.search_query.clone();
        let scope = self.search_scope;
        let provider = self.search_provider;

        // Switch to Search view. `new_search()` returns an empty, loading
        // state; clear the search-history dropdown.
        // Push as a fresh history slot so the outgoing view survives for Back.
        let new_view = ViewData::new_search(query.clone(), provider, scope);
        let nav_task = self.push_new_view(new_view);
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
        nav_task
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

    pub fn handle_search_execute(&mut self) -> Task<Message> {
        if self.show_search_history {
            if let Some(i) = self.drag.hovered_search_history() {
                return self.handle_search_history_select(i);
            }
        }
        self.run_search()
    }

    pub fn handle_search_scope_changed(
        &mut self,
        scope: crate::providers::SearchScope,
    ) -> Task<Message> {
        if scope != self.search_scope {
            self.search_scope = scope;
            self.save_session();
            return self.run_search();
        }
        Task::none()
    }

    pub fn handle_search_provider_changed(
        &mut self,
        provider: crate::providers::ProviderId,
    ) -> Task<Message> {
        if provider != self.search_provider {
            self.search_provider = provider;
            // Clamp the scope to one the new provider supports.
            if !provider.supported_scopes().contains(&self.search_scope) {
                self.search_scope = provider.supported_scopes()[0];
            }
            self.save_session();
            return self.run_search();
        }
        Task::none()
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

        // Append targets the slot that issued the original search. The id was
        // zeroed when the initial results landed, so mint a fresh one.
        let rid = self.slot_request_id();
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

    pub fn handle_search_history_select(&mut self, index: usize) -> Task<Message> {
        if index < self.last_filtered_history.len() {
            self.search_query = self.last_filtered_history[index].clone();
            self.show_search_history = false;
            self.drag.clear_hovered_search_history();
            self.run_search()
        } else {
            Task::none()
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
        CaptureSearchHistoryRows::new().into()
    }

    /// Start a song or artist radio seeded by `provider`. When the track
    /// carries no id for the provider, one is resolved by search inside the
    /// spawned thread before querying the radio.
    pub fn start_radio_provider(
        &mut self,
        provider: crate::providers::ProviderId,
        track: &Track,
        artist: bool,
    ) -> Task<Message> {
        if !provider.capabilities().radio {
            let p = format!("{provider:?}");
            let msg = (self.strings.provider_no_radio)(&p);
            self.notify(msg);
            return Task::none();
        }
        let name = if artist { &track.artist } else { &track.title };
        let word = if artist {
            self.strings.radio_word_artist
        } else {
            self.strings.radio_word_song
        };
        let label = (self.strings.radio_label)(word, name);
        let kind = if artist {
            ViewKind::ArtistRadio(label.clone())
        } else {
            ViewKind::SongRadio(label.clone())
        };
        let nav_task = self.push_new_view(ViewData::new_radio(kind));
        let rid = self.request_ids.next();
        self.view_data_mut().request_id = rid;
        let word = if artist {
            self.strings.radio_word_artist
        } else {
            self.strings.radio_word_song
        };
        let name = name.clone();
        let msg = (self.strings.generating_radio_for)(word, &name);
        self.notify(msg);
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
        let not_found = self.strings.could_not_find_on;
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
                        None => anyhow::bail!((not_found)(&name, provider.label())),
                    }
                } else {
                    id
                };
                radio_fn(provider, &id)
            },
            move |tracks| BackendResult::RadioResults(rid, label.clone(), tracks),
            tx,
        );
        nav_task
    }

    /// Shared drill-down: switch to the given browse view kind (loading),
    /// fetch its tracks via the provider's `browse()`, and send
    /// `BrowseResults`. All browse parameters are derived from `kind` via
    /// `ViewKind::browse_params`; the originating `provider` selects which
    /// backend answers the browse (`YouTube` cards vs. `MusicBrainz` `artist`/
    /// `release` pages).
    pub fn handle_browse(
        &mut self,
        kind: &ViewKind,
        provider: crate::providers::ProviderId,
    ) -> Task<Message> {
        let (id, kind_str, label) = kind
            .browse_params()
            .expect("start_browse called with a non-browse ViewKind");
        let nav_task = self.push_new_view(ViewData {
            kind: kind.clone(),
            content: crate::load_state::LoadState::Loading,
            ..Default::default()
        });
        let rid = self.request_ids.next();
        self.view_data_mut().request_id = rid;
        let msg = (self.strings.opening)(label);
        self.notify(msg);
        let tx = self.result_tx.clone();
        let id = id.to_string();
        Self::spawn_backend_thread(
            move || crate::providers::browse(provider, &id, kind_str),
            move |(tracks, meta)| BackendResult::BrowseResults(rid, tracks, meta),
            tx,
        );
        nav_task
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
