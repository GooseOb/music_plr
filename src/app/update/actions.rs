use super::{BackendResult, ContextMenuState, MusicPlayer, Track, TrackSource};
use crate::app::interaction::{TrackListKind, TrackPos};
use crate::{
    app::{PlaylistPicker, ViewKind},
    data::JsonStore,
    util::plural_suffix,
};

impl MusicPlayer {
    /// Handle download / delete-download for a set of track indices.
    /// Tracks already downloaded get removed from the registry; tracks
    /// not yet downloaded get queued for download. `list` sources the tracks
    /// from the matching list (queue, active track list, or Recently Played).
    pub fn handle_download_or_remove_tracks(&mut self, indices: &[usize], list: TrackListKind) {
        let mut to_download = Vec::new();
        let mut to_remove = Vec::new();

        for &idx in indices {
            let track = self.get_track_at(TrackPos::new(idx, list));
            if let Some(track) = track {
                if self.download_registry.contains(&track.url) {
                    to_remove.push(track);
                } else if track.source == TrackSource::YouTube {
                    to_download.push(track);
                }
            }
        }

        for track in &to_remove {
            self.download_registry.remove(&track.url);
        }

        if !to_download.is_empty() {
            if to_download.len() == 1 {
                let track = to_download[0].clone();
                self.notify(format!("Downloading \"{}\"...", track.title));
                self.spawn_download_thread(track);
            } else {
                let count = to_download.len();
                self.notify(format!("Downloading {count} tracks..."));
                for track in &to_download {
                    let track = track.clone();
                    self.spawn_download_thread(track);
                }
            }
        }

        if !to_remove.is_empty() {
            let removed = to_remove.len();
            self.notify(format!(
                "Removed {} download{}",
                removed,
                plural_suffix(removed)
            ));
        }
        self.clear_selection_if_touched(indices, list);
    }

    fn spawn_download_thread(&self, track: Track) {
        let download_dir = self.config.download_dir.clone();
        let tx = self.result_tx.clone();
        std::thread::spawn(move || {
            let result = crate::youtube::download(&track.url, &download_dir);
            match result {
                Ok(path) => {
                    let _ = tx.send(BackendResult::DownloadComplete(track, path));
                }
                Err(e) => {
                    let _ = tx.send(BackendResult::DownloadError(e.to_string()));
                }
            }
        });
    }

    pub fn handle_toggle_picker(&mut self, indices: Vec<usize>, list: TrackListKind) {
        self.playlist_picker = if self.playlist_picker.is_some() {
            None
        } else {
            Some(PlaylistPicker { indices, list })
        }
    }

    /// Toggle the lyrics overlay for the current track.
    pub fn handle_show_lyrics(&mut self) {
        if self.lyrics.is_some() {
            self.lyrics = None;
        } else {
            self.lyrics = Some(crate::app::LyricsState::new());
        }
    }

    /// Keep the lyrics editor in sync with the current lyrics text.
    pub(super) fn sync_lyrics_editor(&mut self) {
        let Some(state) = &mut self.lyrics else {
            return;
        };
        let text = match &state.lyrics {
            Some(lyrics) => {
                if lyrics.timed.is_empty() {
                    lyrics.plain.clone()
                } else {
                    lyrics
                        .timed
                        .iter()
                        .map(|(_, line)| line.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                }
            }
            None => String::new(),
        };
        state.editor = Some(iced::widget::text_editor::Content::with_text(&text));
    }

    /// Switch the active lyrics provider, persist it, and force a refetch.
    pub fn handle_select_lyrics_provider(&mut self, provider: crate::lyrics::LyricsProvider) {
        self.lyrics_client = crate::lyrics::LyricsClient::new(provider);
        self.clear_lyrics_for_track_change();
        self.save_session();
    }

    /// Load (from cache) or fetch lyrics for the current track when we don't
    /// already hold them; driven by the tick loop so it reacts to the overlay
    /// being shown and track changes.
    pub(super) fn ensure_lyrics_for_current(&mut self) {
        let Some(track) = self.queue.current() else {
            if let Some(state) = &mut self.lyrics {
                state.lyrics = None;
                state.track_id = None;
                state.loading = false;
            }
            self.sync_lyrics_editor();
            return;
        };
        let Some(state) = &mut self.lyrics else {
            return;
        };

        let current_id = track.id.clone();
        let artist = track.artist.clone();
        let title = track.title.clone();
        let album = track.album.as_ref().map(|a| a.name.clone());
        let duration = track.duration;

        if state.track_id.as_deref() == Some(current_id.as_str())
            && (state.lyrics.is_some() || state.loading || state.not_found)
        {
            return;
        }
        let cached = crate::data::lyrics_cache::LyricsCache::load()
            .get_for(&current_id, self.lyrics_client.selected());
        if let Some(cached_lyrics) = cached {
            eprintln!("Loaded cached lyrics for track {}", current_id);
            state.lyrics = Some(cached_lyrics);
            state.track_id = Some(current_id.clone());
            state.loading = false;
            self.clear_notification();
            self.sync_lyrics_editor();
            return;
        }

        let req = crate::lyrics::LyricsRequest {
            artist,
            title,
            album: album.unwrap_or_default(),
            duration,
        };
        let id = current_id.clone();
        let client = self.lyrics_client.clone();
        let tx = self.result_tx.clone();
        state.lyrics = None;
        state.track_id = Some(id.clone());
        state.loading = true;
        state.not_found = false;
        self.sync_lyrics_editor();
        std::thread::spawn(move || {
            let result = client.fetch(&req);
            match result {
                Ok(lyrics) => {
                    let _ = tx.send(BackendResult::LyricsFetched(lyrics, id));
                }
                Err(e) => {
                    let _ = tx.send(BackendResult::LyricsFetched(None, id));
                    tracing::warn!("Lyrics lookup failed: {e}");
                }
            }
        });
    }

    /// Drop loaded lyrics when the track changes; the overlay stays open
    /// and refetches for the new track.
    pub fn clear_lyrics_for_track_change(&mut self) {
        if let Some(state) = &mut self.lyrics {
            state.lyrics = None;
            state.track_id = None;
            state.loading = false;
        }
        self.sync_lyrics_editor();
    }

    pub fn show_context_menu(&mut self, pos: TrackPos) {
        let Some(track) = self.get_track_at(pos) else {
            return;
        };
        let TrackPos { index, list } = pos;

        let sel = self.selection(list);
        let target_indices = if sel.contains(&index) {
            sel.to_vec()
        } else {
            vec![index]
        };

        self.context_menu = Some(ContextMenuState {
            pos,
            target_indices,
            position: (self.drag.cursor_pos.x, self.drag.cursor_pos.y),
            is_youtube: track.source == TrackSource::YouTube,
            is_downloaded: self.download_registry.contains(&track.url),
            in_playlist: matches!(self.view_data_mut().kind, ViewKind::Playlist { .. }),
            track,
        });
    }
}
