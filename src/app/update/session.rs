use super::{warn, MusicPlayer, NavEntry};
use crate::session::SessionState;
use std::time::Duration;

/// Minimum time between `session.json` writes. `save_session()` marks the
/// session dirty on many code paths, but the tick drains that into a write at
/// most this often to avoid rewriting the full state 4×/second.
const SESSION_FLUSH_MIN_INTERVAL: Duration = Duration::from_secs(1);

impl MusicPlayer {
    pub fn save_session(&mut self) {
        self.session_dirty = true;
    }

    /// Persist the session, but at most once per `SESSION_FLUSH_MIN_INTERVAL`
    /// so that the frequent `save_session()` callers (volume, seek, drag,
    /// navigation) don't rewrite `session.json` on every 250ms tick.
    pub fn flush_session(&mut self) {
        if !self.session_dirty {
            return;
        }
        let now = std::time::Instant::now();
        if now.duration_since(self.last_session_flush)
            < crate::app::update::session::SESSION_FLUSH_MIN_INTERVAL
        {
            return;
        }
        self.last_session_flush = now;
        let state = SessionState {
            data: self.view_data.clone(),
            queue: self.queue.clone(),
            show_queue: self.show_queue,
            volume: self.volume,
        };
        state.save();
        self.session_dirty = false;
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
