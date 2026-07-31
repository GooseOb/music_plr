use super::{
    format_duration, to_slint_track, NavigationState, PlaybackState, PlaylistInfo, PlaylistState,
    QueueState, SearchState, Track, View,
};
use slint::ComponentHandle;
use std::rc::Rc;
use tracing::{debug, warn};

fn build_track_model(
    tracks: &[super::RustTrack],
    registry: &super::DownloadRegistry,
    selected_indices: &[usize],
    downloading_index: Option<usize>,
) -> Rc<slint::VecModel<Track>> {
    let model: Vec<Track> = tracks
        .iter()
        .enumerate()
        .map(|(i, t)| {
            to_slint_track(
                t,
                registry,
                selected_indices.contains(&i),
                downloading_index == Some(i),
            )
        })
        .collect();
    Rc::new(slint::VecModel::from(model))
}

impl super::Backend {
    pub fn audio_tick(&mut self) {
        while let Ok(f) = self.event_rx.try_recv() {
            f(self);
        }

        let s = self.audio.get_state();
        self.is_playing = s.is_playing;
        self.progress = s.progress;
        self.duration = s.duration;
        if self.track_loading && s.is_playing {
            self.track_loading = false;
        }

        if let Some(ref pending) = self.pending_cache_id.clone() {
            if s.stream_finished {
                if self.stream_cache.path_for(pending).exists() && self.stream_cache.insert(pending)
                {
                    debug!("Registered cached track: {}", pending);
                }
                self.pending_cache_id = None;
            }
        }

        if s.stream_finished && !s.is_playing && !self.track_loading {
            self.handle_next_track();
        }

        self.send_mpris_update();

        if let Some(window) = self.ui.upgrade() {
            let playback = window.global::<PlaybackState>();
            playback.set_is_playing(self.is_playing);
            playback.set_progress(self.progress);
            playback.set_duration_secs(self.duration);
            playback.set_track_loading(self.track_loading);
            let elapsed = (self.progress * self.duration) as u32;
            playback.set_elapsed_text(format_duration(elapsed).into());
            let total = self.duration as u32;
            playback.set_total_text(format_duration(total).into());
        }

        if let Some(window) = self.ui.upgrade() {
            let search_state = window.global::<SearchState>();
            let showing = search_state.get_show_search_history();
            let has_focus = window.get_search_has_focus();
            if has_focus && !showing {
                self.update_search_history(&window);
                search_state.set_show_search_history(true);
            } else if !has_focus && showing {
                self.focus_lost_ticks += 1;
                if self.focus_lost_ticks >= 2 {
                    search_state.set_show_search_history(false);
                    self.focus_lost_ticks = 0;
                }
            } else if has_focus {
                self.focus_lost_ticks = 0;
            }
        }
    }

    pub fn sync_current_track_ui(&mut self) {
        let Some(window) = self.ui.upgrade() else {
            return;
        };
        let playback = window.global::<PlaybackState>();
        if let Some(track) = self.queue.current() {
            playback.set_current_title(track.title.clone().into());
            playback.set_current_artist(track.artist.clone().into());
            playback.set_current_track_id(track.id.clone().into());
            let thumb_img = if track.source == crate::types::TrackSource::YouTube {
                let cached = crate::thumbnails::thumbnail_path(&track.id);
                if cached.exists() {
                    slint::Image::load_from_path(&cached).unwrap_or_default()
                } else {
                    slint::Image::default()
                }
            } else {
                slint::Image::default()
            };
            playback.set_current_thumbnail(thumb_img);
        } else {
            playback.set_current_title("".into());
            playback.set_current_artist("".into());
            playback.set_current_track_id("".into());
            playback.set_current_thumbnail(slint::Image::default());
        }
        playback.set_is_playing(self.is_playing);
        playback.set_duration_secs(self.duration);
        playback.set_track_loading(self.track_loading);
        playback.set_progress(self.progress);
        let elapsed = (self.progress * self.duration) as u32;
        playback.set_elapsed_text(format_duration(elapsed).into());
        let total = self.duration as u32;
        playback.set_total_text(format_duration(total).into());
        self.sync_queue_ui();
    }

    pub fn sync_queue_ui(&mut self) {
        let Some(window) = self.ui.upgrade() else {
            return;
        };
        let upcoming: Vec<super::RustTrack> = self
            .queue
            .tracks
            .iter()
            .skip(self.queue.current_index + 1)
            .cloned()
            .collect();
        let rc = build_track_model(
            &upcoming,
            &self.download_registry,
            &self.selected_indices,
            self.downloading_index,
        );
        self.queue_model_handle = Some(rc.clone());
        window.global::<QueueState>().set_queue_tracks(rc.into());
    }

