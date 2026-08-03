use super::*;
use tracing::debug;

impl MusicPlayer {
    pub fn handle_play_track(&mut self, index: usize, is_queue: bool) {
        if is_queue {
            // Jump to the selected queue entry without discarding the
            // tracks that precede it: the queue represents what's up next.
            if index < self.queue.tracks.len() {
                self.queue.current_index = index;
                if let Some(t) = self.queue.current() {
                    let t = t.clone();
                    self.play_track_internal(&t);
                }
            }
            return;
        }
        if let Some(track) = self.get_track_at(index, false) {
            let track = track.clone();
            // Record the current track as recently played before replacing
            // the queue with a new one.
            if let Some(old) = self.queue.current().cloned() {
                if old.url != track.url {
                    self.queue
                        .record_played(&old, self.config.max_recently_played);
                }
            }
            self.play_track_internal(&track);
            self.queue.clear();
            self.queue.enqueue(track);
            let count = self.current_track_count(false);
            for i in (index + 1)..count {
                if let Some(t) = self.get_track_at(i, false) {
                    self.queue.enqueue(t);
                }
            }
        }
    }

    /// Play a track picked from the Recently Played list.  The old current
    /// track (if any) is recorded as recently played, then the queue is
    /// rebuilt with *only* the selected track so the old queue is discarded.
    pub fn play_recent_track(&mut self, track: Track) {
        if let Some(old) = self.queue.current().cloned() {
            if old.url != track.url {
                self.queue
                    .record_played(&old, self.config.max_recently_played);
            }
        }
        self.play_track_internal(&track);
        self.queue.clear();
        self.queue.enqueue(track);
        self.send_mpris_update();
    }

    pub fn play_track_internal(&mut self, track: &Track) {
        self.track_loading = true;
        let id = track.id.clone();

        if self.stream_cache.contains(&id) {
            let path = self.stream_cache.path_for(&id);
            debug!("Playing from cache: {}", path.display());
            let duration = track.duration as f32;
            self.audio.play_cached(path, duration);
            self.pending_cache_id = None;
        } else {
            self.pending_cache_id = Some(id.clone());
            let cache_path = self.stream_cache.path_for(&id);
            let duration = track.duration as f32;
            self.audio
                .play_stream_cache(&track.url, duration, cache_path);
        }
    }

    pub fn handle_reorder_queue(&mut self, drop_idx: usize, indices: &[usize]) -> Vec<usize> {
        let new_positions = super::drag::reorder_tracks(&mut self.queue.tracks, drop_idx, indices);
        self.save_session();
        new_positions
    }

    pub fn handle_remove_from_queue(&mut self, index: usize) {
        if index < self.queue.tracks.len() {
            self.queue.tracks.remove(index);
            if self.queue.current_index >= self.queue.tracks.len() {
                self.queue.current_index = self.queue.tracks.len().saturating_sub(1);
            } else if index < self.queue.current_index {
                self.queue.current_index -= 1;
            }
            self.save_session();
        }
    }

    pub fn toggle_play_pause(&mut self) {
        if self.queue.current().is_some() {
            if self.is_playing {
                self.audio.pause();
            } else {
                self.audio.resume();
            }
        }
        self.send_mpris_update();
    }

    pub fn next_track(&mut self) {
        // Record the current track as recently played, then remove it from
        // the queue so that Previous can restore it via recently_played.
        if let Some(old) = self.queue.current().cloned() {
            self.queue
                .record_played(&old, self.config.max_recently_played);
            if self.queue.current_index < self.queue.tracks.len() {
                self.queue.tracks.remove(self.queue.current_index);
                // current_index now points to what was the next track
            }
        }

        if let Some(t) = self.queue.current() {
            self.track_loading = true;
            let t = t.clone();
            self.play_track_internal(&t);
            self.send_mpris_update();
        }
    }

    /// “Previous” navigates the recently-played history.  If a track was
    /// recently played it is popped from the history and inserted *before*
    /// the current track in the queue (so the current track becomes “up next”
    /// again).  When history is empty we fall back to stepping back in the
    /// queue itself.
    pub fn previous_track(&mut self) {
        if let Some(prev) = self.queue.recently_played.first() {
            let prev = prev.clone();
            self.queue.recently_played.remove(0);
            // Insert before the current track so it becomes the new current.
            self.queue
                .tracks
                .insert(self.queue.current_index, prev.clone());
            // current_index is unchanged — it now points at the inserted track.
            self.track_loading = true;
            self.play_track_internal(&prev);
            self.send_mpris_update();
        } else if self.queue.previous().is_some() {
            self.track_loading = true;
            if let Some(t) = self.queue.current() {
                let t = t.clone();
                self.play_track_internal(&t);
            }
            self.send_mpris_update();
        }
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.volume = vol.clamp(0.0, 1.0);
        self.audio.set_volume(self.volume);
    }

    pub fn seek(&mut self, frac: f32) {
        let frac = frac.clamp(0.0, 1.0);
        self.progress = frac;
        self.audio
            .seek(std::time::Duration::from_secs_f32(frac * self.duration));
    }
}
