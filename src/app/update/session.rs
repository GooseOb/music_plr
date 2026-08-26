use super::{warn, MusicPlayer};
use crate::data::{session::SessionState, JsonStore};
use std::time::Duration;

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
        if now.duration_since(self.last_session_flush) < SESSION_FLUSH_MIN_INTERVAL {
            return;
        }
        self.last_session_flush = now;
        let state = SessionState {
            data: self.view_data().clone(),
            queue: self.queue.clone(),
            show_queue: self.show_queue,
            volume: self.volume,
            repeat: self.repeat,
            library_expanded: self.library_expanded,
            search_scope: self.search_scope,
            search_provider: self.search_provider,
            lyrics_provider: self.lyrics_client.selected(),
        };
        state.save();
        self.session_dirty = false;
    }

    pub fn restore_session(&mut self) {
        let state = SessionState::load();
        self.queue = state.queue;
        self.show_queue = state.show_queue;
        self.volume = state.volume;
        self.repeat = state.repeat;
        self.audio.set_volume(state.volume);
        self.library_expanded = state.library_expanded;

        let _ = self.restore_nav_entry(state.data);

        self.nav_history = vec![self.view_data().clone()];
        self.nav_history_pos = 0;
        self.lyrics_client = crate::lyrics::LyricsClient::new(state.lyrics_provider);
        self.search_scope = state.search_scope;
        self.search_provider = state.search_provider;
    }

    pub fn resume_playback(&mut self) {
        if self.is_playing {
            if let Some(track) = self.queue.current() {
                let track = track.clone();
                self.play_track_internal(&track, track.source);
            } else {
                self.is_playing = false;
            }
        }
    }

    pub fn notify(&mut self, msg: impl Into<std::borrow::Cow<'static, str>>) {
        self.notification = Some(msg.into());
    }

    pub fn notify_error(&mut self, msg: String) {
        warn!("Backend error: {}", msg);
        self.notification = Some(msg.into());
    }
}
