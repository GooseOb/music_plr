use super::{BackendResult, ContextMenuState, MusicPlayer, Track};
use crate::app::interaction::{TrackListKind, TrackPos};
use crate::{
    app::{PlaylistPicker, ViewKind},
    data::JsonStore,
};

impl MusicPlayer {
    /// Spawn a download for `track` specifically from `provider` (used by the
    /// "download from [provider]" context-menu flow).
    pub(super) fn spawn_download_thread_for(
        &self,
        provider: crate::providers::ProviderId,
        track: Track,
    ) {
        let download_dir = self.config.download_dir.clone();
        let tx = self.result_tx.clone();
        std::thread::spawn(move || {
            let result = crate::providers::download(provider, &track, &download_dir);
            match result {
                Ok(path) => {
                    let mut downloaded = track;
                    downloaded.download_path = Some(path);
                    let _ = tx.send(BackendResult::DownloadComplete(
                        downloaded,
                        provider.label().to_string(),
                    ));
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

        let current_id = track.primary_id().to_string();
        let artist = track.artist.clone();
        let title = track.title.clone();
        let album = track.album().map(|a| a.name.clone());
        let duration = track.duration();

        if state.track_id.as_deref() == Some(current_id.as_str())
            && (state.lyrics.is_some() || state.loading || state.not_found)
        {
            return;
        }
        let cached = crate::data::lyrics_cache::LyricsCache::load()
            .get_for(&current_id, self.lyrics_client.selected());
        if let Some(cached_lyrics) = cached {
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
            in_playlist: matches!(self.view_data_mut().kind, ViewKind::Playlist { .. }),
            track,
        });
    }
}
