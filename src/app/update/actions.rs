use iced::{widget::operation, Point, Task};

use super::{BackendResult, MusicPlayer, Track};
use crate::{
    app::{
        dialog::Dialog,
        interaction::{TrackListKind, TrackPos},
        update::operation::{CaptureBounds, CaptureContextMenu},
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

    pub fn handle_toggle_picker(&mut self, indices: Vec<usize>, list: TrackListKind) {
        if matches!(self.dialog, Some(Dialog::Picker(_))) {
            self.dialog = None;
        } else {
            self.dialog = Some(Dialog::Picker(PlaylistPicker { indices, list }));
        }
    }

    /// Toggle the lyrics overlay for the current track.
    pub fn handle_show_lyrics(&mut self) -> Task<Message> {
        if self.lyrics.is_some() {
            self.lyrics = None;
            CaptureBounds::new().into()
        } else {
            self.lyrics = Some(crate::app::LyricsState::new());
            self.scroll_lyrics_to_active()
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
        if state.mode == LyricsViewMode::Selectable {
            state.editor = iced::widget::text_editor::Content::with_text(&text);
        }
    }

    pub fn set_lyrics_view_mode(&mut self, mode: LyricsViewMode) -> Task<Message> {
        let Some(state) = &mut self.lyrics else {
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
        self.sync_lyrics_editor();
        if mode == LyricsViewMode::Synced {
            self.scroll_lyrics_to_active()
        } else {
            Task::none()
        }
    }

    pub(super) fn lyrics_active(&self) -> Option<(usize, usize)> {
        let state = self.lyrics.as_ref()?;
        if state.mode != LyricsViewMode::Synced {
            return None;
        }
        let LoadState::Ready(lyrics) = &state.lyrics else {
            return None;
        };
        if lyrics.timed.is_empty() {
            return None;
        }
        let position = self.progress * self.duration;
        Some((lyrics.active_index(position)?, lyrics.timed.len()))
    }

    fn lyrics_scroll_task(&mut self, index: usize, total: usize) -> Task<Message> {
        use crate::app::ui::LYRICS_SCROLL_ID;
        if let Some(vp) = self.lyrics.as_ref().and_then(|s| s.viewport) {
            let avg = vp.content_h / total.max(1) as f32;
            let max = (vp.content_h - vp.height).max(0.0);
            let y = ((index as f32 + 0.5) * avg - vp.height / 2.0).clamp(0.0, max);
            if let Some(state) = &mut self.lyrics {
                if let Some(v) = &mut state.viewport {
                    v.offset_y = y;
                }
            }
            return operation::scroll_to::<Message>(
                LYRICS_SCROLL_ID.clone(),
                operation::AbsoluteOffset { x: 0.0, y },
            );
        }
        let frac = if total > 1 {
            index as f32 / (total - 1) as f32
        } else {
            0.0
        };
        operation::snap_to::<Message>(
            LYRICS_SCROLL_ID.clone(),
            operation::RelativeOffset { x: 0.0, y: frac },
        )
    }

    /// Whether the lyrics line at `index` overlaps the viewport, using
    /// the average line pitch from the last `on_scroll` viewport.
    /// `None` when no viewport is known yet.
    fn lyrics_line_visible(&self, index: usize, total: usize) -> Option<bool> {
        const EPS: f32 = 1.0;
        let vp = self.lyrics.as_ref().and_then(|s| s.viewport)?;
        if total == 0 {
            return None;
        }
        let avg = vp.content_h / total as f32;
        let top = index as f32 * avg;
        let bottom = top + avg;
        Some(top < vp.offset_y + vp.height + EPS && bottom > vp.offset_y - EPS)
    }

    pub(super) fn scroll_lyrics_to_active(&mut self) -> Task<Message> {
        let Some((index, total)) = self.lyrics_active() else {
            return Task::none();
        };
        if let Some(state) = &mut self.lyrics {
            state.scrolled_to = Some(index);
        }
        self.lyrics_scroll_task(index, total)
    }

    pub(super) fn maybe_autoscroll_lyrics(&mut self) -> Task<Message> {
        let Some((active, total)) = self.lyrics_active() else {
            return Task::none();
        };
        let already = self.lyrics.as_ref().and_then(|s| s.scrolled_to);
        if already == Some(active) {
            return Task::none();
        }
        if let Some(state) = &mut self.lyrics {
            state.scrolled_to = Some(active);
        }
        if self.lyrics_line_visible(active, total) == Some(false) {
            return Task::none();
        }
        self.lyrics_scroll_task(active, total)
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
                state.scrolled_to = None;
                state.viewport = None;
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
            let mode = LyricsViewMode::for_lyrics(&cached_lyrics);
            state.lyrics = crate::load_state::LoadState::Ready(cached_lyrics);
            state.track_id = Some(current_id.clone());
            state.mode = mode;
            state.scrolled_to = None;
            state.viewport = None;
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
        state.scrolled_to = None;
        state.viewport = None;
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
            state.scrolled_to = None;
            state.viewport = None;
        }
        self.sync_lyrics_editor();
    }

    /// Open the context menu for `pos` anchored at `point` (absolute window
    /// coordinates), instead of the live cursor. Used by the keyboard
    /// shortcuts, which anchor at the center of the hovered row.
    pub fn show_context_menu_at(&mut self, pos: TrackPos, point: Point) -> Task<Message> {
        let Some(track) = self.get_track_at(pos) else {
            return Task::none();
        };
        let TrackPos { index, list } = pos;

        let sel = self.selection(list);
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
                in_playlist: matches!(self.view_data().kind, ViewKind::Playlist(_)),
                track,
                hovered: None,
            },
        ));
        CaptureContextMenu::default().into()
    }

    fn track_center_point(&self, pos: TrackPos) -> Option<Point> {
        let TrackPos { index, list } = pos;
        let geo = match list {
            TrackListKind::Queue => self.bounds.queue.as_ref(),
            TrackListKind::Active => self.bounds.track.as_ref(),
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
        self.open_artist(
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
        self.start_radio_provider(provider, &menu.track, false)
    }

    pub fn handle_context_menu_artist_radio(&mut self, provider: ProviderId) -> Task<Message> {
        self.bounds.context_menu = None;
        let dialog = self.dialog.take();
        let Some(Dialog::ContextMenu(menu)) = dialog else {
            self.dialog = dialog;
            return Task::none();
        };
        self.start_radio_provider(provider, &menu.track, true)
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
