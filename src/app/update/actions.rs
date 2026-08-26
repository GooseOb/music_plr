use super::{BackendResult, ContextMenuState, MusicPlayer, Track};
use crate::app::interaction::{TrackListKind, TrackPos};
use crate::app::EditTrackState;
use crate::load_state::LoadState;
use crate::{
    app::{PlaylistPicker, ViewKind},
    data::JsonStore,
    providers::ProviderId,
};

impl MusicPlayer {
    /// Take the open menu, clearing its captured geometry so a reopened
    /// menu never renders against stale measurements.
    pub(crate) fn take_context_menu(&mut self) -> Option<ContextMenuState> {
        self.bounds.context_menu = None;
        self.context_menu.take()
    }
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

    /// Keep the lyrics editor in sync with the current lyrics text
    /// (no-op outside `Selectable` mode).
    pub(super) fn sync_lyrics_editor(&mut self) {
        let Some(state) = &mut self.lyrics else {
            return;
        };
        let text = match &state.lyrics {
            LoadState::Ready(lyrics) => {
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
            _ => String::new(),
        };
        if state.mode == crate::app::LyricsViewMode::Selectable {
            state.editor = iced::widget::text_editor::Content::with_text(&text);
        }
    }

    pub fn set_lyrics_view_mode(&mut self, mode: crate::app::LyricsViewMode) {
        let Some(state) = &mut self.lyrics else {
            return;
        };
        if !state.mode_available(mode) {
            return;
        }
        state.mode = mode;
        self.sync_lyrics_editor();
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
                state.lyrics = crate::load_state::LoadState::Loading;
                state.track_id = None;
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

        if state.track_id.as_deref() == Some(current_id.as_str()) && !state.lyrics.is_loading() {
            return;
        }
        let cached = crate::data::lyrics_cache::LyricsCache::load()
            .get_for(&current_id, self.lyrics_client.selected());
        if let Some(cached_lyrics) = cached {
            let mode = crate::app::LyricsViewMode::for_lyrics(&cached_lyrics);
            state.lyrics = crate::load_state::LoadState::Ready(cached_lyrics);
            state.track_id = Some(current_id.clone());
            state.mode = mode;
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
        state.lyrics = crate::load_state::LoadState::Loading;
        state.track_id = Some(id.clone());
        self.sync_lyrics_editor();
        let no_lyrics = self.strings.no_lyrics_found;
        std::thread::spawn(move || {
            let result = match client.fetch(&req) {
                Ok(Some(lyrics)) => Ok(lyrics),
                Ok(None) => Err(no_lyrics.to_string()),
                Err(e) => {
                    tracing::warn!("Lyrics lookup failed: {e}");
                    Err(e.to_string())
                }
            };
            let _ = tx.send(BackendResult::LyricsFetched(result, id));
        });
    }

    /// Drop loaded lyrics when the track changes; the overlay stays open
    /// and refetches for the new track.
    pub fn clear_lyrics_for_track_change(&mut self) {
        if let Some(state) = &mut self.lyrics {
            state.lyrics = crate::load_state::LoadState::Loading;
            state.track_id = None;
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
            cursor: (self.drag.cursor_pos.x, self.drag.cursor_pos.y),
            in_playlist: matches!(self.view_data().kind, ViewKind::Playlist(_)),
            track,
            hovered: None,
        });
    }

    pub fn close_context_menu(&mut self) {
        self.context_menu = None;
        self.bounds.context_menu = None;
    }

    /// Open the artist page on `provider`, using the track's stored artist
    /// id when present and resolving by name otherwise.
    pub fn handle_context_menu_go_to_artist(&mut self, provider: ProviderId) {
        let Some(track) = self.take_context_menu().map(|m| m.track) else {
            return;
        };
        if let Some(artist_id) = track.provider_artist_id(provider) {
            self.open_artist(artist_id, &track.artist, provider);
        } else {
            self.open_artist_by_name(&track.artist, provider);
        }
    }

    pub fn handle_context_menu_song_radio(&mut self, provider: ProviderId) {
        if let Some(track) = self.take_context_menu().map(|m| m.track) {
            self.start_radio_provider(provider, &track, false);
        }
    }

    pub fn handle_context_menu_artist_radio(&mut self, provider: ProviderId) {
        if let Some(track) = self.take_context_menu().map(|m| m.track) {
            self.start_radio_provider(provider, &track, true);
        }
    }

    /// Open the track-editing popup for the track at `pos`, seeding the
    /// working copy from the live track. Only one track is edited at a time
    /// (the right-clicked one), so multi-selection is ignored here.
    pub fn open_edit_track(&mut self, pos: TrackPos) {
        let Some(track) = self.get_track_at(pos) else {
            return;
        };
        self.edit_track = Some(EditTrackState {
            title: track.title.clone(),
            artist: track.artist.clone(),
            source: track.source,
            original: track,
            pos,
        });
    }

    /// Apply the edited fields back to the track's source list and close the
    /// popup. `source` follows the working copy (changed via the provider
    /// "select" buttons); `title`/`artist` are overwritten from the inputs.
    pub fn apply_edit_track(&mut self) {
        let Some(edit) = self.edit_track.take() else {
            return;
        };
        let mut track = edit.original;
        track.title = edit.title;
        track.artist = edit.artist;
        track.source = edit.source;
        self.set_track_at(edit.pos, track);
        self.save_session();
    }
}
