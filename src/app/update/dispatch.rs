//! Top-level `update` / `subscription` dispatch.
//!
//! This is the one place that matches over every `Message` variant; each arm
//! is a one-to-three-line delegation to a handler in a sibling `app/update/*`
//! module. Keeping the dispatcher here (rather than in `app/state.rs`) keeps
//! the root state file focused on construction and the active-view accessors.

use std::time::Duration;

use iced::{Subscription, Task};

use crate::{
    app::{
        dialog::Dialog,
        import::{ImportCsvField, ImportPlaylistDialog},
        interaction::DefaultCtxAction,
        message::{BackendResult, EditTrackField, Message},
        pane::{PaneId, MANUAL_SEARCH_PANE_ID, QUEUE_PANE_ID, RECENT_PANE_ID},
        update::operation::{CaptureContextMenu, CaptureSearchHistoryRows, ContextMenuGeometry},
        PendingAdd,
    },
    deps::DepKind,
    providers::ProviderId,
    types::QueueTab,
};

impl crate::app::MusicPlayer {
    /// Entry point for every message: dispatch, then repair the captured
    /// geometry if what the lists render has changed.
    ///
    /// The capture pass is a snapshot taken on navigation, resize and results,
    /// so a list whose *content* changed afterwards would keep rendering off
    /// the old geometry until the next snapshot. Comparing the rendered row
    /// counts keeps that invariant here, once, instead of asking every mutation
    /// site to remember a `capture_bounds_task()` — the reason a list that was
    /// empty (or shorter) when last captured could look empty until an
    /// unrelated capture happened.
    pub fn update(&mut self, message: Message) -> Task<Message> {
        let task = self.handle_message(message);
        if self.rendered_shape_changed() {
            task.chain(self.capture_bounds_task())
        } else {
            task
        }
    }

    /// The row counts of every list `view` virtualizes from captured geometry:
    /// each pane's track list (skipping panes showing lyrics, which render no
    /// list), the queue panel's active tab, and the Edit popup's manual search.
    /// The queue's own scrollable holds "up next", so its now-playing row does
    /// not count.
    fn list_shape(&self) -> Vec<(PaneId, usize)> {
        let mut shape: Vec<(PaneId, usize)> = self
            .panes
            .iter()
            .filter(|(_, pane)| pane.lyrics.is_none())
            .map(|(&pane, _)| (pane, self.track_count_in(pane)))
            .filter(|(_, count)| *count > 0)
            .collect();
        if self.show_queue {
            let (pane, count) = match self.queue.queue_tab {
                QueueTab::Queue => (QUEUE_PANE_ID, self.queue.tracks.len().saturating_sub(1)),
                QueueTab::RecentlyPlayed => (RECENT_PANE_ID, self.queue.recently_played.len()),
            };
            if count > 0 {
                shape.push((pane, count));
            }
        }
        if let Some(manual) = self.edit_manual() {
            let count = manual.tracks().len();
            if count > 0 {
                shape.push((MANUAL_SEARCH_PANE_ID, count));
            }
        }
        shape.sort_unstable();
        shape
    }

    /// True when the current shape differs from the one last dispatched, after
    /// recording it — one capture per change, so this can never loop.
    fn rendered_shape_changed(&mut self) -> bool {
        let shape = self.list_shape();
        if shape == self.rendered_shape {
            return false;
        }
        self.rendered_shape = shape;
        true
    }

