use super::{
    error, format_duration, mpris, mpsc, spawn_thumbnail_download_thread, BackendResult,
    MprisCommand, MprisUpdate, MusicPlayer, View,
};
use crate::types::Track;
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

        self.update_thumbnail_cache();

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
            if s.stream_finished {
                if self.stream_cache.path_for(&pending).exists()
                    && self.stream_cache.insert(&pending)
                {
                    debug!("Registered cached track: {}", pending);
                }
                self.pending_cache_id = None;
            }
        }

        if s.stream_finished && !s.is_playing && !self.track_loading {
            self.next_track();
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

    fn update_thumbnail_cache(&mut self) {
        if matches!(self.current_view, View::Downloads) {
            self.downloaded_tracks = self
                .download_registry
                .all_tracks()
                .into_iter()
                .cloned()
                .collect();
        }

        let tracks: Vec<&Track> = match self.current_view {
            View::Search => self.search_results.iter().collect(),
            View::SongRadio | View::ArtistRadio => self.radio_tracks.iter().collect(),
            View::Playlist => {
                if let Some(idx) = self.selected_playlist {
                    self.playlists
                        .playlists
                        .get(idx)
                        .map(|pl| pl.tracks.iter().collect())
                        .unwrap_or_default()
                } else {
                    Vec::new()
                }
            }
            View::Downloads => self.downloaded_tracks.iter().collect(),
        };

        for track in &tracks {
            let id = &track.id;
            if !self.thumbnail_cache.contains_key(id) {
                let exists = crate::thumbnails::thumbnail_path(id).exists();
                self.thumbnail_cache.insert(id.clone(), exists);
            }
        }

        if let Some(current) = self.queue.current() {
            let id = &current.id;
            if !self.thumbnail_cache.contains_key(id) {
                let exists = crate::thumbnails::thumbnail_path(id).exists();
                self.thumbnail_cache.insert(id.clone(), exists);
            }
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

    pub fn update_progress_text(&mut self) {
        let elapsed = (self.progress * self.duration) as u32;
        self.elapsed_text = format_duration(elapsed);
        let total = self.duration as u32;
        self.total_text = format_duration(total);
    }

    pub fn process_result(&mut self, result: BackendResult) {
        match result {
            BackendResult::SearchResults(tracks) => {
                self.search_results = tracks;
                self.search_exhausted = self.search_results.len() < crate::theme::SEARCH_PAGE_SIZE;
                self.search_loading = false;
                if matches!(self.current_view, View::Search) {
                    self.push_nav_entry();
                }
                self.save_session();
                spawn_thumbnail_download_thread(&self.search_results, self.result_tx.clone());
                self.clear_notification();
            }
            BackendResult::SearchResultsAppend(tracks) => {
                let exhausted = tracks.len() < crate::theme::SEARCH_PAGE_SIZE;
                self.search_results.extend(tracks);
                self.search_loading = false;
                self.search_exhausted = exhausted;
                let _ = self.update_current_snapshot();
                spawn_thumbnail_download_thread(&self.search_results, self.result_tx.clone());
                self.clear_notification();
                self.save_session();
            }
            BackendResult::RadioResults(label, tracks) => {
                self.radio_label = label;
                self.radio_tracks = tracks;
                self.search_loading = false;
                if matches!(self.current_view, View::SongRadio | View::ArtistRadio)
                    && !self.update_current_snapshot()
                {
                    self.push_nav_entry();
                }
                self.save_session();
                spawn_thumbnail_download_thread(&self.radio_tracks, self.result_tx.clone());
            }
            BackendResult::DownloadComplete(track, path) => {
                self.download_registry.register(track);
                self.notify(format!("Download complete! Saved to {path}"));
                self.thumbnail_cache.clear();
            }
            BackendResult::DownloadError(msg) => {
                error!("Download error: {}", msg);
                self.notify_error(msg);
            }
            BackendResult::SearchError(msg) => {
                self.search_loading = false;
                self.clear_notification();
                self.notify_error(msg);
            }
            BackendResult::ThumbnailsDownloaded => {
                self.thumbnail_cache.clear();
            }
        }
    }

    pub fn send_mpris_update(&self) {
        if let Some(ref tx) = self.mpris_update_tx {
            let track = self.queue.current();
            let update = MprisUpdate {
                playback_status: if self.is_playing {
                    "Playing".into()
                } else if track.is_some() {
                    "Paused".into()
                } else {
                    "Stopped".into()
                },
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
