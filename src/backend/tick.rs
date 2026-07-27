use super::{to_slint_track, BackendResult, PlaylistInfo, Track, View};
use crate::mpris::{MprisCommand, MprisUpdate};
use std::rc::Rc;

impl super::Backend {
    pub fn tick(&mut self) {
        let s = self.audio.get_state();
        self.is_playing = s.is_playing;
        self.progress = s.progress;
        self.duration = s.duration;
        if self.track_loading && s.is_playing {
            self.track_loading = false;
        }

        while let Ok(cmd) = self.mpris_cmd_rx.try_recv() {
            match cmd {
                MprisCommand::TogglePlayPause => {
                    if self.is_playing {
                        self.audio.pause();
                        self.is_playing = false;
                    } else if self.queue.current().is_some() {
                        self.audio.resume();
                        self.is_playing = true;
                    }
                }
                MprisCommand::NextTrack => {
                    if self.queue.next().is_some() {
                        if let Some(track) = self.queue.current().cloned() {
                            self.play_track_internal(&track);
                        }
                    }
                }
                MprisCommand::PreviousTrack => {
                    if self.queue.previous().is_some() {
                        if let Some(track) = self.queue.current().cloned() {
                            self.play_track_internal(&track);
                        }
                    }
                }
                MprisCommand::SetVolume(vol) => {
                    self.volume = vol;
                    self.config.volume = vol;
                    self.audio.set_volume(vol);
                    crate::config::save_config(&self.config);
                }
                MprisCommand::Seek(delta_us) => {
                    let current_progress = self.progress;
                    let delta_frac = delta_us as f32 / 1_000_000.0 / self.duration.max(0.001);
                    let new_frac = (current_progress + delta_frac).clamp(0.0, 1.0);
                    self.handle_seek(new_frac);
                }
            }
        }

        let mut need_search_sync = false;
        let mut need_radio_sync = false;
        let mut need_all_sync = false;
        while let Ok(result) = self.result_rx.try_recv() {
            match result {
                BackendResult::SearchResults(tracks) => {
                    self.search_results = tracks;
                    self.search_offset = self.search_results.len();
                    self.loading = false;
                    need_search_sync = true;
                }
                BackendResult::SearchResultsAppend(tracks) => {
                    self.search_offset += tracks.len();
                    self.search_results.extend(tracks);
                    self.loading = false;
                    need_search_sync = true;
                }
                BackendResult::RadioResults(label, tracks) => {
                    self.radio_label = label;
                    self.radio_tracks = tracks;
                    self.loading = false;
                    self.current_view = View::Radio;
                    need_radio_sync = true;
                }
                BackendResult::DownloadComplete(_idx, url, path) => {
                    self.downloading_index = None;
                    self.download_registry.register(&url, &path);
                    self.notification = Some("Download complete!".into());
                    need_all_sync = true;
                }
                BackendResult::DownloadError(msg) => {
                    self.downloading_index = None;
                    eprintln!("[backend] Download error: {}", msg);
                }
            }
        }

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

        let showing = self
            .ui
            .upgrade()
            .map(|w| w.get_show_search_history())
            .unwrap_or(false);

        if let Some(window) = self.ui.upgrade() {
            let has_focus = window.get_search_has_focus();
            if has_focus && !showing {
                self.update_search_history(&window);
                window.set_show_search_history(true);
            } else if !has_focus && showing {
                self.focus_lost_ticks += 1;
                if self.focus_lost_ticks >= 2 {
                    window.set_show_search_history(false);
                    self.focus_lost_ticks = 0;
                }
            } else if has_focus {
                self.focus_lost_ticks = 0;
            }
        }

        if let Some(text) = self.pending_search_text.take() {
            eprintln!("[tick.pending] text='{}'", text);
            if let Some(w) = self.ui.upgrade() {
                eprintln!("[tick.pending] calling set_search_input_text");
                w.set_search_input_text(text.into());
                w.set_show_search_history(false);
            }
            self.handle_search_execute();
        }

        if need_all_sync {
            self.sync_search_model();
            self.sync_radio_model();
            self.sync_playlist_content();
        } else {
            if need_search_sync {
                self.sync_search_model();
            }
            if need_radio_sync {
                self.sync_radio_model();
            }
        }
        if need_search_sync || need_radio_sync || need_all_sync {
            self.update_playback_ui();
        } else if !showing {
            self.update_playback_ui();
        }
    }

