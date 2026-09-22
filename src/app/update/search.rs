use super::{mpsc, thread, BackendResult, Message, MusicPlayer, Task, ViewData};
use crate::{
    app::{pane::PaneId, update::operation::CaptureSearchHistoryRows, ViewKind},
    data::library::{LibraryItem, LibraryKind},
    load_state::LoadState,
    providers::ProviderId,
    types::Track,
};

impl MusicPlayer {
    pub fn run_search(&mut self, pane: PaneId) -> Task<Message> {
        let query = self.pane(pane).search_query.clone();
        let scope = self.pane(pane).search_scope;
        let provider = self.pane(pane).search_provider;

        // Switch to Search view. `new_search()` returns an empty, loading
        // state; clear the search-history dropdown.
        // Push as a fresh history slot so the outgoing view survives for Back.
        let new_view = ViewData::new_search(query.clone(), provider, scope);
        let nav_task = self.push_new_view(pane, new_view);
        let rid = self.request_ids.next();
        self.view_data_in_mut(pane).request_id = rid;
        self.sync_search_scope(pane);
        self.pane_mut(pane).show_search_history = false;
        self.drag.clear_hovered_track();

        // A blank query is a browse (charts/trending), not a real search, so
        // it doesn't belong in search history.
        if !query.trim().is_empty() {
            self.search_history
                .push(query.clone(), self.config.max_search_history_stored);
        }

        let tx = self.result_tx.clone();
        Self::spawn_backend_thread(
            rid,
            move || crate::providers::search(provider, &query, scope, 0),
            move |(tracks, tab)| BackendResult::SearchResults(rid, tracks, tab),
            tx,
        );
        nav_task
    }

    /// Sidebar "Search" click: restore the most recent completed search view
    /// (query, results, active tab). Until any search has finished this
    /// session, behave like the search button and run a search (a blank
    /// query browses charts/trending).
    pub fn handle_sidebar_search(&mut self) -> Task<Message> {
        let pane = self.focused_pane_id;
        let Some(last) = &self.last_search_view else {
            return self.run_search(pane);
        };
        if self.view_data_in(pane).same_kind(last) {
            Task::none()
        } else {
            self.handle_navigate_to(pane, last.clone())
        }
    }

