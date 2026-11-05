//! Home feed: cache seeding, background refresh and row playback.
//!
//! Local rows come straight from their stores at draw time (see `ui::home`);
//! only the remote rows live in [`HomeData`]. Each provider is fetched on its
//! own thread, so one slow source never holds up the others, and replies are
//! matched back by request id (a superseded refresh's late reply is dropped).

use std::thread;

use super::{BackendResult, Message, MusicPlayer, Task};
use crate::{
    app::{home::HomeTrackSource, pane::PaneId, ViewData, ViewKind},
    data::{home_cache, JsonStore},
    load_state::LoadState,
    providers::{
        self, HomeSection, HomeSectionData, HomeSectionKey, HomeSectionKind, ProviderError,
        ProviderId,
    },
    types::Track,
};

impl MusicPlayer {
    /// Paint a pane's Home view from the cache and refresh whatever is stale
    /// (or everything, when `force`). Called when a Home view becomes active.
    pub fn start_home(&mut self, pane: PaneId, force: bool) -> Task<Message> {
        if !matches!(self.view_data_in(pane).kind, ViewKind::Home(_)) {
            return Task::none();
        }
        let providers = providers::home_providers(self.config.default_provider);
        if self.install_home_cache(pane, &providers) {
            self.seed_active_view_thumbnails(pane);
        }
        if !force && !self.home_cache_stale(&providers) {
            return Task::none();
        }
        self.spawn_home_fetch(pane, providers)
    }

    /// Start a fetch for every pane currently showing Home (boot).
    pub fn start_home_fetches(&mut self) {
        for pane in self.pane_ids() {
            if matches!(self.view_data_in(pane).kind, ViewKind::Home(_)) {
                let _ = self.start_home(pane, false);
            }
        }
    }

    /// Returns whether anything was painted from the cache.
    fn install_home_cache(&mut self, pane: PaneId, providers: &[ProviderId]) -> bool {
        let install = match &self.view_data_in(pane).kind {
            ViewKind::Home(home) => home.sections.is_empty(),
            _ => false,
        };
        if !install {
            return false;
        }
        let cached: Vec<(HomeSectionKey, String, HomeSectionData)> = providers
            .iter()
            .flat_map(|&provider| {
                providers::home::planned_kinds(provider)
                    .iter()
                    .map(move |&kind| HomeSectionKey::new(provider, kind))
            })
            .filter_map(|key| {
                self.home_cache
                    .section(key)
                    .map(|cached| (key, cached.title.clone(), cached.data.clone()))
            })
            .collect();
        if cached.is_empty() {
            return false;
        }

        let ViewData {
            kind,
            content,
            thumbnails_seeded,
            ..
        } = self.view_data_in_mut(pane);
        let ViewKind::Home(home) = kind else {
            return false;
        };
        let mut sections = Vec::new();
        for (key, title, data) in cached {
            if key.kind == HomeSectionKind::QuickPicks {
                if let HomeSectionData::Tracks(tracks) = data {
                    if !tracks.is_empty() {
                        *content = LoadState::Ready(tracks);
                        home.quick_picks_provider = Some(key.provider);
                    }
                }
                continue;
            }
            sections.push(HomeSection::new(key, title, data));
        }
        home.sections = sections;
        providers::sort_sections(&mut home.sections, providers);
        *thumbnails_seeded = false;
        true
    }

    fn home_cache_stale(&self, providers: &[ProviderId]) -> bool {
        let keys = providers.iter().flat_map(|&provider| {
            providers::home::planned_kinds(provider)
                .iter()
                .map(move |&kind| HomeSectionKey::new(provider, kind))
        });
        match self.home_cache.oldest(keys) {
            Some(oldest) => home_cache::is_stale(oldest),
            None => true,
        }
    }