    pub fn update_playback_ui(&mut self) {
        let Some(window) = self.ui.upgrade() else {
            return;
        };
        let cached = &mut self.cached_ui;

        let view = self.current_view as i32;
        if cached.current_view != view {
            cached.current_view = view;
            window.set_current_view(view);
        }

        if cached.is_playing != self.is_playing {
            cached.is_playing = self.is_playing;
            window.set_is_playing(self.is_playing);
        }

        if cached.progress != self.progress {
            cached.progress = self.progress;
            window.set_progress(self.progress);
        }

        if cached.duration_secs != self.duration {
            cached.duration_secs = self.duration;
            window.set_duration_secs(self.duration);
        }

        if cached.volume != self.volume {
            cached.volume = self.volume;
            window.set_volume(self.volume);
        }

        if cached.track_loading != self.track_loading {
            cached.track_loading = self.track_loading;
            window.set_track_loading(self.track_loading);
        }

        let can_back = self.nav_history_pos > 0;
        if cached.can_go_back != can_back {
            cached.can_go_back = can_back;
            window.set_can_go_back(can_back);
        }

        let can_forward = self.nav_history_pos + 1 < self.nav_history.len();
        if cached.can_go_forward != can_forward {
            cached.can_go_forward = can_forward;
            window.set_can_go_forward(can_forward);
        }

        if let Some(track) = self.queue.current() {
            let title: String = track.title.clone();
            if cached.current_title != title {
                cached.current_title = title.clone();
                window.set_current_title(title.into());
            }
            let artist: String = track.artist.clone();
            if cached.current_artist != artist {
                cached.current_artist = artist.clone();
                window.set_current_artist(artist.into());
            }
            let id: String = track.id.clone();
            if cached.current_track_id != id {
                cached.current_track_id = id.clone();
                window.set_current_track_id(id.into());
            }
        } else {
            if !cached.current_title.is_empty() {
                cached.current_title.clear();
                window.set_current_title("".into());
            }
            if !cached.current_artist.is_empty() {
                cached.current_artist.clear();
                window.set_current_artist("".into());
            }
            if !cached.current_track_id.is_empty() {
                cached.current_track_id.clear();
                window.set_current_track_id("".into());
            }
        }

        let elapsed = (self.progress * self.duration) as u32;
        let elapsed_str = format!("{}:{:02}", elapsed / 60, elapsed % 60);
        if cached.elapsed_text != elapsed_str {
            cached.elapsed_text = elapsed_str.clone();
            window.set_elapsed_text(elapsed_str.into());
        }

        let total = self.duration as u32;
        let total_str = format!("{}:{:02}", total / 60, total % 60);
        if cached.total_text != total_str {
            cached.total_text = total_str.clone();
            window.set_total_text(total_str.into());
        }

        if cached.loading != self.loading {
            cached.loading = self.loading;
            window.set_loading(self.loading);
        }

        let notif: String = self.notification.as_deref().unwrap_or("").into();
        if cached.notification != notif {
            cached.notification = notif.clone();
            window.set_notification(notif.into());
        }
    }

    pub fn sync_search_model(&mut self) {
        let Some(window) = self.ui.upgrade() else {
            return;
        };
        let registry = &self.download_registry;
        let model: Vec<Track> = self
            .search_results
            .iter()
            .enumerate()
            .map(|(i, t)| to_slint_track(t, registry, self.is_selected(i)))
            .collect();
        let rc = Rc::new(slint::VecModel::from(model));
        self.search_model_handle = Some(rc.clone());
        window.set_search_results(rc.into());
    }

    pub fn sync_radio_model(&mut self) {
        let Some(window) = self.ui.upgrade() else {
            return;
        };
        let registry = &self.download_registry;
        let model: Vec<Track> = self
            .radio_tracks
            .iter()
            .enumerate()
            .map(|(i, t)| to_slint_track(t, registry, self.is_selected(i)))
            .collect();
        let rc = Rc::new(slint::VecModel::from(model));
        self.radio_model_handle = Some(rc.clone());
        window.set_radio_tracks(rc.into());
    }

    pub fn sync_playlist_sidebar(&mut self) {
        let Some(window) = self.ui.upgrade() else {
            return;
        };
        let list: Vec<PlaylistInfo> = self
            .playlists
            .playlists
            .iter()
            .map(|pl| PlaylistInfo {
                name: pl.name.clone().into(),
                track_count: pl.tracks.len() as i32,
            })
            .collect();
        window.set_playlist_list(Rc::new(slint::VecModel::from(list)).into());
        let names: Vec<slint::SharedString> = self
            .playlists
            .playlists
            .iter()
            .map(|pl| pl.name.clone().into())
            .collect();
        window.set_picker_playlists(Rc::new(slint::VecModel::from(names)).into());
    }

    pub fn sync_playlist_content(&mut self) {
        let Some(window) = self.ui.upgrade() else {
            return;
        };
        let registry = &self.download_registry;
        if let Some(idx) = self.selected_playlist {
            if let Some(pl) = self.playlists.playlists.get(idx) {
                let model: Vec<Track> = pl
                    .tracks
                    .iter()
                    .enumerate()
                    .map(|(i, t)| to_slint_track(t, registry, self.is_selected(i)))
                    .collect();
                let rc = Rc::new(slint::VecModel::from(model));
                self.playlist_model_handle = Some(rc.clone());
                window.set_playlist_tracks(rc.into());
                window.set_selected_playlist_name(self.selected_playlist_name.clone().into());
                window.set_selected_playlist(idx as i32);
                window.set_playlist_create_name(self.playlist_create_name.clone().into());
                return;
            }
        }
        self.playlist_model_handle = None;
        window.set_selected_playlist(-1);
        window.set_selected_playlist_name("".into());
        window.set_playlist_tracks(Rc::new(slint::VecModel::<Track>::from(vec![])).into());
        window.set_playlist_create_name(self.playlist_create_name.clone().into());
    }

    pub fn update_ui(&mut self) {
        self.update_playback_ui();
        self.sync_search_model();
        self.sync_radio_model();
        self.sync_playlist_sidebar();
        self.sync_playlist_content();
        if let Some(window) = self.ui.upgrade() {
            self.update_search_history(&window);
        }
    }

    pub fn handle_navigate_to(&mut self, view: View) {
        self.clear_selection();
        self.nav_history.truncate(self.nav_history_pos + 1);
        self.nav_history.push(view);
        self.nav_history_pos = self.nav_history.len() - 1;
        self.current_view = view;
    }

    pub fn handle_navigate_back(&mut self) {
        if self.nav_history_pos > 0 {
            self.nav_history_pos -= 1;
            self.current_view = self.nav_history[self.nav_history_pos];
        }
    }

    pub fn handle_navigate_forward(&mut self) {
        if self.nav_history_pos + 1 < self.nav_history.len() {
            self.nav_history_pos += 1;
            self.current_view = self.nav_history[self.nav_history_pos];
        }
    }
}
