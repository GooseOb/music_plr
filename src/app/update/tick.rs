use super::{
    error, mpris, mpsc, spawn_thumbnail_download, BackendResult, MprisCommand, MprisUpdate,
    MusicPlayer, ViewData,
};
use crate::app::ViewKind;
use crate::data::cache::StreamCache;
use crate::data::JsonStore;
use crate::provider::ProviderId;
use tracing::debug;

/// Split a cache key of the form `"{provider:?}:{id}"` back into its parts.
fn parse_cache_key(key: &str) -> (ProviderId, String) {
    if let Some((p, id)) = key.split_once(':') {
        let provider = match p {
            "YouTube" => ProviderId::YouTube,
            "SoundCloud" => ProviderId::SoundCloud,
            "Jamendo" => ProviderId::Jamendo,
            "MusicBrainz" => ProviderId::MusicBrainz,
            _ => ProviderId::Local,
        };
        (provider, id.to_string())
    } else {
        (ProviderId::YouTube, key.to_string())
    }
}

impl MusicPlayer {
    pub fn init_mpris(&mut self) {
        let (mpris_update_tx, mpris_update_rx) = mpsc::channel();
        mpris::start(self.mpris_cmd_tx.clone(), mpris_update_rx);
        self.mpris_update_tx = Some(mpris_update_tx);
    }

    pub fn handle_tick(&mut self) {
        while let Ok(result) = self.result_rx.try_recv() {
            self.process_result(result);
        }

        while let Ok(cmd) = self.mpris_cmd_rx.try_recv() {
            self.process_mpris_command(cmd);
        }

        // Reconcile currently visible thumbnails: queue any missing ones for
        // download and flush the queue to a background thread.
        self.update_thumbnails();

        let s = self.audio.get_state();
        // Detect audio state changes for MPRIS update throttling.
        if self.is_playing != s.is_playing || (self.duration - s.duration).abs() > 0.001 {
            self.mpris_dirty = true;
        }
        self.is_playing = s.is_playing;
        self.progress = s.progress;
        self.duration = s.duration;
        if self.track_loading && s.is_playing {
            self.track_loading = false;
        }

        if let Some(pending) = self.pending_cache_id.clone() {
            // Register the cache as soon as the stream pipeline
            // finishes writing the file (`cache_ready`)
            if s.cache_ready {
                let (provider, id) = parse_cache_key(&pending);
                if self.stream_cache.insert(provider, &id) {
                    debug!("Registered cached track: {}", pending);
                }
                self.pending_cache_id = None;
                // The cache file is now complete: analyze it for volume
                // normalization if a fresh stream was awaiting this.
                if self.pending_normalization_id.as_deref() == Some(pending.as_str()) {
                    let path = StreamCache::path_for(provider, &id);
                    self.request_normalization_analysis(&pending, path);
                    self.pending_normalization_id = None;
                }
            }
        }

        if s.stream_finished && !s.is_playing && !self.track_loading {
            // When repeat is on, restart the current track instead of
            // advancing the queue so the same song loops until toggled off.
            if self.repeat {
                if let Some(track) = self.queue.current() {
                    let track = track.clone();
                    self.play_track_internal(&track, track.origin);
                }
                self.audio.clear_stream_finished();
            } else if self.queue.current().is_some() {
                self.next_track();
            } else {
                self.audio.clear_stream_finished();
            }
        }

        self.update_mpris_if_dirty();
        self.flush_session();

        if self.lyrics.is_some() {
            self.ensure_lyrics_for_current();
        }
    }

    fn update_mpris_if_dirty(&mut self) {
        if self.mpris_dirty {
            self.send_mpris_update();
            self.mpris_dirty = false;
        }
    }

    /// Seed a view's thumbnail ids into the index so the next tick drains any
    /// missing ones. Called wherever a view becomes active (navigation,
    /// results installed) — the tick only drains, it never re-scans visibility.
    pub(crate) fn seed_view_thumbnails(&mut self, view: &ViewData) {
        for track in &view.tracks {
            // Seed thumbnails for any track that carries a thumbnail URL,
            // regardless of provider (YouTube, SoundCloud, MusicBrainz, …).
            if !track.thumbnail.is_empty() {
                self.thumbnail_index
                    .ensure(track.primary_id(), &track.thumbnail);
            }
        }
        if let ViewKind::Search {
            tab:
                crate::provider::SearchTab::Artists(cards)
                | crate::provider::SearchTab::Albums(cards)
                | crate::provider::SearchTab::Playlists(cards),
            ..
        } = &view.kind
        {
            for card in cards {
                self.thumbnail_index.ensure(&card.id, &card.thumbnail);
            }
        }
    }