    /// Flat dispatch over every `Message` variant. Long by nature: each arm is
    /// a one-to-three-line delegation to a handler in `app/update/`.
    #[allow(clippy::too_many_lines)]
    fn handle_message(&mut self, message: Message) -> Task<Message> {
        if let Some(pane) = message.pane() {
            if !self.panes.contains_key(&pane) {
                return Task::none();
            }
        }
        match message {
            Message::Tick => self.handle_tick(),
            Message::WindowResized(size) => {
                self.window_size = size;
                self.capture_bounds_task()
            }
            #[allow(unused_variables)]
            Message::WindowOpened(id) => {
                // On Windows the media-control server needs the window HWND,
                // resolved here once the window actually exists.
                #[cfg(target_os = "windows")]
                {
                    return iced::window::raw_id::<Message>(id)
                        .then(move |raw| Task::done(Message::MediaHwnd(Some(raw))));
                }
                #[cfg(not(target_os = "windows"))]
                {
                    Task::none()
                }
            }
            Message::MediaHwnd(hwnd) => {
                self.init_media_controls(hwnd.map(|h| h as *mut std::ffi::c_void));
                Task::none()
            }
            Message::MediaEvent(event) => {
                self.process_media_event(&event);
                Task::none()
            }
            Message::WindowClose => {
                self.flush_session();
                Task::none()
            }
            Message::CursorMoved(pos) => self.handle_cursor_moved(pos),
            Message::LeftButtonReleased => self.handle_left_release(),
            Message::SearchInputClicked(pane) => self.handle_search_input_clicked(pane),
            Message::ListBoundsCaptured(bounds) => {
                for (pane, geo) in &bounds.tracks {
                    if let Some(p) = self.panes.get_mut(pane) {
                        p.view_data_mut().scroll = geo.translation_y;
                    }
                }
                self.bounds = *bounds;

                Task::none()
            }
            Message::SearchHistoryBoundsCaptured(pane, geo) => {
                self.bounds.search_history.insert(pane, geo);
                Task::none()
            }
            Message::Scrolled(target, translation) => {
                self.bounds.set_scroll_offset(target, translation);
                Task::none()
            }
            Message::LyricsScrolled {
                pane,
                translation_y,
                viewport_h,
                content_h,
            } => {
                if let Some(state) = &mut self.pane_mut(pane).lyrics {
                    state.viewport = Some(crate::app::LyricsViewport {
                        offset_y: translation_y,
                        height: viewport_h,
                        content_h,
                    });
                }
                Task::none()
            }
            Message::Shortcut(shortcut) => self.apply_shortcut(shortcut),
            Message::LyricsEditorAction(pane, action) => {
                if let Some(state) = &mut self.pane_mut(pane).lyrics {
                    if !matches!(action, iced::widget::text_editor::Action::Edit(_)) {
                        state.editor.perform(action);
                    }
                }
                Task::none()
            }
            Message::CopyLyrics(pane) => {
                let Some(state) = &self.pane(pane).lyrics else {
                    return Task::none();
                };
                let text = match state.displayed_lyrics() {
                    Some(lyrics) => {
                        if state.mode == crate::app::LyricsViewMode::Synced && lyrics.has_timed() {
                            lyrics.to_edit_text()
                        } else {
                            lyrics.plain.clone()
                        }
                    }
                    None => return Task::none(),
                };
                if text.is_empty() {
                    return Task::none();
                }
                self.notify(self.strings.lyrics_copied);
                iced::clipboard::write(text)
            }
            Message::SelectLyricLine(pane, idx) => self.select_lyric_line(pane, idx),
            Message::LyricNoteAction(pane, action) => {
                if let Some(state) = &mut self.pane_mut(pane).lyrics {
                    state.note_editor.perform(action);
                }
                Task::none()
            }
            Message::CustomLyricsEditorAction(pane, action) => {
                if let Some(state) = &mut self.pane_mut(pane).lyrics {
                    if state.editing {
                        state.edit_content.perform(action);
                    }
                }
                Task::none()
            }
            Message::StartCustomLyricsEdit(pane) => {
                self.start_custom_lyrics_edit(pane);
                Task::none()
            }
            Message::EditCustomLyrics(pane, name) => {
                self.edit_custom_lyrics(pane, name);
                Task::none()
            }
            Message::SelectCustomLyrics(pane, name) => self.select_custom_lyrics(pane, name),
            Message::CustomLyricsNameChanged(pane, name) => {
                if let Some(state) = &mut self.pane_mut(pane).lyrics {
                    if state.editing {
                        state.edit_name = name;
                    }
                }
                Task::none()
            }
            Message::SaveCustomLyrics(pane) => self.save_custom_lyrics(pane),
            Message::CancelCustomLyricsEdit(pane) => {
                self.cancel_custom_lyrics_edit(pane);
                Task::none()
            }
            Message::DeleteCustomLyrics(pane) => {
                self.delete_custom_lyrics(pane);
                Task::none()
            }
            Message::OpenTranslateDialog(pane) => self.open_translate_dialog(pane),
            Message::TranslateLanguageChanged(value) => {
                self.handle_translate_language_changed(value);
                Task::none()
            }
            Message::TranslateLanguageSubmit => {
                self.handle_translate_language_submit();
                Task::none()
            }
            Message::TranslatePromptAction(action) => {
                self.handle_translate_prompt_action(action);
                Task::none()
            }
            Message::TranslateResetPrompt => {
                self.handle_translate_reset_prompt();
                Task::none()
            }
            Message::TranslateSubmit => self.submit_translation(),
            Message::RequestTranslationModels => {
                self.request_translation_models();
                Task::none()
            }
            Message::TranslationModelsFilterChanged(value) => {
                self.translation_models_filter = value;
                Task::none()
            }
            Message::SearchInputChanged(pane, query) => {
                self.pane_mut(pane).search_query = query;
                self.update_search_history(pane);
                self.drag.clear_hovered_search_history();
                CaptureSearchHistoryRows::new(pane).into()
            }
            Message::SearchExecute(pane) => self.handle_search_execute(pane),
            Message::SearchScopeChanged(pane, scope) => {
                self.handle_search_scope_changed(pane, scope)
            }
            Message::SearchProviderChanged(pane, provider) => {
                self.handle_search_provider_changed(pane, provider)
            }
            Message::Browse(pane, kind, provider) => self.handle_browse(pane, &kind, provider),
            Message::OpenArtist {
                pane,
                id,
                name,
                source,
                url,
            } => self.open_artist(pane, Some(&id), &name, source, &url),
            Message::ArtistSectionProviderChanged(pane, section, provider) => {
                self.handle_artist_section_provider_changed(pane, section, provider);
                Task::none()
            }
            Message::ArtistHeaderProviderChanged(pane, provider) => {
                self.handle_artist_header_provider_changed(pane, provider);
                Task::none()
            }
            Message::ToggleLibrarySave(item) => self.toggle_library_save_with_toast(item),
            Message::ToggleLibraryExpanded => {
                self.library_expanded = !self.library_expanded;
                self.save_session();
                Task::none()
            }
            Message::SearchLoadMore(pane) => {
                self.handle_search_load_more(pane);
                Task::none()
            }
            Message::SearchHistorySelected(pane, index) => {
                self.handle_search_history_select(pane, index)
            }
            Message::DeleteSearchHistory(pane, index) => {
                self.handle_delete_search_history(pane, index);
                Task::none()
            }
            Message::DragPress(pressed) => {
                self.handle_drag_press(pressed);
                Task::none()
            }
            Message::HoverStart(target) => {
                self.drag.hover_started(target);
                Task::none()
            }
            Message::HoverEnd(target) => {
                self.drag.hover_ended(&target);
                Task::none()
            }
            Message::TrackRightClicked(pos, point) => {
                self.show_context_menu_at(pos, self.window_point(pos.pane, point))
            }
            Message::CardRightClicked {
                host,
                pane,
                target,
                row,
                point,
            } => {
                let point = self.card_window_point(host, row, point);
                self.show_card_menu_at(pane, target, point)
            }
            Message::PlayTrackAt(pos) => {
                self.handle_play_track(pos);
                Task::none()
            }
            Message::TogglePlayPause => {
                self.toggle_play_pause();
                Task::none()
            }
            Message::NextTrack => {
                self.next_track();
                Task::none()
            }
            Message::PreviousTrack => {
                self.previous_track();
                Task::none()
            }
            Message::SetVolume(vol) => {
                self.set_volume(vol);
                Task::none()
            }
            Message::Seek(frac) => {
                self.seek(frac);
                Task::none()
            }
            Message::CreatePlaylist => {
                self.handle_create_playlist();
                Task::none()
            }
            Message::ToggleCurrentTrackLike => {
                self.toggle_like_current_track();
                Task::none()
            }
            Message::ToggleTrackLike(pos) => {
                self.toggle_track_like_at(pos);
                Task::none()
            }
            Message::SaveBrowseAsPlaylist(pane) => self.save_browse_as_playlist(pane),
            Message::OpenPlaylistJump => self.open_playlist_jump(),
            Message::PlaylistJumpInput(query) => {
                if let Some(Dialog::PlaylistJump(jump)) = &mut self.dialog {
                    jump.query = query;
                    jump.selected = 0;
                }
                Task::none()
            }
            Message::PlaylistJumpOpen(index) => self.submit_playlist_jump(index, false),
            Message::PlaylistJumpHover(pos) => {
                self.hover_playlist_jump(pos);
                Task::none()
            }
            Message::PlaylistJumpOpenPlay(index) => self.submit_playlist_jump(index, true),
            Message::NewPlaylistNameChanged(name) => {
                self.playlist_create_name = name;
                Task::none()
            }
            Message::RenamePlaylist(name) => {
                self.handle_rename_playlist(&name);
                Task::none()
            }
            Message::AddLocalMusic => {
                let tx = self.result_tx.clone();
                std::thread::spawn(move || {
                    let files = rfd::FileDialog::new()
                        .add_filter(
                            "Audio",
                            &["mp3", "flac", "wav", "ogg", "m4a", "aac", "opus", "wma"],
                        )
                        .pick_files()
                        .filter(|f| !f.is_empty())
                        .unwrap_or_default();
                    let _ = tx.send(BackendResult::LocalFilesPicked(files));
                });
                Task::none()
            }
            Message::OpenPlaylistAdd(indices) => {
                let pane = match &self.dialog {
                    Some(Dialog::ContextMenu(m)) => m.pos.pane,
                    _ => self.focused_pane_id,
                };
                self.open_playlist_add(PendingAdd { indices, pane })
            }
            Message::CloseDialog => {
                self.save_translate_prefs();
                self.dialog = None;
                Task::none()
            }
            Message::ShowDeleteConfirm(index) => {
                self.dialog = Some(Dialog::DeleteConfirm(index));
                Task::none()
            }
            Message::ConfirmDeletePlaylist => {
                if let Some(Dialog::DeleteConfirm(idx)) = &self.dialog {
                    self.handle_delete_playlist(*idx)
                } else {
                    Task::none()
                }
            }
            Message::OpenImportPlaylist => {
                self.dialog = Some(Dialog::Import(ImportPlaylistDialog::default()));
                Task::none()
            }
            Message::ImportMethodChanged(method) => {
                if let Some(Dialog::Import(dialog)) = &mut self.dialog {
                    dialog.method = method;
                }
                Task::none()
            }
            Message::ImportCsvColChanged(field, value) => {
                if let Some(Dialog::Import(dialog)) = &mut self.dialog {
                    match field {
                        ImportCsvField::Name => dialog.csv_name_col = value,
                        ImportCsvField::Artist => dialog.csv_artist_col = value,
                        ImportCsvField::Album => dialog.csv_album_col = value,
                    }
                }
                Task::none()
            }
            Message::ImportCsvPresetChanged(preset) => {
                if let Some(Dialog::Import(dialog)) = &mut self.dialog {
                    dialog.apply_csv_preset(preset);
                }
                Task::none()
            }
            Message::ImportPlaylistNameChanged(value) => {
                if let Some(Dialog::Import(dialog)) = &mut self.dialog {
                    dialog.playlist_name = value;
                }
                Task::none()
            }
            Message::ImportPatternChanged(index, value) => {
                if let Some(Dialog::Import(dialog)) = &mut self.dialog {
                    if let Some(slot) = dialog.patterns.get_mut(index) {
                        *slot = value;
                    }
                }
                Task::none()
            }
            Message::ImportAddPattern => {
                if let Some(Dialog::Import(dialog)) = &mut self.dialog {
                    dialog.patterns.push(String::new());
                }
                Task::none()
            }
            Message::ImportRemovePattern(index) => {
                if let Some(Dialog::Import(dialog)) = &mut self.dialog {
                    if index < dialog.patterns.len() {
                        dialog.patterns.remove(index);
                    }
                }
                Task::none()
            }
            Message::ImportSelectFiles => {
                self.handle_import_pick();
                Task::none()
            }
            Message::OpenAndPlayPlaylist(index) => self.handle_open_and_play_playlist(index),
            Message::TrackListSearchInput(query) => self.handle_track_list_search_input(&query),
            Message::TrackListSearchNext => self.handle_track_list_search_step(1),
            Message::TrackListSearchPrev => self.handle_track_list_search_step(-1),
            Message::TrackListSearchClose => {
                self.track_list_search = None;
                Task::none()
            }
            Message::ToggleQueue => {
                self.show_queue = !self.show_queue;
                self.save_session();
                if self.show_queue {
                    self.capture_bounds_task()
                } else {
                    Task::none()
                }
            }
            Message::ToggleRepeat => {
                self.repeat = !self.repeat;
                self.save_session();
                Task::none()
            }
            Message::ShowLyrics(pane) => self.handle_show_lyrics(pane),
            Message::RevealNowPlaying => self.handle_reveal_now_playing(),
            Message::SetLyricsViewMode(pane, mode) => self.set_lyrics_view_mode(pane, mode),
            Message::LyricsLineClicked(secs) => {
                self.seek_to_seconds(secs);
                Task::none()
            }
            Message::SelectLyricsProvider(pane, id) => {
                self.handle_select_lyrics_provider(pane, id);
                Task::none()
            }
            Message::SelectLyricsTranslation(pane, language) => {
                self.handle_select_lyrics_translation(pane, language)
            }
            Message::SwitchQueueTab(tab) => self.switch_queue_tab(tab),
            Message::NavigateTo(pane, data) => {
                self.pane_mut(pane).lyrics = None;
                self.handle_navigate_to(pane, data)
            }
            Message::SidebarSearch => {
                let pane = self.focused_pane_id;
                self.pane_mut(pane).lyrics = None;
                self.handle_sidebar_search()
            }
            Message::NavigateBack(pane) => {
                if self.pane(pane).lyrics.is_some() {
                    self.pane_mut(pane).lyrics = None;
                    Task::none()
                } else {
                    self.handle_navigate_back(pane)
                }
            }
            Message::NavigateForward(pane) => self.handle_navigate_forward(pane),
            Message::SplitHorizontal(pane) => {
                self.split_pane(pane, crate::app::SplitDir::Horizontal)
            }
            Message::SplitVertical(pane) => self.split_pane(pane, crate::app::SplitDir::Vertical),
            Message::ClosePane(pane) => self.close_pane(pane),
            Message::FocusPane(pane) => {
                self.focus_pane(pane);
                Task::none()
            }
            Message::SettingsChanged(change) => {
                self.handle_settings_change(change);
                Task::none()
            }
            Message::SettingsResetDefaults => {
                self.handle_settings_reset_defaults();
                Task::none()
            }
            Message::ContextMenuPlayTrack(pos) => {
                self.close_context_menu();
                self.handle_play_track(pos);
                Task::none()
            }
            Message::ContextMenuGoToArtist => {
                let Some(Dialog::ContextMenu(menu)) = &self.dialog else {
                    return Task::none();
                };
                let provider = menu.default_go_to_artist_provider(self.config.default_provider);
                self.handle_context_menu_go_to_artist(provider)
            }
            Message::ContextMenuGoToArtistProvider(provider) => {
                self.handle_context_menu_go_to_artist(provider)
            }
            Message::ContextMenuPlayViaProvider(provider, pos) => {
                self.close_context_menu();
                self.play_track_via_provider(provider, pos);
                Task::none()
            }
            Message::ContextMenuDownloadViaProvider(provider) => {
                self.download_track_via_provider(provider);
                Task::none()
            }
            Message::ContextMenuSongRadioProvider(provider) => {
                self.handle_context_menu_song_radio(provider)
            }
            Message::ContextMenuArtistRadioProvider(provider) => {
                self.handle_context_menu_artist_radio(provider)
            }
            Message::ContextMenuClearCache => {
                self.handle_context_menu_clear_cache_current();
                Task::none()
            }
            Message::ContextMenuClearCacheProvider(provider) => {
                self.handle_context_menu_clear_cache(provider);
                Task::none()
            }
            Message::ContextMenuHover(focus) => {
                if let Some(anchor) = self.menu_anchor_mut() {
                    anchor.hovered = focus;
                }
                Task::none()
            }
            Message::ContextMenuBoundsCaptured { panel, row_offsets } => {
                let prev = self.bounds.context_menu.take();
                let width_changed = prev
                    .as_ref()
                    .is_none_or(|p| (p.panel.width - panel.width).abs() > f32::EPSILON);
                let window = self.window_size;
                let moved = self
                    .menu_anchor_mut()
                    .is_some_and(|anchor| anchor.flip_position(panel, window));
                let stable = !moved && !width_changed;
                self.bounds.context_menu = Some(ContextMenuGeometry {
                    panel,
                    row_offsets,
                    stable,
                });
                // Re-measure after a flip or a clipped-width correction; the
                // captures converge once position and width stop changing.
                if stable {
                    Task::none()
                } else {
                    CaptureContextMenu::default().into()
                }
            }
            Message::ContextMenuDefault(action) => {
                let Some(Dialog::ContextMenu(menu)) = &self.dialog else {
                    return Task::none();
                };
                let provider = menu.default_provider(action, self.config.default_provider);
                match action {
                    DefaultCtxAction::Download => {
                        self.download_track_via_provider(provider);
                        Task::none()
                    }
                    DefaultCtxAction::SongRadio => self.handle_context_menu_song_radio(provider),
                    DefaultCtxAction::ArtistRadio => {
                        self.handle_context_menu_artist_radio(provider)
                    }
                }
            }
            Message::ContextMenuAddToQueue(source, indices) => {
                self.close_context_menu();
                self.handle_add_to_queue(source, &indices);
                Task::none()
            }
            Message::ContextMenuAddToTrashbin(source, indices) => {
                self.close_context_menu();
                self.handle_add_to_trashbin(source, &indices);
                Task::none()
            }
            Message::ContextMenuRemoveFromTrashbin(source, indices) => {
                self.close_context_menu();
                self.handle_remove_from_trashbin(source, &indices);
                Task::none()
            }
            Message::ContextMenuRemoveFromList(source, indices) => {
                self.close_context_menu();
                match source {
                    QUEUE_PANE_ID => self.handle_remove_from_queue_batch(&indices),
                    RECENT_PANE_ID => self.handle_remove_from_recent_batch(&indices),
                    _ => self.handle_remove_from_playlist_batch(source, &indices),
                }
                Task::none()
            }
            Message::ContextMenuEditTrack => {
                let Some(Dialog::ContextMenu(menu)) = &self.dialog else {
                    return Task::none();
                };
                let pos = menu.pos;
                self.close_context_menu();
                self.open_edit_track(pos);
                Task::none()
            }
            Message::ContextMenuOpenInBrowser => {
                self.handle_context_menu_open_in_browser();
                Task::none()
            }
            Message::CardMenuOpen => {
                let Some(menu) = self.take_card_menu() else {
                    return Task::none();
                };
                let (pane, target) = (menu.pane, menu.target);
                self.open_card(pane, &target)
            }
            Message::CardMenuOpenInBrowser => {
                self.handle_card_menu_open_in_browser();
                Task::none()
            }
            Message::CardMenuSaveAsPlaylist => self.handle_card_menu_save_as_playlist(),
            Message::CardMenuToggleLibrary => self.handle_card_menu_toggle_library(),
            Message::OpenInBrowser(url) => {
                self.open_in_browser(&url);
                Task::none()
            }
            Message::EditTrackField(field, value) => {
                if let Some(Dialog::Edit(edit)) = &mut self.dialog {
                    match field {
                        EditTrackField::Title => edit.title = value,
                        EditTrackField::Artist => edit.artist = value,
                    }
                }
                Task::none()
            }
            Message::EditTrackSelectProvider(provider) => {
                if let Some(Dialog::Edit(edit)) = &mut self.dialog {
                    edit.source = provider;
                }
                Task::none()
            }
            Message::EditTrackFindProvider(provider) => {
                self.handle_edit_track_find_provider(provider);
                Task::none()
            }
            Message::EditTrackManualSearch(provider) => self.open_edit_manual_search(provider),
            Message::EditTrackManualQuery(value) => {
                self.handle_edit_manual_query(value);
                Task::none()
            }
            Message::EditTrackManualScope(scope) => {
                // A scope change re-runs the query, like the search bar's chips.
                if self.handle_edit_manual_scope(scope) {
                    self.handle_edit_manual_run();
                }
                Task::none()
            }
            Message::EditTrackManualRun => {
                self.handle_edit_manual_run();
                Task::none()
            }
            Message::EditTrackManualConfirm => {
                self.apply_edit_manual_selection();
                Task::none()
            }
            Message::EditTrackManualCancel => self.handle_edit_manual_cancel(),
            Message::SaveEditTrack => {
                self.apply_edit_track();
                Task::none()
            }
            Message::CloseContextMenu => {
                self.close_context_menu();
                Task::none()
            }
            Message::DepToggle(kind) => {
                if let Some(Dialog::Dependencies(dialog)) = &mut self.dialog {
                    if kind.auto_installable() {
                        if dialog.selected.contains(&kind) {
                            dialog.selected.remove(&kind);
                        } else {
                            dialog.selected.insert(kind);
                        }
                    }
                }
                Task::none()
            }
            Message::DepInstall => self.handle_install_dependencies(),
            Message::DepDismiss => {
                self.dialog = None;
                // If the active source is no longer searchable (its tools were
                // not installed), fall back to one that is.
                for pane in self.pane_ids() {
                    if !self.pane(pane).search_provider.capabilities().search {
                        self.pane_mut(pane).search_provider = ProviderId::searchable()
                            .iter()
                            .copied()
                            .find(|p| p.capabilities().search)
                            .unwrap_or(ProviderId::SoundCloud);
                    }
                }
                Task::none()
            }
            Message::DepSettingsInstall(kind) => {
                self.handle_dep_settings_install(kind);
                Task::none()
            }
            Message::DepSettingsDelete(kind) => {
                self.handle_dep_settings_delete(kind);
                Task::none()
            }
            Message::CheckForUpdates => {
                self.check_for_updates();
                Task::none()
            }
            Message::UpdateApp => {
                self.start_update();
                Task::none()
            }
        }
    }

