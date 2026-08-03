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
        if self.queue.next().is_some() {
            self.track_loading = true;
            if let Some(t) = self.queue.current() {
                let t = t.clone();
                self.play_track_internal(&t);
            }
            self.send_mpris_update();
        }
    }

    pub fn previous_track(&mut self) {
        if self.queue.previous().is_some() {
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
