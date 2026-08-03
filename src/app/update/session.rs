use super::*;

impl MusicPlayer {
    pub fn save_session(&self) {
        let state = SessionState {
            current_view: self.current_view.clone(),
            queue: self.queue.clone(),
            is_playing: self.is_playing,
            selected_playlist: self.selected_playlist,
            selected_playlist_name: self.selected_playlist_name.clone(),
            show_queue: self.show_queue,
        };
        state.save();
    }

    pub fn restore_session(&mut self) {
        let state = SessionState::load();
        self.current_view = state.current_view;
        self.queue = state.queue;
        self.is_playing = state.is_playing;
        self.selected_playlist = state.selected_playlist;
        self.selected_playlist_name = state.selected_playlist_name.clone();
        self.show_queue = state.show_queue;
        self.nav_history = vec![NavEntry {
            view: self.current_view.clone(),
            snapshot: self.snapshot_current(),
        }];
        self.nav_history_pos = 0;
    }

    pub fn resume_playback(&mut self) {
        if self.is_playing {
            if let Some(track) = self.queue.current() {
                let track = track.clone();
                self.play_track_internal(&track);
            } else {
                self.is_playing = false;
            }
        }
    }

    pub fn notify(&mut self, msg: String) {
        self.notification = Some(msg);
    }

    pub fn notify_error(&mut self, msg: String) {
        warn!("Backend error: {}", msg);
        self.notification = Some(msg);
    }

    pub fn clear_notification(&mut self) {
        self.notification = None;
    }
}