    /// Spawn a background thread that runs `run` (or returns an error), maps
    /// the result into a `BackendResult`, and sends it on `tx`. Failures carry
    /// `rid` so the tick can route them to the requesting pane's slot.
    /// All search/radio/browse callers share this one thread body.
    pub(super) fn spawn_backend_thread<T, R, M>(
        rid: u64,
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
                let _ = tx.send(BackendResult::SearchError(rid, e.to_string()));
            }
        });
    }

    pub fn handle_search_execute(&mut self, pane: PaneId) -> Task<Message> {
        if self.pane(pane).show_search_history {
            if let Some(i) = self.drag.hovered_search_history() {
                return self.handle_search_history_select(pane, i);
            }
        }
        self.run_search(pane)
    }

    pub fn handle_search_scope_changed(
        &mut self,
        pane: PaneId,
        scope: crate::providers::SearchScope,
    ) -> Task<Message> {
        if scope != self.pane(pane).search_scope {
            self.pane_mut(pane).search_scope = scope;
            self.save_session();
            return self.run_search(pane);
        }
        Task::none()
    }

    pub fn scope_name(&self, scope: crate::providers::SearchScope) -> &str {
        match scope {
            crate::providers::SearchScope::Songs => self.strings.scope_songs,
            crate::providers::SearchScope::Videos => self.strings.scope_videos,
            crate::providers::SearchScope::Artists => self.strings.scope_artists,
            crate::providers::SearchScope::Albums => self.strings.scope_albums,
            crate::providers::SearchScope::Playlists => self.strings.scope_playlists,
        }
    }

    pub fn stage_search_provider(&mut self, pane: PaneId, provider: crate::providers::ProviderId) {
        if provider == self.pane(pane).search_provider {
            return;
        }
        if !provider.capabilities().search {
            self.notify(format!(
                "{}: {}",
                provider.label(),
                self.strings.deps_not_installed
            ));
            return;
        }
        self.pane_mut(pane).search_provider = provider;
        if !provider
            .supported_scopes()
            .contains(&self.pane(pane).search_scope)
        {
            self.pane_mut(pane).search_scope = provider.supported_scopes()[0];
        }
        self.save_session();
        self.notify(provider.label().to_string());
    }

    pub fn cycle_search_provider(&mut self, pane: PaneId, dir: isize) {
        let list = crate::providers::ProviderId::searchable();
        let cur = list
            .iter()
            .position(|&p| p == self.pane(pane).search_provider)
            .unwrap_or(0)
            .cast_signed();
        let next = list[((cur + dir).rem_euclid(list.len().cast_signed())) as usize];
        self.stage_search_provider(pane, next);
    }

    pub fn stage_search_scope(&mut self, pane: PaneId, scope: crate::providers::SearchScope) {
        if scope == self.pane(pane).search_scope {
            return;
        }
        if !self
            .pane(pane)
            .search_provider
            .supported_scopes()
            .contains(&scope)
        {
            return;
        }
        self.pane_mut(pane).search_scope = scope;
        self.save_session();
        self.notify(self.scope_name(scope).to_string());
    }

    pub fn cycle_search_scope(&mut self, pane: PaneId, dir: isize) {
        let provider = self.pane(pane).search_provider;
        let scopes = provider.supported_scopes();
        let cur = scopes
            .iter()
            .position(|&s| s == self.pane(pane).search_scope)
            .unwrap_or(0)
            .cast_signed();
        let next = scopes[((cur + dir).rem_euclid(scopes.len().cast_signed())) as usize];
        self.stage_search_scope(pane, next);
    }

    pub fn handle_search_provider_changed(
        &mut self,
        pane: PaneId,
        provider: crate::providers::ProviderId,
    ) -> Task<Message> {
        if provider != self.pane(pane).search_provider {
            self.pane_mut(pane).search_provider = provider;
            // Clamp the scope to one the new provider supports.
            if !provider
                .supported_scopes()
                .contains(&self.pane(pane).search_scope)
            {
                self.pane_mut(pane).search_scope = provider.supported_scopes()[0];
            }
            self.save_session();
            return self.run_search(pane);
        }
        Task::none()
    }

    pub fn handle_search_load_more(&mut self, pane: PaneId) {
        if !matches!(self.view_data_in(pane).kind, ViewKind::Search(_)) {
            return;
        }
        let vd = self.view_data_in(pane);
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
        let rid = self.slot_request_id(pane);
        if let ViewKind::Search(s) = &mut self.view_data_in_mut(pane).kind {
            s.append_in_flight = true;
        }

        let query = self.pane(pane).search_query.clone();
        let offset = count;
        let provider = self.pane(pane).search_provider;
        let tx = self.result_tx.clone();

        thread::spawn(move || {
            let tracks = match crate::providers::search_more(provider, &query, offset) {
                Ok(tracks) => tracks,
                Err(e) => {
                    let _ = tx.send(BackendResult::SearchError(rid, e.to_string()));
                    return;
                }
            };
            let _ = tx.send(BackendResult::SearchResultsAppend(rid, tracks));
        });
    }

    pub fn handle_search_history_select(&mut self, pane: PaneId, index: usize) -> Task<Message> {
        let query = self.pane(pane).last_filtered_history.get(index).cloned();
        if let Some(query) = query {
            self.pane_mut(pane).search_query = query;
            self.pane_mut(pane).show_search_history = false;
            self.drag.clear_hovered_search_history();
            self.run_search(pane)
        } else {
            Task::none()
        }
    }

    pub fn handle_delete_search_history(&mut self, pane: PaneId, index: usize) {
        let query = self.pane(pane).last_filtered_history.get(index).cloned();
        if let Some(query) = query {
            self.search_history.remove(&query);
            self.update_search_history(pane);
        }
    }

    pub fn update_search_history(&mut self, pane: PaneId) {
        let query_lower = self.pane(pane).search_query.to_lowercase();
        let mut filtered = self.search_history.filtered(&query_lower);
        if filtered.len() > self.config.max_search_history_visible {
            filtered.truncate(self.config.max_search_history_visible);
        }
        self.pane_mut(pane).last_filtered_history = filtered;
    }

    pub fn activate_search_input(
        &mut self,
        pane: PaneId,
    ) -> iced::Task<crate::app::message::Message> {
        self.update_search_history(pane);
        self.pane_mut(pane).show_search_history = true;
        CaptureSearchHistoryRows::new(pane).into()
    }

    /// Start a song or artist radio seeded by `provider`. When the track
    /// carries no id for the provider, one is resolved by search inside the
    /// spawned thread before querying the radio.
    pub fn start_radio_provider(
        &mut self,
        pane: PaneId,
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
        let nav_task = self.push_new_view(pane, ViewData::new_radio(kind));
        let rid = self.request_ids.next();
        self.view_data_in_mut(pane).request_id = rid;
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
            rid,
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
        pane: PaneId,
        kind: &ViewKind,
        provider: crate::providers::ProviderId,
    ) -> Task<Message> {
        let params = kind
            .browse_params()
            .expect("start_browse called with a non-browse ViewKind");
        let (id, kind_str, label) = (params.id, params.kind, params.name);
        let nav_task = self.push_new_view(
            pane,
            ViewData {
                kind: kind.clone(),
                content: crate::load_state::LoadState::Loading,
                ..Default::default()
            },
        );
        let rid = self.request_ids.next();
        self.view_data_in_mut(pane).request_id = rid;
        let msg = (self.strings.opening)(label);
        self.notify(msg);
        let tx = self.result_tx.clone();
        let id = id.to_string();
        Self::spawn_backend_thread(
            rid,
            move || crate::providers::browse(provider, &id, kind_str),
            move |(tracks, meta)| BackendResult::BrowseResults(rid, tracks, meta),
            tx,
        );
        nav_task
    }

    pub fn current_library_item(&self, pane: PaneId) -> Option<LibraryItem> {
        match &self.view_data_in(pane).kind {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        data::config,
        providers::{ProviderId, SearchScope},
    };

    fn player() -> MusicPlayer {
        let mut p = MusicPlayer::new_with(config::Config::default());
        p.reset_test_pane(vec![ViewData::new_search(
            String::new(),
            ProviderId::YouTube,
            SearchScope::Songs,
        )]);
        p
    }

    #[test]
    fn provider_cycle_wraps_and_clamps_scope() {
        let mut p = player();
        let pane = p.focused_pane_id;
        let start = p.pane(pane).search_provider;
        for _ in 0..ProviderId::searchable().len() {
            p.cycle_search_provider(pane, 1);
        }
        assert_eq!(p.pane(pane).search_provider, start);
        assert!(p.notification.is_some());

        p.pane_mut(pane).search_scope = SearchScope::Playlists;
        p.stage_search_provider(pane, ProviderId::LastFm);
        assert_eq!(p.pane(pane).search_provider, ProviderId::LastFm);
        assert_eq!(p.pane(pane).search_scope, SearchScope::Songs);

        p.cycle_search_scope(pane, 1);
        assert_eq!(p.pane(pane).search_scope, SearchScope::Artists);
        p.cycle_search_scope(pane, -1);
        assert_eq!(p.pane(pane).search_scope, SearchScope::Songs);
    }
}
