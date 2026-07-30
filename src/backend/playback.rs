use super::PlaybackState;
use crate::config;
use slint::ComponentHandle;

impl super::Backend {
    pub fn handle_play_track(&mut self, index: usize) {
        let tracks = self.get_current_tracks();
        if tracks.is_empty() {
            return;
        }
        let track = match tracks.get(index) {
            Some(t) => t.clone(),
            None => return,
        };
        self.queue = super::PlayQueue {
            tracks: tracks.clone(),
            current_index: index,
        };

        self.play_track_internal(&track);
    }

    pub fn handle_next_track(&mut self) {
        if self.queue.next().is_none() {
            return;
        }
        let track = match self.queue.current() {
            Some(t) => t.clone(),
            None => return,
        };
        self.play_track_internal(&track);
    }

    pub fn handle_previous_track(&mut self) {
        if self.queue.previous().is_none() {
            return;
        }
        let track = match self.queue.current() {
            Some(t) => t.clone(),
            None => return,
        };
        self.play_track_internal(&track);
    }

    pub fn handle_toggle_play_pause(&mut self) {
        if self.is_playing {
            self.audio.pause();
            self.is_playing = false;
        } else if self.queue.current().is_some() {
            self.audio.resume();
            self.is_playing = true;
        }
        if let Some(window) = self.ui.upgrade() {
            window
                .global::<PlaybackState>()
                .set_is_playing(self.is_playing);
        }
    }

    pub fn handle_set_volume(&mut self, vol: f32) {
        self.volume = vol;
        self.config.volume = vol;
        self.audio.set_volume(vol);
        config::save_config(&self.config);
        if let Some(window) = self.ui.upgrade() {
            window.global::<PlaybackState>().set_volume(vol);
        }
    }

    pub fn handle_seek(&mut self, frac: f32) {
        let pos = std::time::Duration::from_secs_f32(frac * self.duration);
        self.progress = frac;
        self.audio.seek(pos);
        if let Some(window) = self.ui.upgrade() {
            let playback = window.global::<PlaybackState>();
            playback.set_progress(frac);
            let elapsed = (self.progress * self.duration) as u32;
            playback.set_elapsed_text(format!("{}:{:02}", elapsed / 60, elapsed % 60).into());
        }
    }

    pub fn handle_reorder_queue(&mut self, from_rel: usize, to_rel: usize) {
        let offset = self.queue.current_index + 1;
        let abs_from = offset + from_rel;
        let mut abs_to = offset + to_rel;
        if abs_from >= self.queue.tracks.len() || abs_to > self.queue.tracks.len() {
            return;
        }
        let track = self.queue.tracks.remove(abs_from);
        if abs_to > abs_from {
            abs_to -= 1;
        }
        self.queue.tracks.insert(abs_to, track);
        self.sync_queue_ui();
    }

    pub fn handle_remove_from_queue(&mut self, rel_idx: usize) {
        let offset = self.queue.current_index + 1;
        let abs_idx = offset + rel_idx;
        if abs_idx < self.queue.tracks.len() {
            self.queue.tracks.remove(abs_idx);
            self.sync_queue_ui();
        }
    }

    pub fn handle_play_from_queue(&mut self, rel_idx: usize) {
        let abs_idx = self.queue.current_index + 1 + rel_idx;
        if abs_idx >= self.queue.tracks.len() {
            return;
        }
        self.queue.current_index = abs_idx;
        let track = match self.queue.current() {
            Some(t) => t.clone(),
            None => return,
        };
        self.play_track_internal(&track);
    }
}