    /// Spawn a background install thread that reports download progress and the
    /// final result back to the main thread through `tx` (as [`BackendResult`]).
    /// Single helper for both the startup dialog and the Settings view so the
    /// install/report plumbing lives in one place.
    fn spawn_dep_install(tx: std::sync::mpsc::Sender<BackendResult>, kind: DepKind) {
        std::thread::spawn(move || {
            let tx_progress = tx.clone();
            let result = crate::deps::install(kind, move |downloaded, total| {
                let _ =
                    tx_progress.send(BackendResult::DependencyProgress(kind, downloaded, total));
            });
            let _ = tx.send(BackendResult::DependencyInstalled(
                kind,
                result.map_err(|e| e.to_string()),
            ));
        });
    }

    /// Spawn a background install thread for each selected, not-yet-attempted
    /// dependency. Results arrive via [`BackendResult::DependencyInstalled`],
    /// drained by the tick and applied in [`Self::process_result`].
    fn handle_install_dependencies(&mut self) -> Task<Message> {
        let Some(Dialog::Dependencies(dialog)) = &self.dialog else {
            return Task::none();
        };
        let pending = dialog.pending(&self.dep_ops);
        let tx = self.result_tx.clone();
        for kind in pending {
            self.dep_ops.entry(kind).or_default().installing = true;
            Self::spawn_dep_install(tx.clone(), kind);
        }
        Task::none()
    }

