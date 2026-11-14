use std::sync::Arc;

use iced::{widget::operation, Point, Task};

use super::{BackendResult, MusicPlayer, Track};
use crate::{
    app::{
        dialog::Dialog,
        interaction::{CardMenuState, ContextMenuState, MenuAnchor, TrackPos},
        pane::{is_main_pane, PaneId},
        update::operation::{CaptureContextMenu, ScrollTarget},
        LyricsViewMode, Message, ViewKind,
    },
    data::JsonStore,
    load_state::LoadState,
    providers::ProviderId,
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
            let emit = |event| drop(tx.send(BackendResult::PlayerClientEvent(event)));
            let result = crate::providers::download(
                provider,
                &track,
                std::path::Path::new(&download_dir),
                &emit,
            );
            match result {
                Ok(path) => {
                    let mut downloaded = track;
                    downloaded.set_download_path(path);
                    let _ = tx.send(BackendResult::DownloadComplete(
                        downloaded,
                        provider.name().into_owned(),
                    ));
                }
                Err(e) => {
                    let _ = tx.send(BackendResult::DownloadError(provider, e.into()));
                }
            }
        });
    }

    /// Toggle the lyrics overlay for the current track.
    pub fn handle_show_lyrics(&mut self, pane: PaneId) -> Task<Message> {
        if self.pane(pane).lyrics.is_some() {
            self.pane_mut(pane).lyrics = None;
            Task::none()
        } else {
            let provider = self.config.default_lyrics_provider;
            self.pane_mut(pane).lyrics = Some(crate::app::LyricsState::new(provider));
            self.ensure_lyrics_for_current(pane);
            self.scroll_lyrics_to_active(pane)
        }
    }

    /// Keep the lyrics editor in sync with the current lyrics text
    /// (no-op outside `Selectable` mode).
    pub(super) fn sync_lyrics_editor(&mut self, pane: PaneId) {
        let Some(state) = &mut self.pane_mut(pane).lyrics else {
            return;
        };
        Self::sync_editor_content(state);
    }

    fn sync_editor_content(state: &mut crate::app::LyricsState) {
        state.sync_plain_lines();
        let timed;
        let text = match state.displayed_lyrics() {
            Some(lyrics) if lyrics.has_timed() => {
                timed = lyrics
                    .lines
                    .iter()
                    .map(|line| line.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                &timed
            }
            Some(lyrics) => &lyrics.plain,
            None => "",
        };
        if state.mode == LyricsViewMode::Selectable {
            state.editor = iced::widget::text_editor::Content::with_text(text);
        }
    }

    /// Put the pane back into its loading state for `track_id`
    /// (`None` while no track is playing), dropping the previous lyrics and
    /// every editor/translation side effect.
    fn load_lyrics(
        state: &mut crate::app::LyricsState,
        track_id: Option<String>,
        custom_names: Vec<String>,
    ) {
        state.lyrics = LoadState::Loading;
        state.track_id = track_id;
        state.scrolled_to = None;
        state.viewport = None;
        state.custom_names = custom_names;
        Self::end_custom_edit(state);
        state.selected_custom = None;
        state.reset_translation_state();
        state.reset_note_state();
        Self::sync_editor_content(state);
    }

    /// Install ready lyrics and reset the display state derived from them.
    pub(super) fn install_lyrics(
        state: &mut crate::app::LyricsState,
        lyrics: Arc<crate::lyrics::Lyrics>,
    ) {
        state.mode = LyricsViewMode::for_lyrics(&lyrics);
        state.lyrics = LoadState::Ready(lyrics);
        state.scrolled_to = None;
        state.viewport = None;
        state.reset_translation_state();
        state.reset_note_state();
    }

    /// Recompute the view mode and drop scroll/note state after the
    /// displayed lyrics changed (e.g. a translation was selected).
    pub(super) fn refresh_displayed_lyrics(state: &mut crate::app::LyricsState) {
        if let Some(lyrics) = state.displayed_lyrics() {
            state.mode = LyricsViewMode::for_lyrics(lyrics);
        }
        state.scrolled_to = None;
        state.viewport = None;
        state.reset_note_state();
    }

    /// Drop everything tied to the current lyrics and wait for a new load.
    fn reset_lyrics(state: &mut crate::app::LyricsState) {
        Self::load_lyrics(state, None, Vec::new());
    }

    fn end_custom_edit(state: &mut crate::app::LyricsState) {
        state.editing = false;
        state.editing_custom_name = None;
    }

    fn begin_custom_edit(
        state: &mut crate::app::LyricsState,
        track_id: String,
        content: iced::widget::text_editor::Content,
        name: Option<String>,
    ) {
        state.track_id = Some(track_id);
        state.edit_name = name.clone().unwrap_or_default();
        state.edit_content = content;
        state.editing = true;
        state.editing_custom_name = name;
        state.viewport = None;
        state.reset_note_state();
    }

    pub(super) fn flush_note_draft(&mut self, pane: PaneId) {
        // Note drafts on provider lyrics are session-only; custom entries never
        // show a translation, so `lyrics` is the entry itself here. The pane
        // borrow ends before the cache is touched.
        let Some(state) = &mut self.pane_mut(pane).lyrics else {
            return;
        };
        let Some(idx) = state.note_line else {
            return;
        };
        let text = state.note_editor.text().trim_end().to_string();
        let track_id = state.track_id.clone();
        let custom_name = state.selected_custom.clone();
        let translated = state.selected_translation.is_some();
        let Some(lyrics) = state.displayed_lyrics_mut() else {
            return;
        };
        let Some(line) = lyrics.lines.get_mut(idx) else {
            return;
        };
        if line.description == text {
            return;
        }
        line.description = text;
        if !translated {
            if let (Some(track_id), Some(name)) = (track_id, custom_name) {
                let lyrics = lyrics.clone();
                self.lyrics_cache.insert_custom(&track_id, &name, &lyrics);
                self.lyrics_cache.save();
            }
        }
    }

    pub(super) fn refresh_note_editor(&mut self, pane: PaneId) {
        let target = self
            .pane(pane)
            .lyrics
            .as_ref()
            .and_then(crate::app::LyricsState::note_target);
        let Some(state) = &mut self.pane_mut(pane).lyrics else {
            return;
        };
        if state.note_line == target {
            return;
        }
        state.note_line = target;
        let text = match (state.displayed_lyrics(), target) {
            (Some(lyrics), Some(idx)) => lyrics.lines.get(idx).map_or("", |line| &line.description),
            _ => "",
        };
        state.note_editor = iced::widget::text_editor::Content::with_text(text);
    }

    pub(super) fn sync_note_editor(&mut self, pane: PaneId) {
        self.flush_note_draft(pane);
        self.refresh_note_editor(pane);
    }

    pub fn select_lyric_line(&mut self, pane: PaneId, idx: usize) -> Task<Message> {
        self.flush_note_draft(pane);
        if let Some(state) = &mut self.pane_mut(pane).lyrics {
            state.picked_line = Some(idx);
        }
        self.refresh_note_editor(pane);
        Task::none()
    }

    pub fn set_lyrics_view_mode(&mut self, pane: PaneId, mode: LyricsViewMode) -> Task<Message> {
        let Some(state) = &mut self.pane_mut(pane).lyrics else {
            return Task::none();
        };
        if !state.mode_available(mode) {
            return Task::none();
        }
        state.mode = mode;
        if mode == LyricsViewMode::Synced {
            state.scrolled_to = None;
        }
        state.viewport = None;
        self.sync_lyrics_editor(pane);
        self.sync_note_editor(pane);
        if mode == LyricsViewMode::Synced {
            self.scroll_lyrics_to_active(pane)
        } else {
            Task::none()
        }
    }

    pub(super) fn lyrics_active(&self, pane: PaneId) -> Option<(usize, usize)> {
        let state = self.pane(pane).lyrics.as_ref()?;
        if state.mode != LyricsViewMode::Synced {
            return None;
        }
        let lyrics = state.displayed_lyrics()?;
        if !lyrics.has_timed() {
            return None;
        }
        let position = self.playback_progress() * self.duration;
        Some((lyrics.active_index(position)?, lyrics.timed_count()))
    }

    fn lyrics_scroll_task(&mut self, pane: PaneId, index: usize, total: usize) -> Task<Message> {
        use crate::app::ui::lyrics_scroll_id;
        if let Some(vp) = self.pane(pane).lyrics.as_ref().and_then(|s| s.viewport) {
            let avg = vp.content_h / total.max(1) as f32;
            let max = (vp.content_h - vp.height).max(0.0);
            let y = ((index as f32 + 0.5) * avg - vp.height / 2.0).clamp(0.0, max);
            if let Some(state) = &mut self.pane_mut(pane).lyrics {
                if let Some(v) = &mut state.viewport {
                    v.offset_y = y;
                }
            }
            return operation::scroll_to::<Message>(
                lyrics_scroll_id(pane),
                operation::AbsoluteOffset { x: 0.0, y },
            );
        }
        let frac = if total > 1 {
            index as f32 / (total - 1) as f32
        } else {
            0.0
        };
        operation::snap_to::<Message>(
            lyrics_scroll_id(pane),
            operation::RelativeOffset { x: 0.0, y: frac },
        )
    }

    /// Whether the lyrics line at `index` overlaps the viewport, using
    /// the average line pitch from the last `on_scroll` viewport.
    /// `None` when no viewport is known yet.
    fn lyrics_line_visible(&self, pane: PaneId, index: usize, total: usize) -> Option<bool> {
        const EPS: f32 = 1.0;
        let vp = self.pane(pane).lyrics.as_ref().and_then(|s| s.viewport)?;
        if total == 0 {
            return None;
        }
        let avg = vp.content_h / total as f32;
        let top = index as f32 * avg;
        let bottom = top + avg;
        Some(top < vp.offset_y + vp.height + EPS && bottom > vp.offset_y - EPS)
    }

    pub(super) fn scroll_lyrics_to_active(&mut self, pane: PaneId) -> Task<Message> {
        let Some((index, total)) = self.lyrics_active(pane) else {
            return Task::none();
        };
        if let Some(state) = &mut self.pane_mut(pane).lyrics {
            state.scrolled_to = Some(index);
        }
        self.sync_note_editor(pane);
        self.lyrics_scroll_task(pane, index, total)
    }

    pub(super) fn maybe_autoscroll_lyrics(&mut self, pane: PaneId) -> Task<Message> {
        let Some((active, total)) = self.lyrics_active(pane) else {
            return Task::none();
        };
        let already = self.pane(pane).lyrics.as_ref().and_then(|s| s.scrolled_to);
        if already == Some(active) {
            return Task::none();
        }
        if let Some(state) = &mut self.pane_mut(pane).lyrics {
            state.scrolled_to = Some(active);
        }
        self.sync_note_editor(pane);
        if self.lyrics_line_visible(pane, active, total) == Some(false) {
            return Task::none();
        }
        self.lyrics_scroll_task(pane, active, total)
    }

    /// Switch the lyrics provider of one pane and force it to refetch.
    pub fn handle_select_lyrics_provider(
        &mut self,
        pane: PaneId,
        provider: crate::lyrics::LyricsProvider,
    ) {
        if !self.config.lyrics_provider_enabled(provider) {
            return;
        }
        self.flush_note_draft(pane);
        if let Some(state) = &mut self.pane_mut(pane).lyrics {
            state.provider = provider;
            Self::reset_lyrics(state);
        }
        self.ensure_lyrics_for_current(pane);
    }

    /// Refetch the selected network provider's lyrics for the current track,
    /// bypassing the cache. Custom entries are edited, not refetched.
    pub fn handle_refresh_lyrics(&mut self, pane: PaneId) {
        let refreshable = self.pane(pane).lyrics.as_ref().is_some_and(|state| {
            state.selected_custom.is_none()
                && state.provider != crate::lyrics::LyricsProvider::Custom
                && self.config.lyrics_provider_enabled(state.provider)
        });
        if !refreshable {
            return;
        }
        self.flush_note_draft(pane);
        self.ensure_lyrics_for_current_with(pane, true);
    }

    /// Switch every open lyrics pane off a provider that was just disabled,
    /// then load the configured default (or the first enabled) one.
    pub(super) fn reconcile_open_lyrics_providers(&mut self) {
        let default = self.config.default_lyrics_provider;
        for pane in self.pane_ids() {
            let Some(provider) = self.pane(pane).lyrics.as_ref().map(|state| state.provider) else {
                continue;
            };
            if self.config.lyrics_provider_enabled(provider) {
                continue;
            }
            if let Some(state) = &mut self.pane_mut(pane).lyrics {
                state.provider = default;
                Self::reset_lyrics(state);
            }
            self.ensure_lyrics_for_current(pane);
        }
    }

    /// Switch the displayed lyrics of one pane between the original and a
    /// loaded translation. Unloaded translations are fetched in the
    /// background; the pane keeps showing the original meanwhile.
    pub fn handle_select_lyrics_translation(
        &mut self,
        pane: PaneId,
        language: Option<String>,
    ) -> Task<Message> {
        let Some(track) = self.session.queue.current() else {
            return Task::none();
        };
        let track_id = track.cache_key();
        let tx = self.result_tx.clone();
        let no_lyrics = self.strings.no_lyrics_found;
        self.flush_note_draft(pane);
        let Some(state) = &mut self.pane_mut(pane).lyrics else {
            return Task::none();
        };
        let provider = state.provider;
        let ready = matches!(state.lyrics, LoadState::Ready(_));
        if !ready || state.editing || state.selected_custom.is_some() {
            return Task::none();
        }
        if state.selected_translation == language {
            return Task::none();
        }
        let source = language.as_deref().and_then(|lang| {
            let LoadState::Ready(lyrics) = &state.lyrics else {
                return None;
            };
            lyrics
                .translations
                .iter()
                .find(|t| t.language == lang)
                .and_then(|t| t.source.clone())
        });
        if let Some(source) = source {
            let Some(language) = language else {
                return Task::none();
            };
            state.selected_translation = Some(language.clone());
            state.loading_translation = Some(language.clone());
            let id = track_id.clone();
            std::thread::spawn(move || {
                let result = match provider.fetch_translation(&language, &source) {
                    Ok(Some(translation)) => Ok(translation),
                    Ok(None) => Err(no_lyrics.to_string()),
                    Err(e) => {
                        tracing::warn!("Lyrics translation lookup failed: {e}");
                        Err(e.to_string())
                    }
                };
                let _ = tx.send(BackendResult::LyricsTranslationFetched(
                    result, id, provider,
                ));
            });
            return Task::none();
        }
        state.selected_translation = language;
        state.loading_translation = None;
        Self::refresh_displayed_lyrics(state);
        self.sync_lyrics_editor(pane);
        self.scroll_lyrics_to_active(pane)
    }

    /// Open a blank editor for a new named custom entry.
    pub fn start_custom_lyrics_edit(&mut self, pane: PaneId) {
        let Some(track) = self.session.queue.current() else {
            return;
        };
        let track_id = track.cache_key();
        self.flush_note_draft(pane);
        let Some(state) = &mut self.pane_mut(pane).lyrics else {
            return;
        };
        Self::begin_custom_edit(
            state,
            track_id,
            iced::widget::text_editor::Content::default(),
            None,
        );
    }

    /// Open the editor prefilled with the named custom entry.
    pub fn edit_custom_lyrics(&mut self, pane: PaneId, name: String) {
        let Some(track) = self.session.queue.current() else {
            return;
        };
        let track_id = track.cache_key();
        let Some(entry) = self.lyrics_cache.get_custom(&track_id, &name) else {
            return;
        };
        let content = iced::widget::text_editor::Content::with_text(&entry.to_edit_text());
        self.flush_note_draft(pane);
        let Some(state) = &mut self.pane_mut(pane).lyrics else {
            return;
        };
        Self::begin_custom_edit(state, track_id, content, Some(name));
    }

    /// Show the named custom entry for the current track.
    pub fn select_custom_lyrics(&mut self, pane: PaneId, name: String) -> Task<Message> {
        let Some(track) = self.session.queue.current() else {
            return Task::none();
        };
        let track_id = track.cache_key();
        let Some(entry) = self.lyrics_cache.get_custom(&track_id, &name) else {
            return Task::none();
        };
        let custom_names = self.lyrics_cache.custom_names(&track_id);
        self.flush_note_draft(pane);
        if let Some(state) = &mut self.pane_mut(pane).lyrics {
            Self::apply_lyrics(state, &track_id, entry, Some(name), custom_names);
        }
        self.scroll_lyrics_to_active(pane)
    }

    pub fn save_custom_lyrics(&mut self, pane: PaneId) -> Task<Message> {
        let (text, name, track_id, edited) = self
            .pane(pane)
            .lyrics
            .as_ref()
            .map(|s| {
                (
                    s.edit_content.text(),
                    s.edit_name.trim().to_string(),
                    s.track_id.clone(),
                    s.editing_custom_name.clone(),
                )
            })
            .unwrap_or_default();
        if name.is_empty() {
            self.notify_error(self.strings.lyrics_name_empty);
            return Task::none();
        }
        let Some(lyrics) = crate::lyrics::Lyrics::from_custom_text(&text) else {
            self.notify_error(self.strings.lyrics_empty);
            return Task::none();
        };
        let Some(track_id) = track_id else {
            return Task::none();
        };
        self.lyrics_cache.insert_custom(&track_id, &name, &lyrics);
        if let Some(old) = &edited {
            if old != &name {
                self.lyrics_cache.remove_custom(&track_id, old);
            }
        }
        self.lyrics_cache.save();
        let custom_names = self.lyrics_cache.custom_names(&track_id);
        if let Some(state) = &mut self.pane_mut(pane).lyrics {
            Self::apply_lyrics(state, &track_id, lyrics, Some(name), custom_names);
        }
        self.notify(self.strings.lyrics_saved);
        self.scroll_lyrics_to_active(pane)
    }

    pub fn cancel_custom_lyrics_edit(&mut self, pane: PaneId) {
        if let Some(state) = &mut self.pane_mut(pane).lyrics {
            Self::end_custom_edit(state);
        }
        self.refresh_note_editor(pane);
    }

    pub fn delete_custom_lyrics(&mut self, pane: PaneId) {
        let (Some(track_id), Some(target)) = self
            .pane(pane)
            .lyrics
            .as_ref()
            .map(|s| {
                (
                    s.track_id.clone(),
                    s.editing_custom_name
                        .clone()
                        .or_else(|| s.selected_custom.clone()),
                )
            })
            .unwrap_or_default()
        else {
            return;
        };
        if self.lyrics_cache.remove_custom(&track_id, &target) {
            self.lyrics_cache.save();
        }
        self.flush_note_draft(pane);
        if let Some(state) = &mut self.pane_mut(pane).lyrics {
            Self::reset_lyrics(state);
        }
        self.ensure_lyrics_for_current(pane);
        self.notify(self.strings.lyrics_deleted);
    }

    /// Load (from cache) or fetch lyrics for the current track when we don't
    /// already hold them. Called when the overlay opens or switches provider,
    /// on every track change, and by the tick loop as a safety net.
    #[allow(clippy::too_many_lines)]
    pub(super) fn ensure_lyrics_for_current(&mut self, pane: PaneId) {
        self.ensure_lyrics_for_current_with(pane, false);
    }

    fn ensure_lyrics_for_current_with(&mut self, pane: PaneId, force: bool) {
        let Some(track) = self.session.queue.current() else {
            self.flush_note_draft(pane);
            if let Some(state) = &mut self.pane_mut(pane).lyrics {
                Self::reset_lyrics(state);
            }
            return;
        };
        let current_id = track.cache_key();
        let artist = track.artist_text();
        let title = track.title.clone();
        let album = track.album().map(|a| a.name.clone());
        let duration = track.duration();
        let no_lyrics = self.strings.no_lyrics_found;
        self.flush_note_draft(pane);
        let Some((provider, editing, same_track, loading, selected)) =
            self.pane(pane).lyrics.as_ref().map(|state| {
                (
                    state.provider,
                    state.editing,
                    state.track_id.as_deref() == Some(current_id.as_str()),
                    state.lyrics.is_loading(),
                    state.selected_custom.clone(),
                )
            })
        else {
            return;
        };

        if editing {
            if same_track && !force {
                return;
            }
            if let Some(state) = &mut self.pane_mut(pane).lyrics {
                Self::end_custom_edit(state);
            }
        }
        if same_track && !loading && !force {
            return;
        }
        if same_track
            && self
                .lyrics_requests
                .contains(&(current_id.clone(), provider))
        {
            return;
        }
        let selected = if same_track && !force { selected } else { None };
        let custom_names = self.lyrics_cache.custom_names(&current_id);
        let custom = selected.as_ref().and_then(|name| {
            self.lyrics_cache
                .get_custom(&current_id, name)
                .map(|lyrics| (name.clone(), lyrics))
        });
        let cached = if !force && custom.is_none() {
            self.lyrics_cache.get_for(&current_id, provider)
        } else {
            None
        };

        let Some(state) = &mut self.pane_mut(pane).lyrics else {
            return;
        };
        if let Some((name, custom)) = custom {
            Self::apply_lyrics(state, &current_id, custom, Some(name), custom_names);
            return;
        }
        if selected.is_some() {
            state.selected_custom = None;
        }
        if let Some(cached_lyrics) = cached {
            Self::apply_lyrics(state, &current_id, cached_lyrics, None, custom_names);
            return;
        }

        Self::load_lyrics(state, Some(current_id.clone()), custom_names);

        // One request per track/provider: other panes (and a close/reopen)
        // wait for its result instead of starting their own.
        if !self.lyrics_requests.insert((current_id.clone(), provider)) {
            return;
        }
        let req = crate::lyrics::LyricsRequest {
            artist,
            title,
            album: album.unwrap_or_default(),
            duration,
        };
        let tx = self.result_tx.clone();
        std::thread::spawn(move || {
            let result = match provider.fetch(&req) {
                Ok(Some(lyrics)) => Ok(lyrics),
                Ok(None) => Err(no_lyrics.to_string()),
                Err(e) => {
                    tracing::warn!("Lyrics lookup failed: {e}");
                    Err(e.to_string())
                }
            };
            let _ = tx.send(BackendResult::LyricsFetched(result, current_id, provider));
        });
    }

    fn apply_lyrics(
        state: &mut crate::app::LyricsState,
        track_id: &str,
        lyrics: crate::lyrics::Lyrics,
        custom_name: Option<String>,
        custom_names: Vec<String>,
    ) {
        state.track_id = Some(track_id.to_owned());
        state.selected_custom = custom_name;
        Self::end_custom_edit(state);
        state.custom_names = custom_names;
        Self::install_lyrics(state, Arc::new(lyrics));
        Self::sync_editor_content(state);
    }

    /// Drop the previous track's lyrics; every open lyrics pane immediately
    /// shows the new track's cached lyrics or starts one fetch for them.
    pub fn clear_lyrics_for_track_change(&mut self) {
        for pane in self.pane_ids() {
            self.flush_note_draft(pane);
            if let Some(state) = &mut self.pane_mut(pane).lyrics {
                Self::reset_lyrics(state);
            }
            self.ensure_lyrics_for_current(pane);
        }
    }

    /// Open the context menu for `pos` anchored at `point` (absolute window
    /// coordinates), instead of the live cursor. Used by the keyboard
    /// shortcuts, which anchor at the center of the hovered row.
    pub fn show_context_menu_at(&mut self, pos: TrackPos, point: Point) -> Task<Message> {
        let Some(track) = self.get_track_at(pos) else {
            return Task::none();
        };
        let TrackPos { index, pane } = pos;
        let in_playlist = is_main_pane(pane)
            && match &self.view_data_in(pane).kind {
                ViewKind::Playlist(_) => true,
                ViewKind::PlaylistView(r) => crate::providers::playlists_supported(r.provider),
                _ => false,
            };

        let sel = self.selection_in(pane);
        let target_indices = if sel.binary_search(&index).is_ok() {
            sel.to_vec()
        } else {
            vec![index]
        };
        let target_tracks = self.get_tracks_at(&target_indices, pane);

        // A menu can be opened from inside a dialog (the Edit popup's
        // manual search), which the single-overlay slot would otherwise
        // destroy: it is stashed here and put back when the menu closes.
        let return_to = self.dialog.take().map(Box::new);
        self.dialog = Some(Dialog::ContextMenu(
            crate::app::interaction::ContextMenuState {
                anchor: crate::app::interaction::MenuAnchor::at(point),
                pos,
                target_indices,
                target_tracks,
                in_playlist,
                is_trashed: self.is_trashed(&track),
                track,
                return_to,
            },
        ));
        CaptureContextMenu::default().into()
    }

    /// Convert a cursor point reported inside a track list to absolute window
    /// coordinates. Every scrollable above the row shifts the cursor it hands
    /// to its content by its own offset, so each one comes back off: the list
    /// itself, plus — on an artist page — the pane's card host, whose page
    /// scroll carries the popular-tracks list. Lists scroll vertically only,
    /// so only `y` shifts.
    pub(super) fn window_point(&self, pane: PaneId, point: Point) -> Point {
        let scroll = self
            .bounds
            .list_geo(pane)
            .map_or(0.0, |geo| geo.translation_y);
        Point::new(point.x, point.y - scroll - self.track_host_scroll(pane))
    }

    /// How far the scrollable that hosts `pane`'s track list is scrolled
    /// *above* the list. Only an artist page nests its popular-tracks list
    /// inside the pane's card host; every other view's list is its own page.
    fn track_host_scroll(&self, pane: PaneId) -> f32 {
        let nested_in_card_host =
            is_main_pane(pane) && matches!(self.view_data_in(pane).kind, ViewKind::Artist(_));
        if nested_in_card_host {
            self.bounds.scroll_offset(ScrollTarget::Cards(pane))
        } else {
            0.0
        }
    }

    /// Close the open menu and hand its state to the caller. `None` when no
    /// menu of that kind is open (the dialog is put back untouched).
    pub(crate) fn take_context_menu(&mut self) -> Option<ContextMenuState> {
        self.bounds.context_menu = None;
        match self.dialog.take() {
            Some(Dialog::ContextMenu(mut menu)) => {
                // Hand back the dialog the menu covered (the Edit popup's
                // manual search) so the action runs on top of it, as it would
                // with no menu at all.
                self.dialog = menu.return_to.take().map(|dialog| *dialog);
                Some(menu)
            }
            other => {
                self.dialog = other;
                None
            }
        }
    }

    /// The card-menu counterpart of [`take_context_menu`](Self::take_context_menu).
    pub(crate) fn take_card_menu(&mut self) -> Option<CardMenuState> {
        self.bounds.context_menu = None;
        match self.dialog.take() {
            Some(Dialog::CardMenu(menu)) => Some(menu),
            other => {
                self.dialog = other;
                None
            }
        }
    }

    /// The open menu's anchor, for hover focus and the window-edge flip.
    pub(crate) fn menu_anchor_mut(&mut self) -> Option<&mut MenuAnchor> {
        match &mut self.dialog {
            Some(Dialog::ContextMenu(menu)) => Some(&mut menu.anchor),
            Some(Dialog::CardMenu(menu)) => Some(&mut menu.anchor),
            _ => None,
        }
    }

    /// The pane a menu acts on: the row's own pane, or the focused one for the
    /// global queue/recent lists. `None` when that pane has since been closed.
    fn menu_pane(&self, menu: &ContextMenuState) -> Option<PaneId> {
        let pane = if is_main_pane(menu.pos.pane) {
            menu.pos.pane
        } else {
            self.session.focused
        };
        self.session.panes.contains_key(&pane).then_some(pane)
    }

    /// Open the context menu for the card at `target` (an artist, album or
    /// playlist card from a search tab, an artist page or the sidebar
    /// library), anchored at `point` in window coordinates. A card in a pane
    /// focuses it first, like clicking it does.
    pub fn show_card_menu_at(
        &mut self,
        pane: PaneId,
        target: crate::app::view_data::CardMenuTarget,
        point: Point,
    ) -> Task<Message> {
        self.session.focused = pane;
        self.dialog = Some(Dialog::CardMenu(CardMenuState {
            anchor: MenuAnchor::at(point),
            pane,
            target,
        }));
        CaptureContextMenu::default().into()
    }

    /// Convert a cursor point reported inside a card list to absolute window
    /// coordinates, like [`window_point`](Self::window_point): a scrollable
    /// hands its content a cursor shifted by the *whole* translation vector, so
    /// both axes may need undoing. `rail` names the card shelf the card sits in,
    /// which scrolls sideways on top of the host's vertical offset.
    pub(super) fn card_window_point(
        &self,
        host: super::operation::ScrollTarget,
        rail: Option<super::operation::RailKey>,
        point: Point,
    ) -> Point {
        let vertical = self.bounds.scroll_offset(host);
        let horizontal = match (host, rail) {
            (super::operation::ScrollTarget::Cards(pane), Some(rail)) => {
                self.bounds.rail_scroll(pane, rail)
            }
            _ => 0.0,
        };
        Point::new(point.x - horizontal, point.y - vertical)
    }

    /// Drill into the card the card menu was opened on, exactly as clicking it
    /// would: artist pages have their own load path, everything else goes
    /// through the generic browse flow.
    pub fn open_card(&mut self, pane: PaneId, target: &ViewKind) -> Task<Message> {
        let Some(card) = target.card_ref() else {
            return Task::none();
        };
        if matches!(target, ViewKind::Artist(_)) {
            return self.open_artist(pane, Some(card.id), card.title, card.provider, card.url);
        }
        self.handle_browse(pane, target, card.provider)
    }

    /// Open the card menu's public page in the desktop's default browser.
    pub fn handle_card_menu_open_in_browser(&mut self) {
        let Some(menu) = self.take_card_menu() else {
            return;
        };
        if let Some(url) = menu.card().web_url() {
            self.open_in_browser(url);
        }
    }

    /// Save the card menu's card in the library, or drop it if already saved —
    /// the menu's "Add to library" / "Remove from library" entry.
    pub fn handle_card_menu_toggle_library(&mut self) -> Task<Message> {
        let Some(menu) = self.take_card_menu() else {
            return Task::none();
        };
        self.toggle_library_save_with_toast(menu.target.card)
    }

    /// Save the card menu's album/playlist as a local playlist, fetching its
    /// tracks in the background.
    pub fn handle_card_menu_save_as_playlist(&mut self) -> Task<Message> {
        let Some(menu) = self.take_card_menu() else {
            return Task::none();
        };
        let item = menu.target.card;
        self.create_playlist_from_card(&item, self.playlists.playlists.len())
    }

    fn track_center_point(&self, pos: TrackPos) -> Option<Point> {
        let TrackPos { index, pane } = pos;
        let geo = self.bounds.list_geo(pane)?;
        let scroll = geo.translation_y + self.track_host_scroll(pane);
        let visual_index = index - super::pane_first_index(pane).min(index);
        Some(if let Some(row) = geo.rows.get(visual_index) {
            Point::new(row.x + row.width / 2.0, row.y - scroll + row.height / 2.0)
        } else {
            let row_y = visual_index as f32 * crate::theme::ROW_HEIGHT - scroll;
            Point::new(
                geo.bounds.x + geo.bounds.width / 2.0,
                geo.bounds.y + row_y + crate::theme::ROW_HEIGHT / 2.0,
            )
        })
    }

    /// Open the context menu for the hovered track, anchored at its row center
    /// — the keyboard equivalent of right-clicking the row. No-op when nothing
    /// is hovered (so the shortcut is inert outside a track list).
    pub fn open_context_menu_for_hovered_track(&mut self) -> Task<Message> {
        if let Some(pos) = self.drag.hovered_track() {
            let point = self.track_center_point(pos).unwrap_or(self.drag.cursor_pos);
            self.show_context_menu_at(pos, point)
        } else {
            Task::none()
        }
    }

    /// Open the artist page on `provider`, using the track's stored artist
    /// id when present and resolving by name otherwise.
    pub fn handle_context_menu_go_to_artist(&mut self, provider: ProviderId) -> Task<Message> {
        let Some(menu) = self.take_context_menu() else {
            return Task::none();
        };
        let Some(pane) = self.menu_pane(&menu) else {
            return Task::none();
        };
        // The artist's page, when the provider identifies its artists by a
        // public address (channel id, MBID, Last.fm name).
        let url = menu
            .track
            .provider_artist_id(provider)
            .and_then(|id| provider.artist_page_url(id))
            .unwrap_or_default();
        self.open_artist(
            pane,
            menu.track.provider_artist_id(provider),
            menu.track.primary_artist(),
            provider,
            &url,
        )
    }

    pub fn handle_context_menu_song_radio(&mut self, provider: ProviderId) -> Task<Message> {
        let Some(menu) = self.take_context_menu() else {
            return Task::none();
        };
        let Some(pane) = self.menu_pane(&menu) else {
            return Task::none();
        };
        self.start_radio_provider(pane, provider, &menu.track, false)
    }

    pub fn handle_context_menu_artist_radio(&mut self, provider: ProviderId) -> Task<Message> {
        let Some(menu) = self.take_context_menu() else {
            return Task::none();
        };
        let Some(pane) = self.menu_pane(&menu) else {
            return Task::none();
        };
        self.start_radio_provider(pane, provider, &menu.track, true)
    }

    /// Clear the stream cache for every selected track on `provider`.
    pub fn handle_context_menu_clear_cache(&mut self, provider: ProviderId) {
        let Some(menu) = self.take_context_menu() else {
            return;
        };
        let cleared = self.stream_cache.remove_many(
            menu.target_tracks
                .iter()
                .filter_map(|track| track.provider_id(provider).map(|id| (provider, id))),
        );
        self.notify((self.strings.cache_cleared_for)(
            cleared,
            &self.strings.provider(provider),
        ));
    }

    /// Clear the stream cache on the selected tracks' own source providers:
    /// the direct click on the "Clear cache" parent row. A mixed-source
    /// selection clears each track's source cache.
    pub fn handle_context_menu_clear_cache_current(&mut self) {
        let Some(menu) = self.take_context_menu() else {
            return;
        };
        let cleared = self.stream_cache.remove_many(
            menu.target_tracks
                .iter()
                .filter(|track| track.source != ProviderId::Local)
                .filter_map(|track| track.provider_id(track.source).map(|id| (track.source, id))),
        );
        self.notify((self.strings.cache_cleared_n)(cleared));
    }

    /// Open the context menu's track page in the desktop's default browser.
    /// The page is the first provider identity on the track that has one, so
    /// the item is only ever offered for such tracks.
    pub fn handle_context_menu_open_in_browser(&mut self) {
        let Some(menu) = self.take_context_menu() else {
            return;
        };
        if let Some(url) = menu.track.web_url() {
            self.open_in_browser(&url);
        }
    }

    /// Hand `url` to the desktop's default browser, reporting a launcher
    /// failure as a toast. Shared by every "Open in browser" entry (the track
    /// and card menus, the browse-page header button).
    pub fn open_in_browser(&mut self, url: &str) {
        if let Err(e) = crate::util::open_external(url) {
            tracing::warn!("opening {url} failed: {e}");
            self.notify_error((self.strings.open_in_browser_failed)(url));
        }
    }

    pub fn close_context_menu(&mut self) {
        let dialog = self.dialog.take();
        self.bounds.context_menu = None;
        // Hand back the dialog the menu covered (the Edit popup's manual
        // search) instead of leaving no overlay at all.
        if let Some(Dialog::ContextMenu(menu)) = dialog {
            self.dialog = menu.return_to.map(|dialog| *dialog);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::{
            pane::QUEUE_PANE_ID,
            update::operation::{ListGeometry, RailKey},
        },
        data::{
            config,
            library::{Card, LibraryItem},
        },
        providers::ArtistSectionKind,
    };

    #[test]
    fn right_click_anchor_subtracts_list_scroll() {
        let mut p = MusicPlayer::new_with(config::Config::default());
        let pane = p.session.focused;
        p.bounds.tracks.insert(
            pane,
            ListGeometry {
                translation_y: 200.0,
                ..ListGeometry::default()
            },
        );
        let anchored = p.window_point(pane, Point::new(300.0, 500.0));
        assert_eq!((anchored.x, anchored.y), (300.0, 300.0));

        let unmeasured = p.window_point(QUEUE_PANE_ID, Point::new(10.0, 20.0));
        assert_eq!((unmeasured.x, unmeasured.y), (10.0, 20.0));
    }

    /// An artist page scrolls as a whole, and its popular-tracks list scrolls
    /// inside that page: the row's point arrives shifted by both offsets.
    #[test]
    fn a_popular_list_artist_section_subtracts_the_page_scroll() {
        use crate::data::library::LibraryKind;

        let mut p = MusicPlayer::new_with(config::Config::default());
        let pane = p.session.focused;
        p.reset_test_pane(vec![crate::app::ViewData::default()]);
        p.bounds.cards.insert(
            pane,
            ListGeometry {
                translation_y: 480.0,
                ..ListGeometry::default()
            },
        );

        // Any other view's track list is its own page: the card host offset
        // belongs to no enclosing scrollable there.
        let anchored = p.window_point(pane, Point::new(300.0, 900.0));
        assert_eq!((anchored.x, anchored.y), (300.0, 900.0));

        p.view_data_in_mut(pane).kind = ViewKind::from(&LibraryItem {
            kind: LibraryKind::Artist,
            id: "ar1".into(),
            title: "Artist".into(),
            thumbnail: String::new(),
            provider: ProviderId::YouTube,
            url: String::new(),
        });
        // The popular list has its own scroll offset on top of the page's.
        p.bounds.tracks.insert(
            pane,
            ListGeometry {
                translation_y: 96.0,
                ..ListGeometry::default()
            },
        );
        let anchored = p.window_point(pane, Point::new(300.0, 900.0));
        assert_eq!((anchored.x, anchored.y), (300.0, 324.0));
    }

    /// A card's own view, as the three card hosts build it.
    fn album_view() -> crate::app::view_data::CardMenuTarget {
        let item = LibraryItem {
            kind: crate::data::library::LibraryKind::Album,
            id: "a1".into(),
            title: "Album".into(),
            thumbnail: String::new(),
            provider: ProviderId::YouTube,
            url: "https://music.youtube.com/browse/MPREb_a1".into(),
        };
        crate::app::view_data::CardMenuTarget::new(Card::new(item))
    }

    #[test]
    fn a_card_right_click_opens_a_menu_that_reads_off_the_target() {
        use crate::app::{interaction::CardAction, update::operation::ScrollTarget};

        let mut p = MusicPlayer::new_with(config::Config::default());
        let pane = p.session.focused;
        let _ = p.update(Message::CardRightClicked {
            host: ScrollTarget::Cards(pane),
            pane,
            target: album_view(),
            rail: None,
            point: Point::new(10.0, 20.0),
        });

        let Some(Dialog::CardMenu(menu)) = &p.dialog else {
            panic!("a card right-click should open the card menu");
        };
        assert_eq!(menu.pane, pane);
        assert_eq!(
            menu.actions(),
            vec![
                CardAction::Open,
                CardAction::OpenInBrowser,
                CardAction::SaveAsPlaylist,
                CardAction::ToggleLibrary,
            ]
        );
        assert!(!menu.saved(&p.library));

        // The library entry comes off the same view the card click opens.
        let _ = p.update(Message::CardMenuToggleLibrary);
        assert!(p.dialog.is_none());
        assert!(p
            .library
            .contains(crate::data::library::LibraryKind::Album, "a1"));
    }

    #[test]
    fn a_reported_scroll_never_invents_track_list_geometry() {
        use crate::app::update::operation::ScrollTarget;

        let mut p = MusicPlayer::new_with(config::Config::default());
        let pane = p.session.focused;
        let _ = p.update(Message::Scrolled(
            ScrollTarget::Tracks(pane),
            iced::Vector::new(0.0, 90.0),
        ));
        // Track lists are virtualized off captured bounds, so an offset that
        // arrives before the capture pass must be dropped, not stored against
        // empty bounds (which would look like "fits on screen").
        assert!(p.bounds.tracks.is_empty());
        assert!(p.bounds.queue.is_none());
        assert!(p.bounds.library.is_none());

        // A card host only ever carries an offset, so it can record one.
        let _ = p.update(Message::Scrolled(
            ScrollTarget::Cards(pane),
            iced::Vector::new(0.0, 90.0),
        ));
        assert_eq!(p.bounds.scroll_offset(ScrollTarget::Cards(pane)), 90.0);
    }

    #[test]
    fn card_right_click_undoes_the_host_scroll_on_both_axes() {
        use crate::app::update::operation::ScrollTarget;

        let mut p = MusicPlayer::new_with(config::Config::default());
        let pane = p.session.focused;
        p.bounds.cards.insert(
            pane,
            ListGeometry {
                translation_y: 120.0,
                ..ListGeometry::default()
            },
        );

        // A card in a horizontally scrolled artist-page shelf: both the
        // shelf's x offset and the page's y offset come off.
        p.bounds
            .card_rows
            .insert((pane, RailKey::Artist(ArtistSectionKind::Albums)), 210.0);
        let anchored = p.card_window_point(
            ScrollTarget::Cards(pane),
            Some(RailKey::Artist(ArtistSectionKind::Albums)),
            Point::new(300.0, 500.0),
        );
        assert_eq!((anchored.x, anchored.y), (90.0, 380.0));

        // The same host without a shelf only shifts y.
        let vertical =
            p.card_window_point(ScrollTarget::Cards(pane), None, Point::new(300.0, 500.0));
        assert_eq!((vertical.x, vertical.y), (300.0, 380.0));

        // A sidebar row reads the library list's offset, and a shelf with no
        // captured offset yet (a section still loading) leaves x alone.
        p.bounds.library = Some(ListGeometry {
            translation_y: 40.0,
            ..ListGeometry::default()
        });
        let sidebar = p.card_window_point(
            ScrollTarget::Library,
            Some(RailKey::Artist(ArtistSectionKind::Related)),
            Point::new(30.0, 300.0),
        );
        assert_eq!((sidebar.x, sidebar.y), (30.0, 260.0));
    }
}
