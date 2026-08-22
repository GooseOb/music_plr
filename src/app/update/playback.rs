use super::{MusicPlayer, Track, TrackListKind, TrackPos};
use crate::app::ViewKind;
use crate::data::cache::StreamCache;
use crate::data::JsonStore;
use crate::providers::ProviderId;
use std::path::PathBuf;
use tracing::debug;

impl MusicPlayer {
    /// `Recent` appends via [`Self::play_recent_track`] rather than
    /// replacing the queue.
    pub fn handle_play_track(&mut self, pos: TrackPos) {
        let TrackPos { index, list } = pos;
        if list == TrackListKind::Recent {
            if let Some(track) = self.get_track_at(pos) {
                self.play_track_replacing_queue(track, self.config.default_provider);
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
                    self.play_track_internal(&t, self.config.default_provider);
                }
                self.save_session();
                self.mpris_dirty = true;
            }
            return;
        }
        if let Some(track) = self.get_track_at(TrackPos::new(index, TrackListKind::Active)) {
            self.play_and_queue_rest(track, self.config.default_provider, index);
        }
    }

    /// Play `track` through `provider`, replacing the queue, then enqueue the
    /// remaining view tracks after `index` and persist the session.
    fn play_and_queue_rest(&mut self, track: Track, provider: ProviderId, index: usize) {
        self.play_track_replacing_queue(track, provider);
        for t in self.tracks_after(index).to_vec() {
            self.queue.enqueue(t);
        }
        self.save_session();
    }

    /// Returns the tracks after `index` in the current view.
    pub(super) fn tracks_after(&self, index: usize) -> &[Track] {
        let start = index + 1;
        self.view_tracks().get(start..).unwrap_or(&[])
    }

    /// Persist `track` back into the list it came from (`pos`), so a resolved
    /// provider id survives and the source view/queue reflects it.
    pub(super) fn set_track_at(&mut self, pos: TrackPos, track: Track) {
        let TrackPos { index, list } = pos;
        match list {
            TrackListKind::Queue => {
                if let Some(t) = self.queue.tracks.get_mut(index) {
                    *t = track;
                }
            }
            TrackListKind::Active => {
                // Avoid holding a `view_data_mut` borrow across the playlist
                // store access by deciding the target first.
                let target = match &self.view_data().kind {
                    ViewKind::Playlist {
                        index: Some(sp), ..
                    } => Some(*sp),
                    _ => None,
                };
                match target {
                    Some(sp) => {
                        if let Some(pl) = self.playlists.playlists.get_mut(sp) {
                            if let Some(t) = pl.tracks.get_mut(index) {
                                *t = track;
                            }
                        }
                        self.playlists.save();
                    }
                    None => {
                        if let Some(t) = self.view_data_mut().tracks.get_mut(index) {
                            *t = track;
                        }
                    }
                }
            }
            TrackListKind::Recent => {}
        }
    }

    pub fn play_track_replacing_queue(&mut self, track: Track, preferred: ProviderId) {
        let key = track.cache_key();
        if let Some(old) = self.queue.current().cloned() {
            if old.cache_key() != key {
                self.queue
                    .record_played(&old, self.config.max_recently_played);
            }
        }
        self.play_track_internal(&track, preferred);
        self.queue.clear();
        self.queue.enqueue(track);
        self.save_session();
        self.mpris_dirty = true;
    }

    /// Play `pos` through `provider`. If the track already carries that
    /// provider's id, switch its source and play directly; otherwise resolve
    /// the id on `provider` in the background, then stream. Either way the
    /// track's `source` becomes `provider` so the chosen provider's metadata
    /// is what displays and drives playback.
    pub fn play_track_via_provider(&mut self, provider: ProviderId, pos: TrackPos) {
        let Some(track) = self.get_track_at(pos) else {
            return;
        };
        if track.has_provider(provider) {
            let mut t = track;
            t.source = provider;
            // Persist the source switch back into the list it came from (a
            // playlist is written to disk; the queue is saved via the session)
            // so the chosen provider survives a restart.
            self.set_track_at(pos, t.clone());
            self.play_and_queue_rest(t, provider, pos.index);
        } else {
            self.resolve_provider(provider, track, Some(pos), true);
        }
    }

    /// Download `indices` from `provider`. Tracks lacking the provider id are
    /// resolved first (best-effort; the resolve flow stores the id and then
    /// downloads).
    pub fn download_track_via_provider(&mut self, provider: ProviderId, indices: &[usize]) {
        let list = self.context_menu.as_ref().map(|m| m.pos.list);
        let mut to_download: Vec<Track> = Vec::new();
        if let Some(list) = list {
            for &idx in indices {
                if let Some(track) = self.get_track_at(TrackPos::new(idx, list)) {
                    if track.can_download_from(provider) {
                        to_download.push(track);
                    } else {
                        let pos = TrackPos::new(idx, list);
                        self.resolve_provider(provider, track, Some(pos), false);
                    }
                }
            }
        }
        if !to_download.is_empty() {
            self.notify(format!("Downloading {} track(s)...", to_download.len()));
            for track in to_download {
                self.spawn_download_thread_for(provider, track);
            }
        }
    }

    /// Resolve a track's id on `provider` in the background. If `play` is true
    /// the resolved track is streamed; otherwise it is downloaded.
    fn resolve_provider(
        &mut self,
        provider: ProviderId,
        track: Track,
        pos: Option<TrackPos>,
        play: bool,
    ) {
        self.notify(format!(
            "Resolving \"{}\" on {}...",
            track.title,
            provider.label()
        ));
        let tx = self.result_tx.clone();
        std::thread::spawn(move || {
            let resolved = crate::providers::resolve_id(provider, &track);
            let resolved = match resolved {
                Ok(resolved) => resolved,
                Err(e) => {
                    let _ = tx.send(crate::app::BackendResult::ProviderResolveError {
                        title: track.title.clone(),
                        provider,
                        message: e.to_string(),
                    });
                    return;
                }
            };
            let result = if play {
                crate::app::BackendResult::ProviderResolved {
                    original: track,
                    provider,
                    resolved,
                    pos,
                }
            } else {
                crate::app::BackendResult::ProviderResolvedDownload {
                    original: track,
                    provider,
                    resolved,
                    pos,
                }
            };
            let _ = tx.send(result);
        });
    }

    pub fn play_track_internal(&mut self, track: &Track, preferred: ProviderId) {
        self.track_loading = true;
        // The active track changed: drop any cached lyrics so the overlay
        // re-fetches for the new track when reopened.
        self.clear_lyrics_for_track_change();

        // Look up any cached normalization gain (1.0 when disabled or unknown).
        let gain = self.normalization_gain_for(track);

        // The file to analyze for loudness: complete for downloaded/local/
        // cached tracks, still downloading for a fresh stream.
        let mut analysis_path: Option<PathBuf> = None;
        let mut streaming = false;

        let provider = track.best_stream_provider(preferred);

        match provider {
            None => {
                // No streamable provider for this track: resolve it on the
                // default provider, then stream from there.
                let track = track.clone();
                self.pending_cache_id = None;
                self.resolve_provider(self.config.default_provider, track, None, true);
                return;
            }
            Some(provider) => {
                let id = track.provider_id(provider).unwrap_or_default().to_string();

                // Prefer a downloaded file on disk over streaming.
                let dl_key = track.cache_key();
                if let Some(dl_path) = self.download_registry.path_for(&dl_key) {
                    let path = PathBuf::from(&dl_path);
                    if path.exists() {
                        debug!("Playing downloaded file: {}", path.display());
                        self.audio
                            .play_cached(path.clone(), track.duration() as f32, gain);
                        self.pending_cache_id = None;
                        analysis_path = Some(path);
                    }
                }

                if analysis_path.is_none() {
                    if self.stream_cache.contains(provider, &id) {
                        let path = StreamCache::path_for(provider, &id);
                        debug!("Playing from cache: {}", path.display());
                        self.audio
                            .play_cached(path.clone(), track.duration() as f32, gain);
                        self.pending_cache_id = None;
                        analysis_path = Some(path);
                    } else {
                        let url = track.provider_url(provider).unwrap_or_default().to_string();
                        self.pending_cache_id = Some(format!("{provider:?}:{id}"));
                        let cache_path = StreamCache::path_for(provider, &id);
                        debug!("Streaming ({}): {}", provider.label(), cache_path.display());
                        if provider.uses_ytdlp() {
                            self.audio.play_stream_cache(
                                &url,
                                track.duration() as f32,
                                cache_path.clone(),
                                gain,
                            );
                        } else {
                            self.audio.play_stream_http(
                                &url,
                                track.duration() as f32,
                                cache_path.clone(),
                                gain,
                            );
                        }
                        streaming = true;
                        analysis_path = Some(cache_path);
                    }
                }
            }
        }

        // Kick off background loudness analysis so subsequent plays are
        // normalized. A streaming track's cache is incomplete until it
        // finishes downloading, so defer analysis to the tick loop.
        if self.config.volume_normalization
            && !self.normalization_cache.contains_key(&track.cache_key())
        {
            if streaming {
                self.pending_normalization_id = Some(track.cache_key());
            } else if let Some(path) = analysis_path {
                self.request_normalization_analysis(&track.cache_key(), path);
            }
        }
    }

    /// Normalization gain for `track`: the cached value when normalization is
    /// enabled and known, otherwise 1.0 (no change).
    fn normalization_gain_for(&self, track: &Track) -> f32 {
        if self.config.volume_normalization {
            self.normalization_cache
                .get(&track.cache_key())
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
                self.play_track_internal(&track, track.source);
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
            self.play_track_internal(&t, t.source);
            self.save_session();
            self.mpris_dirty = true;
        }
    }

    pub fn previous_track(&mut self) {
        if self.queue.restore_previous() {
            self.track_loading = true;
            if let Some(t) = self.queue.current() {
                let t = t.clone();
                self.play_track_internal(&t, t.source);
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
