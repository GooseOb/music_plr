use super::{
    error, format_duration, mpris, mpsc, spawn_thumbnail_download, BackendResult, MprisCommand,
    MprisUpdate, MusicPlayer, ViewData,
};
use crate::app::ViewKind;
use tracing::debug;

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
                if self.stream_cache.insert(&pending) {
                    debug!("Registered cached track: {}", pending);
                }
                self.pending_cache_id = None;
            }
        }

        if s.stream_finished && !s.is_playing && !self.track_loading {
            // Don't busy-loop if there is nothing left to advance to (e.g. a
            // corrupt cached file that emptied without a next track); clear
            // the finished flag so we don't repeatedly call `next_track`.
            if self.queue.current().is_some() {
                self.next_track();
            } else {
                self.audio.clear_stream_finished();
            }
        }

        self.update_mpris_if_dirty();
        self.update_progress_text();
        self.flush_session();
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
            if track.source == crate::types::TrackSource::YouTube {
                self.thumbnail_index.ensure(&track.id, &track.thumbnail);
            }
        }
        if let ViewKind::Search {
            tab:
                crate::youtube::SearchTab::Artists(cards)
                | crate::youtube::SearchTab::Albums(cards)
                | crate::youtube::SearchTab::Playlists(cards),
            ..
        } = &view.kind
        {
            for card in cards {
                self.thumbnail_index.ensure(&card.id, &card.thumbnail);
            }
        }
    }

    fn update_thumbnails(&mut self) {
        // Flush queued thumbnail downloads. Ids are seeded into the index at the
        // points they become visible (search/browse/radio results, download
        // completion, navigation, queue advance) — the tick only drains, it
        // does not re-scan visibility.
        if let Some(entries) = self.thumbnail_index.drain_pending() {
            spawn_thumbnail_download(entries, self.result_tx.clone());
        }
    }

    #[allow(clippy::needless_pass_by_value)]
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

    /// Handle a fresh `SearchResults`: install the tab into the requesting
    /// nav slot, flip exhausted/loading flags. Stale results (slot truncated
    /// away by navigation) are ignored. Thumbnail downloads are driven lazily
    /// by the tick over currently visible ids, so none are kicked off here.
    fn process_search_results(
        &mut self,
        rid: u64,
        tracks: Vec<crate::types::Track>,
        tab: crate::youtube::SearchTab,
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
                self.nav_history[idx].tracks = tracks;
                self.nav_history[idx].loading = false;
                self.nav_history[idx].selection.clear();
                self.nav_history[idx].request_id = 0;
            }
            self.save_session();
            self.seed_view_thumbnails(&self.nav_history[idx].clone());
            self.clear_notification();
        }
    }

    pub fn update_progress_text(&mut self) {
        let elapsed = (self.progress * self.duration) as u32;
        self.elapsed_text = format_duration(elapsed);
        let total = self.duration as u32;
        self.total_text = format_duration(total);
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
                    self.save_session();
                    self.seed_view_thumbnails(&self.nav_history[idx].clone());
                    self.clear_notification();
                }
            }
            BackendResult::BrowseResults(rid, tracks) => {
                // Apply to the slot that issued the browse, matched by request id
                if let Some(idx) = self.slot_for_request(rid) {
                    self.nav_history[idx].tracks = tracks;
                    self.nav_history[idx].loading = false;
                    self.nav_history[idx].selection.clear();
                    self.nav_history[idx].request_id = 0;
                    self.save_session();
                    self.seed_view_thumbnails(&self.nav_history[idx].clone());
                    self.clear_notification();
                }
            }
            BackendResult::RadioResults(rid, label, tracks) => {
                if let Some(idx) = self.slot_for_request(rid) {
                    let kind = match self.nav_history[idx].kind {
                        ViewKind::ArtistRadio(_) => ViewKind::ArtistRadio(label),
                        _ => ViewKind::SongRadio(label),
                    };
                    self.nav_history[idx].kind = kind;
                    self.nav_history[idx].tracks = tracks;
                    self.nav_history[idx].loading = false;
                    self.nav_history[idx].request_id = 0;
                    self.save_session();
                    self.seed_view_thumbnails(&self.nav_history[idx].clone());
                }
            }
            BackendResult::DownloadComplete(track, path) => {
                let mut track = track;
                track.download_path = Some(path.clone());
                self.download_registry.register(track.clone());
                self.notify(format!("Download complete! Saved to {path}"));
                self.thumbnail_index.mark_downloaded(&track.id);
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
            BackendResult::ThumbnailsDownloaded(ids) => {
                for id in &ids {
                    self.thumbnail_index.mark_downloaded(id);
                }
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
                artist: track.map(|t| t.artist.clone()).unwrap_or_default(),
                duration_secs: self.duration,
                position_us: (self.progress * self.duration * 1_000_000.0) as i64,
                volume: self.volume,
                has_track: track.is_some(),
            };
            let _ = tx.send(update);
        }
    }
}
