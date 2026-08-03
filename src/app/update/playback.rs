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
        let sorted_indices: Vec<usize> = {
            let mut s = indices.to_vec();
            s.sort_unstable();
            s
        };
        let extracted: Vec<Track> = sorted_indices
            .iter()
            .filter_map(|&i| self.queue.tracks.get(i).cloned())
            .collect();
        for &i in sorted_indices.iter().rev() {
            if i < self.queue.tracks.len() {
                self.queue.tracks.remove(i);
            }
        }
        let removed_before = sorted_indices.iter().filter(|&&i| i < drop_idx).count();
        let adjusted_drop = (drop_idx - removed_before).min(self.queue.tracks.len());
        let new_count = extracted.len();
        for (j, track) in extracted.into_iter().enumerate() {
            self.queue.tracks.insert(adjusted_drop + j, track);
        }
        self.save_session();

        (adjusted_drop..adjusted_drop + new_count).collect()
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
        } else {
            self.is_playing = !self.is_playing;
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