    fn spawn_home_fetch(&mut self, pane: PaneId, providers: Vec<ProviderId>) -> Task<Message> {
        let rid = self.request_ids.next();
        {
            let view = self.view_data_in_mut(pane);
            view.request_id = rid;
            if let ViewKind::Home(home) = &mut view.kind {
                home.pending = providers.len();
            }
        }
        let tx = self.result_tx.clone();
        for provider in providers {
            let tx = tx.clone();
            thread::spawn(move || {
                let result = providers::fetch_home(provider).map_err(ProviderError::from);
                let _ = tx.send(BackendResult::HomeLoaded {
                    rid,
                    provider,
                    result,
                });
            });
        }
        Task::none()
    }

    /// A provider's home rows arrived (or failed). Only the provider's own
    /// rows are replaced; everything else stays as it is, so a failed source
    /// keeps whatever the cache painted.
    pub fn handle_home_loaded(
        &mut self,
        rid: u64,
        provider: ProviderId,
        result: Result<Vec<HomeSection>, ProviderError>,
    ) -> Task<Message> {
        let Some((pane, idx)) = self.slot_for_request(rid) else {
            return Task::none();
        };
        let now = home_cache::now_secs();
        let providers = providers::home_providers(self.config.default_provider);
        let mut cached_rows: Vec<HomeSection> = Vec::new();
        {
            let Some(view) = self
                .panes
                .get_mut(&pane)
                .and_then(|pane| pane.nav_history.get_mut(idx))
            else {
                return Task::none();
            };
            if view.request_id != rid {
                return Task::none();
            }
            let ViewData {
                kind,
                content,
                thumbnails_seeded,
                ..
            } = view;
            let ViewKind::Home(home) = kind else {
                return Task::none();
            };
            match result {
                Ok(sections) => {
                    let mut rows = Vec::new();
                    for section in sections {
                        if section.key.provider != provider {
                            continue;
                        }
                        if section.key.kind == HomeSectionKind::QuickPicks {
                            // An empty row would replace the list with a blank
                            // page; keep whatever is already there instead.
                            if let HomeSectionData::Tracks(tracks) = &section.data {
                                if !tracks.is_empty() {
                                    *content = LoadState::Ready(tracks.clone());
                                    home.quick_picks_provider = Some(provider);
                                }
                            }
                        } else {
                            rows.push(section.clone());
                        }
                        cached_rows.push(section);
                    }
                    home.replace_provider(provider, rows);
                    if !cached_rows.is_empty() {
                        *thumbnails_seeded = false;
                    }
                }
                Err(error) => {
                    tracing::debug!("Home rows for {provider:?} failed: {error:#}");
                }
            }
            home.pending = home.pending.saturating_sub(1);
            providers::sort_sections(&mut home.sections, &providers);
        }

        if !cached_rows.is_empty() {
            for row in cached_rows {
                self.home_cache.insert(row.key, row.title, row.data, now);
            }
            self.home_cache.save();
        }
        self.seed_view_thumbnails(pane, idx);
        Task::none()
    }

    /// Play a home feed track. Home tiles aren't in the pane's track list, so
    /// the queue is built from the row they came from.
    pub fn handle_play_home_track(&mut self, pane: PaneId, source: HomeTrackSource, index: usize) {
        let mut tracks: Vec<Track> = match source {
            HomeTrackSource::Recent => self.queue.recently_played.iter().cloned().collect(),
            HomeTrackSource::Downloads => self
                .download_registry
                .tracks()
                .into_iter()
                .cloned()
                .collect(),
            HomeTrackSource::Section(key) => match &self.view_data_in(pane).kind {
                ViewKind::Home(home) => match home.section(key).map(|s| &s.data) {
                    Some(HomeSectionData::Tracks(tracks)) => tracks.clone(),
                    _ => return,
                },
                _ => return,
            },
        };
        let Some(track) = tracks.get(index).cloned() else {
            return;
        };
        if self.is_trashed(&track) {
            return;
        }
        self.set_queue(tracks.split_off(index));
        self.record_now_playing_origin();
    }
}
