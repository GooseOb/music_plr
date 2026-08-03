use super::*;
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

        let s = self.audio.get_state();
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

        self.send_mpris_update();
        self.update_progress_text();
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
                    self.send_mpris_update();
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
                spawn_thumbnail_download_thread(&self.search_results);
                self.clear_notification();
            }
            BackendResult::SearchResultsAppend(tracks) => {
                let exhausted = tracks.len() < crate::theme::SEARCH_PAGE_SIZE;
                self.search_results.extend(tracks);
                self.search_loading = false;
                self.search_exhausted = exhausted;
                if let Some(entry) = self.nav_history.get_mut(self.nav_history_pos) {
                    if let ViewSnapshot::Search { results, .. } = &mut entry.snapshot {
                        *results = self.search_results.clone();
                    }
                }
                spawn_thumbnail_download_thread(&self.search_results);
                self.clear_notification();
            }
            BackendResult::RadioResults(label, tracks) => {
                self.radio_label = label;
                self.radio_tracks = tracks;
                self.search_loading = false;
                if matches!(self.current_view, View::SongRadio | View::ArtistRadio) {
                    self.push_nav_entry();
                }
                self.save_session();
                spawn_thumbnail_download_thread(&self.radio_tracks);
            }
            BackendResult::DownloadComplete(url, path) => {
                self.downloading_index = None;
                self.download_registry.register(&url, &path);
                self.notify("Download complete!".into());
            }
            BackendResult::DownloadError(msg) => {
                self.downloading_index = None;
                error!("Download error: {}", msg);
                self.notify_error(msg);
            }
            BackendResult::SearchError(msg) => {
                self.search_loading = false;
                self.clear_notification();
                self.notify_error(msg);
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
