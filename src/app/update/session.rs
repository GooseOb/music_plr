use std::time::Duration;

use super::{warn, MusicPlayer};
use crate::{
    app::pane::PaneId,
    data::{session::SessionState, JsonStore},
};

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
        let mut panes: Vec<crate::app::PaneData> = self
            .panes
            .values()
            .map(crate::app::PaneData::from)
            .collect();
        panes.sort_by_key(|p| p.id);
        let state = SessionState {
            panes,
            root: self.split_root.clone(),
            focused: self.focused_pane_id,
            queue: self.queue.clone(),
            show_queue: self.show_queue,
            volume: self.volume,
            repeat: self.repeat,
            library_expanded: self.library_expanded,
            lyrics_provider: self.lyrics_provider,
        };
        state.save();
        self.session_dirty = false;
    }

    pub fn restore_session(&mut self) {
        let state = SessionState::load_migrated();
        self.queue = state.queue;
        self.show_queue = state.show_queue;
        self.volume = state.volume;
        self.repeat = state.repeat;
        self.audio.set_volume(state.volume);
        self.library_expanded = state.library_expanded;

        self.panes = state
            .panes
            .into_iter()
            .map(|data| (data.id, data.into_pane()))
            .collect();
        if self.panes.is_empty() {
            self.panes.insert(0, crate::app::Pane::new(0));
        }
        // Drop panes the tree doesn't reference and fall back to a single
        // leaf when the saved tree is inconsistent.
        let mut leaves = Vec::new();
        state.root.leaves(&mut leaves);
        let valid: Vec<PaneId> = leaves
            .into_iter()
            .filter(|id| self.panes.contains_key(id))
            .collect();
        if valid.is_empty() {
            let id = *self.panes.keys().min().unwrap_or(&0);
            self.split_root = crate::app::SplitNode::Leaf(id);
            self.focused_pane_id = id;
        } else {
            self.panes.retain(|id, _| valid.contains(id));
            self.split_root = state.root;
            self.focused_pane_id = if valid.contains(&state.focused) {
                state.focused
            } else {
                valid[0]
            };
        }
        self.next_pane_id = self.panes.keys().max().map_or(0, |m| m + 1).max(1);
        for pane in self.pane_ids() {
            self.sync_downloads_view(pane);
        }
        self.lyrics_provider = state.lyrics_provider;
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
        self.notification = Some(crate::app::Toast {
            message: msg.into(),
            until: std::time::Instant::now() + crate::app::update::tick::NOTIFICATION_DURATION,
            is_error: false,
        });
    }

    pub fn notify_for(
        &mut self,
        msg: impl Into<std::borrow::Cow<'static, str>>,
        duration: std::time::Duration,
    ) {
        self.notification = Some(crate::app::Toast {
            message: msg.into(),
            until: std::time::Instant::now() + duration,
            is_error: false,
        });
    }

    /// Surface a `YouTube` player-client race event as a toast, using the
    /// current language. The "none available" case is an error toast, but
    /// playback/download still falls back to yt-dlp's defaults.
    pub fn notify_client_event(&mut self, event: crate::providers::ClientEvent) {
        match event {
            crate::providers::ClientEvent::Resolving => {
                self.notify(self.strings.resolving_player_client);
            }
            crate::providers::ClientEvent::Resolved(client) => {
                self.notify((self.strings.resolved_player_client)(&client));
            }
            crate::providers::ClientEvent::Unavailable => {
                self.notify_error(self.strings.player_client_unavailable.to_string());
            }
        }
    }

    pub fn notify_error(&mut self, msg: String) {
        warn!("Backend error: {}", msg);
        self.notification = Some(crate::app::Toast {
            message: msg.into(),
            until: std::time::Instant::now() + crate::app::update::tick::NOTIFICATION_DURATION,
            is_error: true,
        });
    }
}