    /// Install a single dependency from the Settings view into the app cache.
    /// Progress/result land in [`BackendResult::DependencyInstalled`] and update
    /// [`MusicPlayer::dep_ops`].
    fn handle_dep_settings_install(&mut self, kind: DepKind) {
        if !kind.auto_installable() {
            return;
        }
        let op = self.dep_ops.entry(kind).or_default();
        if op.installing || op.deleting {
            return;
        }
        op.installing = true;
        Self::spawn_dep_install(self.result_tx.clone(), kind);
    }

    /// Remove the app-managed copy of a dependency from the Settings view.
    /// Result lands in [`BackendResult::DependencyDeleted`] and updates
    /// [`MusicPlayer::dep_ops`].
    fn handle_dep_settings_delete(&mut self, kind: DepKind) {
        if !crate::deps::installed_via_app(kind) {
            return;
        }
        let op = self.dep_ops.entry(kind).or_default();
        if op.installing || op.deleting {
            return;
        }
        op.deleting = true;
        let tx = self.result_tx.clone();
        std::thread::spawn(move || {
            let result = crate::deps::uninstall(kind);
            let _ = tx.send(BackendResult::DependencyDeleted(
                kind,
                result.map_err(|e| e.to_string()),
            ));
        });
    }

    pub fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            self.tick_subscription(),
            Self::slow_event_subscription(),
            self.drag_subscription(),
            self.key_subscription(),
            Self::media_hwnd_subscription(),
        ])
    }

    /// The progress bar animates from the wall clock at draw time, so 1Hz
    /// suffices for everything the tick does: time texts, drift correction,
    /// and end-of-track detection (auto-advance can lag up to a second).
    #[allow(clippy::unused_self)]
    fn tick_subscription(&self) -> Subscription<Message> {
        iced::time::every(Duration::from_secs(1)).map(|_| Message::Tick)
    }

    /// Discrete events only: mouse release and window. Keyboard lives in
    /// [`Self::key_subscription`], gated by open dialog.
    fn slow_event_subscription() -> Subscription<Message> {
        iced::event::listen_with(Self::slow_event_to_message)
    }

    /// Key chords decoded to [`Shortcut`](crate::app::Shortcut) at the event
    /// level. The context menu and the playlist jumper swallow everything but
    /// their own keys, exactly like the old handler's early returns did — so
    /// they get their own matchers and the main table stays off.
    fn key_subscription(&self) -> Subscription<Message> {
        use crate::app::shortcut;
        match &self.dialog {
            Some(Dialog::ContextMenu(_)) => iced::event::listen_with(shortcut::match_menu),
            Some(Dialog::PlaylistJump(_)) => iced::event::listen_with(shortcut::match_jump),
            _ => iced::event::listen_with(shortcut::match_main),
        }
    }

    fn drag_subscription(&self) -> Subscription<Message> {
        if self.drag.pressed.is_some() {
            iced::event::listen_with(Self::drag_event_to_message)
        } else {
            Subscription::none()
        }
    }

    /// Emits `Message::WindowOpened` once the application window is created, so
    /// the Windows HWND can be resolved for SMTC. A no-op subscription on the
    /// other platforms (media controls start without a window handle there).
    fn media_hwnd_subscription() -> Subscription<Message> {
        #[cfg(target_os = "windows")]
        {
            iced::window::open_events().map(Message::WindowOpened)
        }
        #[cfg(not(target_os = "windows"))]
        {
            Subscription::none()
        }
    }

    // `iced::event::listen_with` hands the event over by value, so the
    // by-value parameter is mandated by the API.
    #[allow(clippy::needless_pass_by_value)]
    fn slow_event_to_message(
        event: iced::Event,
        _status: iced::event::Status,
        _window: iced::window::Id,
    ) -> Option<Message> {
        match event {
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                Some(Message::LeftButtonReleased)
            }
            iced::Event::Window(iced::window::Event::CloseRequested) => Some(Message::WindowClose),
            iced::Event::Window(iced::window::Event::Resized(size)) => {
                Some(Message::WindowResized(size))
            }
            _ => None,
        }
    }

    // Cursor tracking runs only while a press is down (see
    // `drag_subscription`). Every message out of here means drag work, so the
    // handler applies it without re-checking whether tracking is wanted.
    #[allow(clippy::needless_pass_by_value)]
    fn drag_event_to_message(
        event: iced::Event,
        _status: iced::event::Status,
        _window: iced::window::Id,
    ) -> Option<Message> {
        match event {
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                Some(Message::CursorMoved(position))
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{app::ViewData, data::config, types::Track};

    fn track(id: &str) -> Track {
        Track::from_provider(
            ProviderId::YouTube,
            id.into(),
            format!("https://example.com/{id}"),
            format!("Track {id}"),
            "Artist",
            10,
            String::new(),
            None,
            None,
        )
    }

    fn player() -> crate::app::MusicPlayer {
        let mut p = crate::app::MusicPlayer::new_with(config::Config::default());
        p.reset_test_pane(vec![ViewData::new_downloads(Vec::new())]);
        p.rendered_shape = p.list_shape();
        p
    }

    #[test]
    fn a_list_gaining_or_losing_rows_asks_for_one_capture() {
        let mut p = player();
        assert!(p.list_shape().is_empty());

        p.view_data_mut()
            .set_tracks(vec![track("1"), track("2"), track("3")]);
        // Geometry captured while the list was empty (or shorter) no longer
        // describes what renders, so the update loop recaptures exactly once.
        assert!(p.rendered_shape_changed());
        assert!(!p.rendered_shape_changed());

        p.view_data_mut().set_tracks(vec![track("1")]);
        assert!(p.rendered_shape_changed());
        assert!(!p.rendered_shape_changed());
    }

    #[test]
    fn the_queue_counts_up_next_and_a_lyrics_overlay_hides_its_pane() {
        let mut p = player();
        p.show_queue = true;
        assert!(p.list_shape().is_empty());

        // A lone track is the panel's now-playing row, rendered outside the
        // scrollable, so it does not need list geometry.
        p.queue.tracks = vec![track("1")];
        assert!(p.list_shape().is_empty());

        p.queue.tracks.push(track("2"));
        assert_eq!(p.list_shape(), vec![(QUEUE_PANE_ID, 1)]);

        p.view_data_mut().set_tracks(vec![track("1")]);
        let pane = p.focused_pane_id;
        p.pane_mut(pane).lyrics = Some(crate::app::LyricsState::new(
            crate::lyrics::LyricsProvider::LrcLib,
        ));
        assert_eq!(p.list_shape(), vec![(QUEUE_PANE_ID, 1)]);
    }
}