    fn update_thumbnails(&mut self) {
        if let Some(entries) = self.thumbnail_index.drain_pending() {
            spawn_thumbnail_download(entries, self.result_tx.clone());
        }
    }

    pub fn process_mpris_command(&mut self, cmd: MprisCommand) {
        match cmd {
            MprisCommand::TogglePlayPause => self.toggle_play_pause(),
            MprisCommand::NextTrack => self.next_track(),
            MprisCommand::PreviousTrack => self.previous_track(),
            MprisCommand::Stop => {
                if self.is_playing {
                    self.toggle_play_pause();
                }
            }
            MprisCommand::Play => {
                if !self.is_playing {
                    self.toggle_play_pause();
                }
            }
            MprisCommand::Pause => {
                if self.is_playing {
                    self.audio.pause();
                    self.mpris_dirty = true;
                }
            }
            MprisCommand::SetVolume(vol) => self.set_volume(vol),
            MprisCommand::Seek(delta_us) => {
                let delta_frac = delta_us as f32 / 1_000_000.0 / self.duration.max(0.001);
                let new_frac = (self.progress + delta_frac).clamp(0.0, 1.0);
                self.seek(new_frac);
            }
        }
    }

    fn install_results(&mut self, idx: usize, tracks: Vec<crate::types::Track>) {
        let slot = &mut self.nav_history[idx];
        slot.tracks = tracks;
        slot.loading = false;
        slot.selection.clear();
        slot.request_id = 0;
        self.finalize_view(idx);
    }

    fn finalize_view(&mut self, idx: usize) {
        self.save_session();
        self.seed_view_thumbnails(&self.nav_history[idx].clone());
        self.clear_notification();
    }

    fn process_search_results(
        &mut self,
        rid: u64,
        tracks: Vec<crate::types::Track>,
        tab: crate::provider::SearchTab,
    ) {
        // Apply to the slot that requested this search.
        if let Some(idx) = self.slot_for_request(rid) {
            if let ViewKind::Search {
                exhausted,
                tab: kind_tab,
                ..
            } = &mut self.nav_history[idx].kind
            {
                let count = if tab.is_track_tab() {
                    tracks.len()
                } else {
                    tab.len()
                };
                *exhausted = count < crate::theme::SEARCH_PAGE_SIZE;
                *kind_tab = tab;
                self.install_results(idx, tracks);
            }
        }
    }