    pub fn update_nav_ui(&mut self) {
        let Some(window) = self.ui.upgrade() else {
            return;
        };
        window.set_current_view(self.current_view as i32);
        let nav = window.global::<NavigationState>();
        nav.set_can_go_back(self.nav_history_pos > 0);
        nav.set_can_go_forward(self.nav_history_pos + 1 < self.nav_history.len());
    }

    pub fn notify(&mut self, msg: String) {
        self.notification = Some(msg.clone());
        if let Some(window) = self.ui.upgrade() {
            window.set_notification(msg.into());
        }
    }

    pub fn notify_error(&mut self, msg: String) {
        warn!("Backend error: {}", msg);
        self.notification = Some(msg.clone());
        if let Some(window) = self.ui.upgrade() {
            window.set_notification(msg.into());
        }
    }

    pub fn clear_notification(&mut self) {
        self.notification = None;
        if let Some(window) = self.ui.upgrade() {
            window.set_notification("".into());
        }
    }

    pub fn sync_search_model(&mut self) {
        let Some(window) = self.ui.upgrade() else {
            return;
        };
        let rc = build_track_model(
            &self.search_results,
            &self.download_registry,
            &self.selected_indices,
            self.downloading_index,
        );
        self.search_model_handle = Some(rc.clone());
        window.global::<SearchState>().set_search_results(rc.into());
    }

    pub fn sync_radio_model(&mut self) {
        let Some(window) = self.ui.upgrade() else {
            return;
        };
        let rc = build_track_model(
            &self.radio_tracks,
            &self.download_registry,
            &self.selected_indices,
            self.downloading_index,
        );
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
        let playlist_state = window.global::<PlaylistState>();
        playlist_state.set_playlist_list(Rc::new(slint::VecModel::from(list)).into());
        let names: Vec<slint::SharedString> = self
            .playlists
            .playlists
            .iter()
            .map(|pl| pl.name.clone().into())
            .collect();
        playlist_state.set_picker_playlists(Rc::new(slint::VecModel::from(names)).into());
    }

    pub fn sync_playlist_content(&mut self) {
        let Some(window) = self.ui.upgrade() else {
            return;
        };
        if let Some(idx) = self.selected_playlist {
            if let Some(pl) = self.playlists.playlists.get(idx) {
                let rc = build_track_model(
                    &pl.tracks,
                    &self.download_registry,
                    &self.selected_indices,
                    self.downloading_index,
                );
                self.playlist_model_handle = Some(rc.clone());
                let playlist_state = window.global::<PlaylistState>();
                playlist_state.set_playlist_tracks(rc.into());
                playlist_state
                    .set_selected_playlist_name(self.selected_playlist_name.clone().into());
                playlist_state.set_selected_playlist(idx as i32);
                playlist_state.set_playlist_create_name(self.playlist_create_name.clone().into());
                return;
            }
        }
        self.playlist_model_handle = None;
        let playlist_state = window.global::<PlaylistState>();
        playlist_state.set_selected_playlist(-1);
        playlist_state.set_selected_playlist_name("".into());
        playlist_state.set_playlist_tracks(Rc::new(slint::VecModel::<Track>::from(vec![])).into());
        playlist_state.set_playlist_create_name(self.playlist_create_name.clone().into());
    }

    pub fn update_ui(&mut self) {
        self.sync_current_track_ui();
        self.update_nav_ui();
        if let Some(window) = self.ui.upgrade() {
            let playback = window.global::<PlaybackState>();
            playback.set_volume(self.volume);
            window.global::<SearchState>().set_loading(self.loading);
            window.set_notification(self.notification.as_deref().unwrap_or("").into());
            window
                .global::<QueueState>()
                .set_show_queue(self.show_queue);
        }
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
        self.update_nav_ui();
        self.save_session();
    }

    pub fn handle_navigate_back(&mut self) {
        if self.nav_history_pos > 0 {
            self.nav_history_pos -= 1;
            self.current_view = self.nav_history[self.nav_history_pos];
            self.update_nav_ui();
            self.save_session();
        }
    }

    pub fn handle_navigate_forward(&mut self) {
        if self.nav_history_pos + 1 < self.nav_history.len() {
            self.nav_history_pos += 1;
            self.current_view = self.nav_history[self.nav_history_pos];
            self.update_nav_ui();
            self.save_session();
        }
    }
}
