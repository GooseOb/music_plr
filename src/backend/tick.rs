use super::{to_slint_track, PlaylistInfo, Track, View};
use std::rc::Rc;

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

        self.send_mpris_update();

        if let Some(window) = self.ui.upgrade() {
            window.set_is_playing(self.is_playing);
            window.set_progress(self.progress);
            window.set_duration_secs(self.duration);
            window.set_track_loading(self.track_loading);
            let elapsed = (self.progress * self.duration) as u32;
            window.set_elapsed_text(format!("{}:{:02}", elapsed / 60, elapsed % 60).into());
            let total = self.duration as u32;
            window.set_total_text(format!("{}:{:02}", total / 60, total % 60).into());
        }

        if let Some(window) = self.ui.upgrade() {
            let showing = window.get_show_search_history();
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
    }

    pub fn sync_current_track_ui(&mut self) {
        let Some(window) = self.ui.upgrade() else {
            return;
        };
        if let Some(track) = self.queue.current() {
            window.set_current_title(track.title.clone().into());
            window.set_current_artist(track.artist.clone().into());
            window.set_current_track_id(track.id.clone().into());
        } else {
            window.set_current_title("".into());
            window.set_current_artist("".into());
            window.set_current_track_id("".into());
        }
        window.set_is_playing(self.is_playing);
        window.set_duration_secs(self.duration);
        window.set_track_loading(self.track_loading);
        window.set_progress(self.progress);
        let elapsed = (self.progress * self.duration) as u32;
        window.set_elapsed_text(format!("{}:{:02}", elapsed / 60, elapsed % 60).into());
        let total = self.duration as u32;
        window.set_total_text(format!("{}:{:02}", total / 60, total % 60).into());
    }

    pub fn update_nav_ui(&mut self) {
        let Some(window) = self.ui.upgrade() else {
            return;
        };
        window.set_current_view(self.current_view as i32);
        window.set_can_go_back(self.nav_history_pos > 0);
        window.set_can_go_forward(self.nav_history_pos + 1 < self.nav_history.len());
    }

    pub fn notify(&mut self, msg: String) {
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
        self.sync_current_track_ui();
        self.update_nav_ui();
        if let Some(window) = self.ui.upgrade() {
            window.set_volume(self.volume);
            window.set_loading(self.loading);
            window.set_notification(self.notification.as_deref().unwrap_or("").into());
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
    }

    pub fn handle_navigate_back(&mut self) {
        if self.nav_history_pos > 0 {
            self.nav_history_pos -= 1;
            self.current_view = self.nav_history[self.nav_history_pos];
            self.update_nav_ui();
        }
    }

    pub fn handle_navigate_forward(&mut self) {
        if self.nav_history_pos + 1 < self.nav_history.len() {
            self.nav_history_pos += 1;
            self.current_view = self.nav_history[self.nav_history_pos];
            self.update_nav_ui();
        }
    }
}
