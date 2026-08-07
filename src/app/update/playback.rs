use super::{MusicPlayer, Track, ViewData};
use tracing::debug;

impl MusicPlayer {
    pub fn handle_play_track(&mut self, index: usize, is_queue: bool) {
        if is_queue {
            // Skip to the selected queue entry: discard all tracks before it
            // (including the current one), keeping the clicked track and
            // everything after it.
            if index < self.queue.tracks.len() {
                if index > 0 {
                    if let Some(old) = self.queue.current().cloned() {
                        self.queue
                            .record_played(&old, self.config.max_recently_played);
                    }
                }
                self.queue.tracks.drain(0..index);
                if let Some(t) = self.queue.current() {
                    let t = t.clone();
                    self.play_track_internal(&t);
                }
                self.save_session();
                self.mpris_dirty = true;
            }
            return;
        }
        if let Some(track) = self.get_track_at(index, false) {
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
            self.mpris_dirty = true;

            // Enqueue remaining tracks after the played one.
            let remaining: Vec<Track> = self.tracks_after(index).to_vec();
            for t in remaining {
                self.queue.enqueue(t);
            }
            self.save_session();
        }
    }

    /// Returns the tracks after `index` in the current view.
    fn tracks_after(&self, index: usize) -> &[Track] {
        let start = index + 1;
        match &self.view_data {
            ViewData::Search { results, .. } => results.get(start..).unwrap_or(&[]),
            ViewData::Radio { tracks, .. } | ViewData::Downloads { tracks, .. } => {
                tracks.get(start..).unwrap_or(&[])
            }
            ViewData::Playlist {
                selected_playlist, ..
            } => {
                if let Some(sp) = selected_playlist {
                    if let Some(pl) = self.playlists.playlists.get(*sp) {
                        pl.tracks.get(start..).unwrap_or(&[])
                    } else {
                        &[]
                    }
                } else {
                    &[]
                }
            }
        }
    }

    /// Play a track picked from the Recently Played list.
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
        self.save_session();
        self.mpris_dirty = true;
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

    pub fn handle_reorder_queue(
        &mut self,
        drop_idx: usize,
        indices: &[usize],
        selection: &[usize],
    ) -> Vec<usize> {
        let new_positions =
            super::drag::reorder_tracks(&mut self.queue.tracks, drop_idx, indices, selection);
        self.save_session();
        new_positions
    }

    pub fn handle_remove_from_queue_batch(&mut self, indices: &[usize]) {
        let mut sorted: Vec<usize> = indices.to_vec();
        sorted.sort_unstable();
        sorted.dedup();
        let mut removed = 0;
        for &i in sorted.iter().rev() {
            if i < self.queue.tracks.len() {
                self.queue.tracks.remove(i);
                removed += 1;
            }
        }
        self.save_session();
        self.mpris_dirty = true;
        self.notify(format!(
            "Removed {} track{} from queue",
            removed,
            crate::util::plural_suffix(removed)
        ));
        // Clear selection if any removed indices were selected, since the
        // queue shifted.
        let sel = self.queue_selected_indices.clone();
        if indices.iter().any(|&i| sel.contains(&i)) {
            self.clear_selection();
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
        self.mpris_dirty = true;
    }

    pub fn next_track(&mut self) {
        // Record the current track as recently played, then advance the queue
        // so that Previous can restore it via recently_played.
        if let Some(old) = self.queue.current().cloned() {
            self.queue
                .record_played(&old, self.config.max_recently_played);
            self.queue.advance();
        }

        if let Some(t) = self.queue.current() {
            self.track_loading = true;
            let t = t.clone();
            self.play_track_internal(&t);
            self.save_session();
            self.mpris_dirty = true;
        }
    }

    /// "Previous" navigates the recently-played history.  If a track was
    /// recently played it is popped from the history and restored to the
    /// front of the queue (becomes the new current track).  When history is
    /// empty there is nothing to go back to.
    pub fn previous_track(&mut self) {
        // Restore the most recently played track to the front of the queue
        // (becomes the new current track).
        if self.queue.restore_previous() {
            self.track_loading = true;
            if let Some(t) = self.queue.current() {
                let t = t.clone();
                self.play_track_internal(&t);
            }
            self.save_session();
            self.mpris_dirty = true;
        }
    }

    pub fn set_volume(&mut self, vol: f32) {
        self.volume = vol.clamp(0.0, 1.0);
        self.audio.set_volume(self.volume);
        self.save_session();
        self.mpris_dirty = true;
    }

    pub fn seek(&mut self, frac: f32) {
        let frac = frac.clamp(0.0, 1.0);
        self.progress = frac;
        self.audio
            .seek(std::time::Duration::from_secs_f32(frac * self.duration));
        self.mpris_dirty = true;
    }
}
