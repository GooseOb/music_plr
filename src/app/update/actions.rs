use iced::{widget::operation, Point, Task};

use super::{BackendResult, MusicPlayer, Track};
use crate::{
    app::{
        dialog::Dialog,
        interaction::{TrackListKind, TrackPos},
        pane::PaneId,
        update::operation::CaptureContextMenu,
        EditTrackState, LyricsViewMode, Message, PlaylistPicker, ViewKind,
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
                        provider.label().to_string(),
                    ));
                }
                Err(e) => {
                    let _ = tx.send(BackendResult::DownloadError(e.to_string()));
                }
            }
        });
    }

    pub fn handle_toggle_picker(&mut self, pane: PaneId, indices: Vec<usize>, list: TrackListKind) {
        if matches!(self.dialog, Some(Dialog::Picker(_))) {
            self.dialog = None;
        } else {
            self.dialog = Some(Dialog::Picker(PlaylistPicker {
                indices,
                list,
                pane,
            }));
        }
    }

    /// Toggle the lyrics overlay for the current track.
    pub fn handle_show_lyrics(&mut self, pane: PaneId) -> Task<Message> {
        if self.pane(pane).lyrics.is_some() {
            self.pane_mut(pane).lyrics = None;
            self.capture_bounds_task()
        } else {
            let provider = self.lyrics_provider;
            self.pane_mut(pane).lyrics = Some(crate::app::LyricsState::new(provider));
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
        let text = match state.displayed_lyrics() {
            Some(lyrics) => {
                if lyrics.has_timed() {
                    lyrics
                        .lines
                        .iter()
                        .map(|line| line.text.as_str())
                        .collect::<Vec<_>>()
                        .join("\n")
                } else {
                    lyrics.plain.clone()
                }
            }
            None => String::new(),
        };
        if state.mode == LyricsViewMode::Selectable {
            state.editor = iced::widget::text_editor::Content::with_text(&text);
        }
    }

    pub(super) fn flush_note_draft(&mut self, pane: PaneId) {
        let (idx, text) = match self.pane(pane).lyrics.as_ref() {
            Some(state) => match state.note_line {
                Some(idx) => (idx, state.note_editor.text()),
                None => return,
            },
            None => return,
        };
        let text = text.trim_end().to_string();
        let Some(state) = &mut self.pane_mut(pane).lyrics else {
            return;
        };
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
        // Note drafts on provider lyrics are session-only; custom entries
        // never show a translation, so `lyrics` is the entry itself here.
        if !translated {
            if let (Some(track_id), Some(name)) = (track_id, custom_name) {
                let mut cache = crate::data::lyrics_cache::LyricsCache::load_migrated();
                cache.insert_custom(&track_id, &name, lyrics);
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
            (Some(lyrics), Some(idx)) => lyrics
                .lines
                .get(idx)
                .map_or(String::new(), |line| line.description.clone()),
            _ => String::new(),
        };
        state.note_editor = iced::widget::text_editor::Content::with_text(&text);
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
        let position = self.progress * self.duration;
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

    /// Switch the lyrics provider of one pane (and the default for newly
    /// opened panes), persist it, and force that pane to refetch.
    pub fn handle_select_lyrics_provider(
        &mut self,
        pane: PaneId,
        provider: crate::lyrics::LyricsProvider,
    ) {
        self.lyrics_provider = provider;
        self.flush_note_draft(pane);
        if let Some(state) = &mut self.pane_mut(pane).lyrics {
            state.provider = provider;
            state.lyrics = crate::load_state::LoadState::Loading;
            state.track_id = None;
            state.scrolled_to = None;
            state.viewport = None;
            state.editing = false;
            state.editing_custom_name = None;
            state.selected_custom = None;
            state.reset_translation_state();
            state.custom_names = Vec::new();
            state.reset_note_state();
        }
        self.sync_lyrics_editor(pane);
        self.save_session();
    }

    /// Switch the displayed lyrics of one pane between the original and a
    /// loaded translation. Unloaded translations are fetched in the
    /// background; the pane keeps showing the original meanwhile.
    pub fn handle_select_lyrics_translation(
        &mut self,
        pane: PaneId,
        language: Option<String>,
    ) -> Task<Message> {
        let Some(track) = self.queue.current() else {
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
        match language {
            None => {
                state.selected_translation = None;
                state.loading_translation = None;
                if let Some(lyrics) = state.displayed_lyrics() {
                    state.mode = LyricsViewMode::for_lyrics(lyrics);
                }
                state.scrolled_to = None;
                state.viewport = None;
                state.reset_note_state();
            }
            Some(language) => match source {
                None => {
                    state.selected_translation = Some(language);
                    state.loading_translation = None;
                    if let Some(lyrics) = state.displayed_lyrics() {
                        state.mode = LyricsViewMode::for_lyrics(lyrics);
                    }
                    state.scrolled_to = None;
                    state.viewport = None;
                    state.reset_note_state();
                }
                Some(source) => {
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
            },
        }
        self.sync_lyrics_editor(pane);
        self.scroll_lyrics_to_active(pane)
    }

    /// Open a blank editor for a new named custom entry.
    pub fn start_custom_lyrics_edit(&mut self, pane: PaneId) {
        let Some(track) = self.queue.current() else {
            return;
        };
        let track_id = track.cache_key();
        self.flush_note_draft(pane);
        let Some(state) = &mut self.pane_mut(pane).lyrics else {
            return;
        };
        state.track_id = Some(track_id);
        state.edit_name = String::new();
        state.edit_content = iced::widget::text_editor::Content::default();
        state.editing = true;
        state.editing_custom_name = None;
        state.viewport = None;
        state.reset_note_state();
    }

    /// Open the editor prefilled with the named custom entry.
    pub fn edit_custom_lyrics(&mut self, pane: PaneId, name: String) {
        let Some(track) = self.queue.current() else {
            return;
        };
        let track_id = track.cache_key();
        let Some(entry) =
            crate::data::lyrics_cache::LyricsCache::load_migrated().get_custom(&track_id, &name)
        else {
            return;
        };
        self.flush_note_draft(pane);
        let Some(state) = &mut self.pane_mut(pane).lyrics else {
            return;
        };
        state.track_id = Some(track_id);
        state.edit_name.clone_from(&name);
        state.edit_content = iced::widget::text_editor::Content::with_text(&entry.to_edit_text());
        state.editing = true;
        state.editing_custom_name = Some(name);
        state.viewport = None;
        state.reset_note_state();
    }

    /// Show the named custom entry for the current track.
    pub fn select_custom_lyrics(&mut self, pane: PaneId, name: String) -> Task<Message> {
        let Some(track) = self.queue.current() else {
            return Task::none();
        };
        let track_id = track.cache_key();
        let cache = crate::data::lyrics_cache::LyricsCache::load_migrated();
        let Some(entry) = cache.get_custom(&track_id, &name) else {
            return Task::none();
        };
        let custom_names = cache.custom_names(&track_id);
        self.flush_note_draft(pane);
        if let Some(state) = &mut self.pane_mut(pane).lyrics {
            let mode = LyricsViewMode::for_lyrics(&entry);
            state.lyrics = crate::load_state::LoadState::Ready(entry);
            state.mode = mode;
            state.track_id = Some(track_id);
            state.selected_custom = Some(name);
            state.reset_translation_state();
            state.custom_names = custom_names;
            state.editing = false;
            state.scrolled_to = None;
            state.viewport = None;
            state.reset_note_state();
        }
        self.sync_lyrics_editor(pane);
        self.scroll_lyrics_to_active(pane)
    }

    pub fn save_custom_lyrics(&mut self, pane: PaneId) -> Task<Message> {
        let (text, name) = self
            .pane(pane)
            .lyrics
            .as_ref()
            .map(|s| (s.edit_content.text(), s.edit_name.trim().to_string()))
            .unwrap_or_default();
        if name.is_empty() {
            self.notify_error(self.strings.lyrics_name_empty.to_string());
            return Task::none();
        }
        let Some(lyrics) = crate::lyrics::Lyrics::from_custom_text(&text) else {
            self.notify_error(self.strings.lyrics_empty.to_string());
            return Task::none();
        };
        let Some(track_id) = self
            .pane(pane)
            .lyrics
            .as_ref()
            .and_then(|s| s.track_id.clone())
        else {
            return Task::none();
        };
        let edited = self
            .pane(pane)
            .lyrics
            .as_ref()
            .and_then(|s| s.editing_custom_name.clone());
        let mut cache = crate::data::lyrics_cache::LyricsCache::load_migrated();
        cache.insert_custom(&track_id, &name, &lyrics);
        if let Some(old) = edited {
            if old != name {
                cache.remove_custom(&track_id, &old);
            }
        }
        let custom_names = cache.custom_names(&track_id);
        if let Some(state) = &mut self.pane_mut(pane).lyrics {
            let mode = LyricsViewMode::for_lyrics(&lyrics);
            state.lyrics = crate::load_state::LoadState::Ready(lyrics);
            state.mode = mode;
            state.track_id = Some(track_id);
            state.selected_custom = Some(name);
            state.reset_translation_state();
            state.custom_names = custom_names;
            state.editing = false;
            state.editing_custom_name = None;
            state.scrolled_to = None;
            state.viewport = None;
            state.reset_note_state();
        }
        self.sync_lyrics_editor(pane);
        self.notify(self.strings.lyrics_saved);
        self.scroll_lyrics_to_active(pane)
    }

    pub fn cancel_custom_lyrics_edit(&mut self, pane: PaneId) {
        if let Some(state) = &mut self.pane_mut(pane).lyrics {
            state.editing = false;
            state.editing_custom_name = None;
        }
        self.refresh_note_editor(pane);
    }

    pub fn delete_custom_lyrics(&mut self, pane: PaneId) {
        let Some(track_id) = self
            .pane(pane)
            .lyrics
            .as_ref()
            .and_then(|s| s.track_id.clone())
        else {
            return;
        };
        let target = self
            .pane(pane)
            .lyrics
            .as_ref()
            .and_then(|s| s.editing_custom_name.clone())
            .or_else(|| {
                self.pane(pane)
                    .lyrics
                    .as_ref()
                    .and_then(|s| s.selected_custom.clone())
            });
        let Some(target) = target else {
            return;
        };
        let mut cache = crate::data::lyrics_cache::LyricsCache::load_migrated();
        cache.remove_custom(&track_id, &target);
        let custom_names = cache.custom_names(&track_id);
        self.flush_note_draft(pane);
        if let Some(state) = &mut self.pane_mut(pane).lyrics {
            state.editing = false;
            state.editing_custom_name = None;
            state.selected_custom = None;
            state.reset_translation_state();
            state.custom_names = custom_names;
            state.lyrics = crate::load_state::LoadState::Loading;
            state.scrolled_to = None;
            state.viewport = None;
            state.reset_note_state();
        }
        self.sync_lyrics_editor(pane);
        self.notify(self.strings.lyrics_deleted);
    }

    /// Load (from cache) or fetch lyrics for the current track when we don't
    /// already hold them; driven by the tick loop so it reacts to the overlay
    /// being shown and track changes.
    #[allow(clippy::too_many_lines)]
    pub(super) fn ensure_lyrics_for_current(&mut self, pane: PaneId) {
        let Some(track) = self.queue.current() else {
            self.flush_note_draft(pane);
            if let Some(state) = &mut self.pane_mut(pane).lyrics {
                state.lyrics = crate::load_state::LoadState::Loading;
                state.track_id = None;
                state.scrolled_to = None;
                state.viewport = None;
                state.editing = false;
                state.editing_custom_name = None;
                state.selected_custom = None;
                state.reset_translation_state();
                state.custom_names = Vec::new();
                state.reset_note_state();
            }
            self.sync_lyrics_editor(pane);
            return;
        };
        let current_id = track.cache_key();
        let artist = track.artist.clone();
        let title = track.title.clone();
        let album = track.album().map(|a| a.name.clone());
        let duration = track.duration();
        let tx = self.result_tx.clone();
        let no_lyrics = self.strings.no_lyrics_found;
        self.flush_note_draft(pane);
        let Some(state) = &mut self.pane_mut(pane).lyrics else {
            return;
        };
        let provider = state.provider;

        if state.editing {
            if state.track_id.as_deref() == Some(current_id.as_str()) {
                return;
            }
            state.editing = false;
            state.editing_custom_name = None;
        }
        let same_track = state.track_id.as_deref() == Some(current_id.as_str());
        if same_track && !state.lyrics.is_loading() {
            return;
        }
        if !same_track {
            state.selected_custom = None;
            state.reset_translation_state();
        }
        let cache = crate::data::lyrics_cache::LyricsCache::load_migrated();
        let custom_names = cache.custom_names(&current_id);
        if let Some(name) = state.selected_custom.clone() {
            if let Some(custom) = cache.get_custom(&current_id, &name) {
                let mode = LyricsViewMode::for_lyrics(&custom);
                state.lyrics = crate::load_state::LoadState::Ready(custom);
                state.track_id = Some(current_id.clone());
                state.mode = mode;
                state.scrolled_to = None;
                state.viewport = None;
                state.custom_names = custom_names;
                state.reset_translation_state();
                state.reset_note_state();
                Self::sync_editor_content(state);
                return;
            }
            state.selected_custom = None;
        }
        let cached = cache.get_for(&current_id, provider);
        if let Some(cached_lyrics) = cached {
            let mode = LyricsViewMode::for_lyrics(&cached_lyrics);
            state.lyrics = crate::load_state::LoadState::Ready(cached_lyrics);
            state.track_id = Some(current_id.clone());
            state.mode = mode;
            state.scrolled_to = None;
            state.viewport = None;
            state.custom_names = custom_names;
            state.reset_translation_state();
            state.reset_note_state();
            Self::sync_editor_content(state);
            return;
        }

        let req = crate::lyrics::LyricsRequest {
            artist,
            title,
            album: album.unwrap_or_default(),
            duration,
        };
        let id = current_id.clone();
        state.lyrics = crate::load_state::LoadState::Loading;
        state.track_id = Some(id.clone());
        state.scrolled_to = None;
        state.viewport = None;
        state.custom_names = custom_names;
        state.reset_translation_state();
        state.reset_note_state();
        Self::sync_editor_content(state);
        std::thread::spawn(move || {
            let result = match provider.fetch(&req) {
                Ok(Some(lyrics)) => Ok(lyrics),
                Ok(None) => Err(no_lyrics.to_string()),
                Err(e) => {
                    tracing::warn!("Lyrics lookup failed: {e}");
                    Err(e.to_string())
                }
            };
            let _ = tx.send(BackendResult::LyricsFetched(result, id, provider));
        });
    }

    /// Drop loaded lyrics when the track changes; every open lyrics pane
    /// stays open and refetches for the new track.
    pub fn clear_lyrics_for_track_change(&mut self) {
        for pane in self.pane_ids() {
            self.flush_note_draft(pane);
            if let Some(state) = &mut self.pane_mut(pane).lyrics {
                state.lyrics = crate::load_state::LoadState::Loading;
                state.track_id = None;
                state.scrolled_to = None;
                state.viewport = None;
                state.editing = false;
                state.editing_custom_name = None;
                state.selected_custom = None;
                state.reset_translation_state();
                state.custom_names = Vec::new();
                state.reset_note_state();
            }
            self.sync_lyrics_editor(pane);
        }
    }

    /// Open the context menu for `pos` anchored at `point` (absolute window
    /// coordinates), instead of the live cursor. Used by the keyboard
    /// shortcuts, which anchor at the center of the hovered row.
    pub fn show_context_menu_at(&mut self, pos: TrackPos, point: Point) -> Task<Message> {
        let Some(track) = self.get_track_at(pos) else {
            return Task::none();
        };
        let TrackPos { index, list, pane } = pos;
        self.focused_pane_id = pane;

        let sel = self.selection_in(pane, list);
        let target_indices = if sel.contains(&index) {
            sel.to_vec()
        } else {
            vec![index]
        };

        self.dialog = Some(Dialog::ContextMenu(
            crate::app::interaction::ContextMenuState {
                pos,
                target_indices,
                position: (point.x, point.y),
                cursor: (point.x, point.y),
                in_playlist: matches!(self.view_data_in(pane).kind, ViewKind::Playlist(_)),
                track,
                hovered: None,
            },
        ));
        CaptureContextMenu::default().into()
    }

    fn track_center_point(&self, pos: TrackPos) -> Option<Point> {
        let TrackPos { index, list, pane } = pos;
        let geo = match list {
            TrackListKind::Queue => self.bounds.queue.as_ref(),
            TrackListKind::Active => self.bounds.track_geo(pane),
            TrackListKind::Recent => self.bounds.recent.as_ref(),
        }?;
        let scroll = geo.translation_y;
        let visual_index = index - list.first_index().min(index);
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
        self.bounds.context_menu = None;
        let dialog = self.dialog.take();
        let Some(Dialog::ContextMenu(menu)) = dialog else {
            self.dialog = dialog;
            return Task::none();
        };
        let pane = menu.pos.pane;
        self.open_artist(
            pane,
            menu.track.provider_artist_id(provider),
            &menu.track.artist,
            provider,
        )
    }

    pub fn handle_context_menu_song_radio(&mut self, provider: ProviderId) -> Task<Message> {
        self.bounds.context_menu = None;
        let dialog = self.dialog.take();
        let Some(Dialog::ContextMenu(menu)) = dialog else {
            self.dialog = dialog;
            return Task::none();
        };
        let pane = menu.pos.pane;
        self.start_radio_provider(pane, provider, &menu.track, false)
    }

    pub fn handle_context_menu_artist_radio(&mut self, provider: ProviderId) -> Task<Message> {
        self.bounds.context_menu = None;
        let dialog = self.dialog.take();
        let Some(Dialog::ContextMenu(menu)) = dialog else {
            self.dialog = dialog;
            return Task::none();
        };
        let pane = menu.pos.pane;
        self.start_radio_provider(pane, provider, &menu.track, true)
    }

    /// Clear the stream cache for the context menu's track on `provider`.
    pub fn handle_context_menu_clear_cache(&mut self, provider: ProviderId) {
        self.bounds.context_menu = None;
        let dialog = self.dialog.take();
        let Some(Dialog::ContextMenu(menu)) = dialog else {
            self.dialog = dialog;
            return;
        };
        if let Some(id) = menu.track.provider_id(provider).map(str::to_string) {
            if self.stream_cache.remove(provider, &id) {
                self.notify((self.strings.cache_cleared_for)(provider.label()));
            }
        }
    }

    /// Clear the stream cache for the context menu's track on its current
    /// (source) provider: the direct click on the "Clear cache" parent row.
    pub fn handle_context_menu_clear_cache_current(&mut self) {
        let Some(Dialog::ContextMenu(menu)) = &self.dialog else {
            return;
        };
        let provider = menu.track.source;
        self.handle_context_menu_clear_cache(provider);
    }

    pub fn close_context_menu(&mut self) {
        self.dialog = None;
        self.bounds.context_menu = None;
    }

    /// Open the track-editing popup for the track at `pos`, seeding the
    /// working copy from the live track. Only one track is edited at a time
    /// (the right-clicked one), so multi-selection is ignored here.
    pub fn open_edit_track(&mut self, pos: TrackPos) {
        let Some(track) = self.get_track_at(pos) else {
            return;
        };
        self.dialog = Some(Dialog::Edit(EditTrackState {
            title: track.title.clone(),
            artist: track.artist.clone(),
            source: track.source,
            original: track,
            pos,
            finding: None,
        }));
    }

    /// Resolve an unresolved provider for the track being edited: the "Find"
    /// button in the Edit Track popup. Searches the provider for the track's
    /// title/artist and, on success, merges the resolved identity into the
    /// working copy so it can later be selected as the source.
    pub fn handle_edit_track_find_provider(&mut self, provider: ProviderId) {
        let Some(Dialog::Edit(edit)) = &mut self.dialog else {
            return;
        };
        if edit.finding.is_some() {
            return;
        }
        edit.finding = Some(provider);
        let original = edit.original.clone();
        let tx = self.result_tx.clone();
        std::thread::spawn(move || {
            let msg = match crate::providers::resolve_id(provider, &original) {
                Ok(resolved) => {
                    crate::app::BackendResult::EditTrackProviderResolved(provider, resolved)
                }
                Err(e) => {
                    crate::app::BackendResult::EditTrackProviderError(provider, e.to_string())
                }
            };
            let _ = tx.send(msg);
        });
    }

    /// Merge a resolved provider identity (from the Edit Track "Find" action)
    /// into the working copy. `None` means no match was found; `Some(track)`
    /// carries that provider's identity (and its rich metadata).
    pub fn apply_edit_track_provider_resolution(
        &mut self,
        provider: ProviderId,
        resolved: Option<Track>,
    ) {
        let found_on = self.strings.found_on;
        let could_not_find_on = self.strings.could_not_find_on;
        {
            let Some(Dialog::Edit(edit)) = &mut self.dialog else {
                return;
            };
            edit.finding = None;
            if let Some(track) = resolved {
                if let Some(pt) = track.providers.get(&provider) {
                    edit.original.set_provider(provider, pt.clone());
                }
            } else {
                let title = edit.title.clone();
                let msg = could_not_find_on(&title, provider.label());
                let _ = edit;
                self.notify_error(msg);
                return;
            }
        }
        self.notify(found_on(provider.label()));
    }

    /// Apply the edited fields back to the track's source list and close the
    /// popup. `source` follows the working copy (changed via the provider
    /// "select" buttons); `title`/`artist` are overwritten from the inputs.
    pub fn apply_edit_track(&mut self) {
        let dialog = self.dialog.take();
        let Some(Dialog::Edit(edit)) = dialog else {
            self.dialog = dialog;
            return;
        };
        let mut track = edit.original;
        track.title = edit.title;
        track.artist = edit.artist;
        track.source = edit.source;
        self.set_track_at(edit.pos, track);
        self.playlists.save();
        self.save_session();
    }
}
