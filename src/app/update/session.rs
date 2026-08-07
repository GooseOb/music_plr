use super::{warn, MusicPlayer, NavEntry};
use crate::session::SessionState;

impl MusicPlayer {
    pub fn save_session(&mut self) {
        self.session_dirty = true;
    }

    pub fn flush_session(&mut self) {
        if self.session_dirty {
            let state = SessionState {
                data: self.view_data.clone(),
                queue: self.queue.clone(),
                show_queue: self.show_queue,
                volume: self.volume,
            };
            state.save();
            self.session_dirty = false;
        }
    }

    pub fn restore_session(&mut self) {
        let state = SessionState::load();
        self.queue = state.queue;
        self.show_queue = state.show_queue;
        self.volume = state.volume;
        self.audio.set_volume(state.volume);

        let _ = self.restore_nav_entry(&NavEntry { data: state.data });

        self.nav_history = vec![NavEntry {
            data: self.snapshot_current(),
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
