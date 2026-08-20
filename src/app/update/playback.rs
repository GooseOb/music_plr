use super::{MusicPlayer, Track, TrackListKind, TrackPos};
use std::path::PathBuf;
use tracing::debug;

impl MusicPlayer {
    /// `Recent` appends via [`Self::play_recent_track`] rather than
    /// replacing the queue.
    pub fn handle_play_track(&mut self, pos: TrackPos) {
        let TrackPos { index, list } = pos;
        if list == TrackListKind::Recent {
            if let Some(track) = self.get_track_at(pos) {
                self.play_track_replacing_queue(track);
            }
            return;
        }
        if list == TrackListKind::Queue {
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
        if let Some(track) = self.get_track_at(TrackPos::new(index, TrackListKind::Active)) {
            self.play_track_replacing_queue(track);
            for t in self.tracks_after(index).to_vec() {
                self.queue.enqueue(t);
            }
            self.save_session();
        }
    }

    /// Returns the tracks after `index` in the current view.
    fn tracks_after(&self, index: usize) -> &[Track] {
        let start = index + 1;
        self.view_tracks().get(start..).unwrap_or(&[])
    }

    pub fn play_track_replacing_queue(&mut self, track: Track) {
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
        // The active track changed: drop any cached lyrics so the overlay
        // re-fetches for the new track when reopened.
        self.clear_lyrics_for_track_change();

        // Look up any cached normalization gain (1.0 when disabled or unknown).
        let gain = self.normalization_gain_for(track);

        // The file to analyze for loudness: complete for downloaded/local/
        // cached tracks, still downloading for a fresh stream.
        let mut analysis_path: Option<PathBuf> = None;
        let mut streaming = false;

        // Prefer a downloaded file on disk over streaming.
        if let Some(dl_path) = self.download_registry.path_for(&track.url) {
            let path = PathBuf::from(&dl_path);
            if path.exists() {
                debug!("Playing downloaded file: {}", path.display());
                self.audio
                    .play_cached(path.clone(), track.duration as f32, gain);
                self.pending_cache_id = None;
                analysis_path = Some(path);
            }
        }

        if analysis_path.is_none() {
            // YouTube tracks go through the stream/cache pipeline (yt-dlp
            // writes raw AAC-in-M4A straight to the cache file). Local files
            // are played directly from disk (the `PlayCached` path handles any
            // symphonia-decodable format uniformly), so they never hit yt-dlp.
            if track.source == crate::types::TrackSource::YouTube && self.stream_cache.contains(&id)
            {
                let path = self.stream_cache.path_for(&id);
                debug!("Playing from cache: {}", path.display());
                self.audio
                    .play_cached(path.clone(), track.duration as f32, gain);
                self.pending_cache_id = None;
                analysis_path = Some(path);
            } else if track.source == crate::types::TrackSource::Local {
                let path = PathBuf::from(&track.url);
                debug!("Playing local file: {}", path.display());
                self.audio
                    .play_cached(path.clone(), track.duration as f32, gain);
                self.pending_cache_id = None;
                analysis_path = Some(path);
            } else {
                self.pending_cache_id = Some(id.clone());
                let cache_path = self.stream_cache.path_for(&id);
                debug!("Streaming: {}", cache_path.display());
                self.audio.play_stream_cache(
                    &track.url,
                    track.duration as f32,
                    cache_path.clone(),
                    gain,
                );
                streaming = true;
                analysis_path = Some(cache_path);
            }
        }

        // Kick off background loudness analysis so subsequent plays are
        // normalized. A streaming track's cache is incomplete until it
        // finishes downloading, so defer analysis to the tick loop.
        if self.config.volume_normalization && !self.normalization_cache.contains_key(&id) {
            if streaming {
                self.pending_normalization_id = Some(id);
            } else if let Some(path) = analysis_path {
                self.request_normalization_analysis(&id, path);
            }
        }
    }

    /// Normalization gain for `track`: the cached value when normalization is
    /// enabled and known, otherwise 1.0 (no change).
    fn normalization_gain_for(&self, track: &Track) -> f32 {
        if self.config.volume_normalization {
            self.normalization_cache
                .get(&track.id)
                .copied()
                .unwrap_or(1.0)
        } else {
            1.0
        }
    }

    /// Decode `path` in the background to compute its normalization gain and
    /// report it back via the result channel for caching.
    pub(super) fn request_normalization_analysis(&self, track_id: &str, path: PathBuf) {
        let tx = self.result_tx.clone();
        let id = track_id.to_string();
        std::thread::spawn(move || {
            if let Some(gain) = crate::audio::compute_normalization_gain(&path) {
                let _ = tx.send(crate::app::BackendResult::NormalizationComputed(id, gain));
            }
        });
    }

    pub fn handle_reorder_queue(
        &mut self,
        drop_idx: usize,
        indices: &[usize],
        selection: &[usize],
    ) -> Vec<usize> {
        let new_positions =
            crate::util::reorder_tracks(&mut self.queue.tracks, drop_idx, indices, selection);
        self.save_session();
        new_positions
    }

    pub fn handle_remove_from_queue_batch(&mut self, indices: &[usize]) {
        let removed = crate::util::remove_at(&mut self.queue.tracks, indices);
        self.save_session();
        self.mpris_dirty = true;
        self.notify_tracks("Removed", removed, "from queue");
        self.clear_selection_if_touched(indices, TrackListKind::Queue);
    }

    pub fn toggle_play_pause(&mut self) {
        if let Some(track) = self.queue.current().cloned() {
            if self.is_playing {
                self.audio.pause();
            } else if self.audio.has_output() {
                self.audio.resume();
            } else {
                self.play_track_internal(&track);
            }
        }
        self.mpris_dirty = true;
    }

    pub fn next_track(&mut self) {
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

    pub fn previous_track(&mut self) {
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

    /// Seek to an absolute playback position in seconds (used by clicking a
    /// synced lyrics line).
    pub fn seek_to_seconds(&mut self, secs: f32) {
        if self.duration <= 0.0 {
            return;
        }
        let frac = (secs / self.duration).clamp(0.0, 1.0);
        self.seek(frac);
    }
}