    pub fn process_result(&mut self, result: BackendResult) {
        match result {
            BackendResult::SearchResults(rid, tracks, tab) => {
                self.process_search_results(rid, tracks, tab);
            }
            BackendResult::SearchResultsAppend(rid, tracks) => {
                let exhausted = tracks.len() < crate::theme::SEARCH_PAGE_SIZE;
                if let Some(idx) = self.slot_for_request(rid) {
                    self.nav_history[idx].tracks.extend(tracks);
                    self.nav_history[idx].loading = false;
                    self.nav_history[idx].set_exhausted(exhausted);
                    self.nav_history[idx].request_id = 0;
                    self.finalize_view(idx);
                }
            }
            BackendResult::BrowseResults(rid, tracks) => {
                // Apply to the slot that issued the browse, matched by request id
                if let Some(idx) = self.slot_for_request(rid) {
                    self.install_results(idx, tracks);
                }
            }
            BackendResult::CardPlaylistReady(idx, name, tracks) => {
                // A dragged card turned into a playlist; the browse result
                // fills it. The playlist view reads tracks from the store, so
                // they appear as soon as we insert them.
                if idx < self.playlists.playlists.len() {
                    let count = self.playlists.insert_tracks_at(idx, tracks.iter(), 0);
                    self.notify_tracks("Added", count, &format!("to {name}"));
                }
            }
            BackendResult::RadioResults(rid, label, tracks) => {
                if let Some(idx) = self.slot_for_request(rid) {
                    let kind = match self.nav_history[idx].kind {
                        ViewKind::ArtistRadio(_) => ViewKind::ArtistRadio(label),
                        _ => ViewKind::SongRadio(label),
                    };
                    self.nav_history[idx].kind = kind;
                    self.install_results(idx, tracks);
                }
            }
            BackendResult::DownloadComplete(track, _provider) => {
                let path = track.download_path.clone().unwrap_or_default();
                self.download_registry.register(track.clone());
                self.notify(format!("Download complete! Saved to {path}"));
                self.thumbnail_index.mark_downloaded(track.primary_id());
                if matches!(self.view_data().kind, ViewKind::Downloads) {
                    self.view_data_mut().tracks.push(track);
                }
            }
            BackendResult::DownloadError(msg) => {
                error!("Download error: {}", msg);
                self.notify_error(msg);
            }
            BackendResult::SearchError(msg) => {
                if matches!(
                    self.view_data().kind,
                    ViewKind::Search { .. } | ViewKind::SongRadio(_) | ViewKind::ArtistRadio(_)
                ) {
                    self.view_data_mut().loading = false;
                }
                self.clear_notification();
                self.notify_error(msg);
            }
            BackendResult::ProviderResolved {
                original,
                provider,
                id,
                pos,
            } => self.apply_provider_resolution(original, provider, id, pos, true),
            BackendResult::ProviderResolvedDownload {
                original,
                provider,
                id,
                pos,
            } => self.apply_provider_resolution(original, provider, id, pos, false),
            BackendResult::ThumbnailsDownloaded(ids) => {
                for id in &ids {
                    self.thumbnail_index.mark_downloaded(id);
                }
            }
            BackendResult::NormalizationComputed(id, gain) => {
                self.normalization_cache.insert(id, gain);
            }
            BackendResult::LyricsFetched(lyrics, track_id) => {
                if track_id.is_empty() {
                    return;
                }
                let Some(state) = &mut self.lyrics else {
                    return;
                };
                if state.track_id.as_deref() != Some(track_id.as_str()) {
                    return;
                }
                state.loading = false;
                state.lyrics = lyrics;
                state.track_id = Some(track_id);
                state.not_found = state.lyrics.is_none();
                if let Some(lyrics) = &state.lyrics {
                    let mut cache = crate::data::lyrics_cache::LyricsCache::load();
                    cache.insert(state.track_id.as_ref().unwrap(), lyrics);
                }
                self.sync_lyrics_editor();
            }
        }
    }

    /// Apply a resolved provider id to `original`: write it back into the
    /// source list, then either play (replacing the queue) or download.
    fn apply_provider_resolution(
        &mut self,
        mut original: crate::types::Track,
        provider: crate::provider::ProviderId,
        id: Option<(String, String)>,
        pos: Option<crate::app::interaction::TrackPos>,
        play: bool,
    ) {
        match id {
            Some((resolved_id, url)) => {
                original.set_provider(
                    provider,
                    crate::types::ProviderTrack {
                        id: resolved_id,
                        url,
                        artist_id: None,
                    },
                );
                if let Some(p) = pos {
                    self.set_track_at(p, original.clone());
                }
                if play {
                    self.play_track_replacing_queue(original, provider);
                    if let Some(p) = pos {
                        for t in self.tracks_after(p.index).to_vec() {
                            self.queue.enqueue(t);
                        }
                    }
                    self.save_session();
                } else {
                    self.spawn_download_thread_for(provider, original);
                }
            }
            None => {
                self.notify_error(format!(
                    "Could not find \"{}\" on {}",
                    original.title,
                    provider.label()
                ));
            }
        }
    }

    pub fn send_mpris_update(&self) {
        if let Some(ref tx) = self.mpris_update_tx {
            let track = self.queue.current();
            let update = MprisUpdate {
                playback_status: if self.is_playing {
                    "Playing"
                } else if track.is_some() {
                    "Paused"
                } else {
                    "Stopped"
                }
                .into(),
                title: track.map(|t| t.title.clone()).unwrap_or_default(),
                artist: track.map(|t| t.artist.name.clone()).unwrap_or_default(),
                duration_secs: self.duration,
                position_us: (self.progress * self.duration * 1_000_000.0) as i64,
                volume: self.volume,
                has_track: track.is_some(),
            };
            let _ = tx.send(update);
        }
    }
}
