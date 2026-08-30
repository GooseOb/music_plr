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
        import::{ImportCsvField, ImportPlaylistDialog},
        interaction::{DefaultCtxAction, TrackListKind},
        message::{BackendResult, EditTrackField, Message},
        update::{
            operation::{
                CaptureBounds, CaptureContextMenu, CaptureSearchHistoryRows, ContextMenuGeometry,
            },
            settings::SettingsChange,
        },
    },
    deps::DepKind,
    load_state::LoadState,
    providers::ProviderId,
};

impl crate::app::MusicPlayer {
    /// Flat dispatch over every `Message` variant. Long by nature: each arm is
    /// a one-to-three-line delegation to a handler in `app/update/`.
    #[allow(clippy::too_many_lines)]
    pub fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Tick => self.handle_tick(),
            Message::WindowResized(size) => {
                self.window_size = size;
                CaptureBounds::new().into()
            }
            Message::WindowClose => {
                self.flush_session();
                Task::none()
            }
            Message::CursorMoved(pos) => self.handle_cursor_moved(pos),
            Message::LeftButtonReleased => self.handle_left_release(),
            Message::ListBoundsCaptured(bounds) => {
                let scroll = bounds.track.as_ref().map_or(0.0, |b| b.translation_y);
                self.bounds = *bounds;
                self.view_data_mut().scroll = scroll;

                Task::none()
            }
            Message::SearchHistoryBoundsCaptured(geo) => {
                self.bounds.search_history = Some(geo);
                Task::none()
            }
            Message::ListScrolled {
                list,
                translation_y,
            } => {
                let geo = match list {
                    TrackListKind::Queue => &mut self.bounds.queue,
                    TrackListKind::Active => &mut self.bounds.track,
                    TrackListKind::Recent => &mut self.bounds.recent,
                };
                if let Some(g) = geo {
                    g.translation_y = translation_y;
                }
                Task::none()
            }
            Message::KeyPressed { key, modifiers } => self.handle_key_press(key, modifiers),
            Message::LyricsEditorAction(action) => {
                if let Some(state) = &mut self.lyrics {
                    if !matches!(action, iced::widget::text_editor::Action::Edit(_)) {
                        state.editor.perform(action);
                    }
                }
                Task::none()
            }
            Message::CopyLyrics => {
                let Some(state) = &self.lyrics else {
                    return Task::none();
                };
                let text = match &state.lyrics {
                    LoadState::Ready(lyrics) => lyrics.plain.clone(),
                    _ => return Task::none(),
                };
                if text.is_empty() {
                    return Task::none();
                }
                self.notify(self.strings.lyrics_copied);
                iced::clipboard::write(text)
            }
            Message::SearchInputChanged(query) => {
                self.search_query = query;
                self.update_search_history();
                self.drag.clear_hovered_search_history();
                CaptureSearchHistoryRows::new().into()
            }
            Message::SearchExecute => self.handle_search_execute(),
            Message::SearchScopeChanged(scope) => self.handle_search_scope_changed(scope),
            Message::SearchProviderChanged(provider) => {
                self.handle_search_provider_changed(provider)
            }
            Message::Browse(kind, provider) => self.handle_browse(&kind, provider),
            Message::OpenArtist { id, name, source } => self.open_artist(Some(&id), &name, source),
            Message::ArtistSectionProviderChanged(section, provider) => {
                self.handle_artist_section_provider_changed(section, provider);
                Task::none()
            }
            Message::ArtistHeaderProviderChanged(provider) => {
                self.handle_artist_header_provider_changed(provider);
                Task::none()
            }
            Message::ToggleLibrarySave(item) => {
                let saved = self.toggle_library_save(item);
                self.notify(if saved {
                    self.strings.saved_to_library
                } else {
                    self.strings.removed_from_library
                });
                Task::none()
            }
            Message::ToggleLibraryExpanded => {
                self.library_expanded = !self.library_expanded;
                self.save_session();
                Task::none()
            }
            Message::SearchLoadMore => {
                self.handle_search_load_more();
                Task::none()
            }
            Message::SearchHistorySelected(index) => self.handle_search_history_select(index),
            Message::DeleteSearchHistory(index) => {
                self.handle_delete_search_history(index);
                Task::none()
            }
            Message::DragPress(pressed) => {
                self.handle_drag_press(pressed);
                Task::none()
            }
            Message::HoverStart(target) => {
                if !self.drag.is_hover_controlled && self.drag.hovered.as_ref() != Some(&target) {
                    self.drag.set_hovered(target);
                }
                Task::none()
            }
            Message::HoverEnd(target) => {
                if !self.drag.is_hover_controlled && self.drag.hovered.as_ref() == Some(&target) {
                    self.drag.hovered = None;
                }
                Task::none()
            }
            Message::TrackRightClicked(pos) => self.show_context_menu_at(pos, self.drag.cursor_pos),
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
                        .pick_files();
                    if let Some(files) = files.filter(|f| !f.is_empty()) {
                        let _ = tx.send(BackendResult::LocalFilesPicked(files));
                    }
                });
                Task::none()
            }
            Message::AddToPlaylist(playlist_idx) => {
                if let Some(picker) = self.playlist_picker.take() {
                    self.handle_add_to_playlist(playlist_idx, &picker.indices, picker.list);
                }
                Task::none()
            }
            Message::TogglePicker(indices) => {
                let list = self
                    .context_menu
                    .as_ref()
                    .map_or(TrackListKind::Active, |m| m.pos.list);
                self.handle_toggle_picker(indices, list);
                Task::none()
            }
            Message::ClosePicker => {
                self.playlist_picker = None;
                Task::none()
            }
            Message::ShowDeleteConfirm(index) => {
                self.delete_confirm_index = Some(index);
                Task::none()
            }
            Message::ConfirmDeletePlaylist => {
                let mut nav_task = Task::none();
                if let Some(idx) = self.delete_confirm_index {
                    nav_task = self.handle_delete_playlist(idx);
                }
                self.delete_confirm_index = None;
                nav_task
            }
            Message::HideDeleteConfirm => {
                self.delete_confirm_index = None;
                Task::none()
            }
            Message::OpenImportPlaylist => {
                self.import_dialog = Some(ImportPlaylistDialog::default());
                Task::none()
            }
            Message::CloseImportPlaylist => {
                self.import_dialog = None;
                Task::none()
            }
            Message::ImportMethodChanged(method) => {
                if let Some(dialog) = &mut self.import_dialog {
                    dialog.method = method;
                }
                Task::none()
            }
            Message::ImportCsvColChanged(field, value) => {
                if let Some(dialog) = &mut self.import_dialog {
                    match field {
                        ImportCsvField::Name => dialog.csv_name_col = value,
                        ImportCsvField::Artist => dialog.csv_artist_col = value,
                        ImportCsvField::Album => dialog.csv_album_col = value,
                    }
                }
                Task::none()
            }
            Message::ImportCsvPresetChanged(preset) => {
                if let Some(dialog) = &mut self.import_dialog {
                    dialog.apply_csv_preset(preset);
                }
                Task::none()
            }
            Message::ImportPlaylistNameChanged(value) => {
                if let Some(dialog) = &mut self.import_dialog {
                    dialog.playlist_name = value;
                }
                Task::none()
            }
            Message::ImportPatternChanged(index, value) => {
                if let Some(dialog) = &mut self.import_dialog {
                    if let Some(slot) = dialog.patterns.get_mut(index) {
                        *slot = value;
                    }
                }
                Task::none()
            }
            Message::ImportAddPattern => {
                if let Some(dialog) = &mut self.import_dialog {
                    dialog.patterns.push(String::new());
                }
                Task::none()
            }
            Message::ImportRemovePattern(index) => {
                if let Some(dialog) = &mut self.import_dialog {
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
                    CaptureBounds::new().into()
                } else {
                    Task::none()
                }
            }
            Message::ToggleRepeat => {
                self.repeat = !self.repeat;
                self.save_session();
                Task::none()
            }
            Message::ShowLyrics => {
                self.handle_show_lyrics();
                Task::none()
            }
            Message::RevealNowPlaying => self.handle_reveal_now_playing(),
            Message::SetLyricsViewMode(mode) => {
                self.set_lyrics_view_mode(mode);
                Task::none()
            }
            Message::LyricsLineClicked(secs) => {
                self.seek_to_seconds(secs);
                Task::none()
            }
            Message::SelectLyricsProvider(id) => {
                self.handle_select_lyrics_provider(id);
                Task::none()
            }
            Message::SwitchQueueTab(tab) => {
                self.queue.queue_tab = tab;
                self.drag.clear_hovered_track();
                self.save_session();
                CaptureBounds::new().into()
            }
            Message::NavigateTo(data) => {
                self.lyrics = None;
                self.handle_navigate_to(data)
            }
            Message::NavigateBack => {
                if self.lyrics.is_some() {
                    self.lyrics = None;
                    Task::none()
                } else {
                    self.handle_navigate_back()
                }
            }
            Message::NavigateForward => self.handle_navigate_forward(),
            Message::SettingsDownloadDirChanged(dir) => {
                self.handle_settings_change(SettingsChange::DownloadDir(dir));
                Task::none()
            }
            Message::SettingsMaxHistoryVisibleChanged(v) => {
                self.handle_settings_change(SettingsChange::MaxHistoryVisible(v));
                Task::none()
            }
            Message::SettingsMaxHistoryStoredChanged(v) => {
                self.handle_settings_change(SettingsChange::MaxHistoryStored(v));
                Task::none()
            }
            Message::SettingsCacheMaxSizeChanged(v) => {
                self.handle_settings_change(SettingsChange::CacheMaxSize(v));
                Task::none()
            }
            Message::SettingsMaxRecentlyPlayedChanged(v) => {
                self.handle_settings_change(SettingsChange::MaxRecentlyPlayed(v));
                Task::none()
            }
            Message::SettingsVolumeNormalizationToggled(enabled) => {
                self.handle_settings_change(SettingsChange::VolumeNormalization(enabled));
                Task::none()
            }
            Message::SettingsLanguageChanged(language) => {
                self.handle_settings_change(SettingsChange::Language(language));
                Task::none()
            }
            Message::SettingsDefaultProviderChanged(provider) => {
                self.handle_settings_change(SettingsChange::DefaultProvider(provider));
                Task::none()
            }
            Message::SettingsThemeChanged(kind) => {
                self.handle_settings_change(SettingsChange::Theme(kind));
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
                let provider = match self.context_menu.as_ref() {
                    Some(menu) => menu.default_go_to_artist_provider(self.config.default_provider),
                    None => return Task::none(),
                };
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
            Message::ContextMenuHover(focus) => {
                if let Some(menu) = &mut self.context_menu {
                    menu.hovered = focus;
                }
                Task::none()
            }
            Message::ContextMenuBoundsCaptured { panel, row_offsets } => {
                let prev = self.bounds.context_menu.take();
                let width_changed = prev
                    .as_ref()
                    .is_none_or(|p| (p.panel.width - panel.width).abs() > f32::EPSILON);
                // Recompute the flip from the original cursor point using
                // the latest measurement. A panel flush with the window edge
                // means its measurement was clipped by the remaining space,
                // so that counts as overflow too. Flipped menus keep their
                // bottom/right edge at the cursor.
                let edge_epsilon = 1.0;
                let moved = if let Some(menu) = &mut self.context_menu {
                    let (cx, cy) = menu.cursor;
                    let nx = if cx + panel.width > self.window_size.width - edge_epsilon {
                        (cx - panel.width).max(0.0)
                    } else {
                        cx
                    };
                    let ny = if cy + panel.height > self.window_size.height - edge_epsilon {
                        (cy - panel.height).max(0.0)
                    } else {
                        cy
                    };
                    let moved = (nx, ny) != menu.position;
                    menu.position = (nx, ny);
                    moved
                } else {
                    false
                };
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
                let Some(menu) = self.context_menu.as_ref() else {
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
            Message::ContextMenuRemoveFromPlaylist(indices) => {
                self.close_context_menu();
                self.handle_remove_from_playlist_batch(&indices);
                Task::none()
            }
            Message::ContextMenuRemoveFromQueue(indices) => {
                self.close_context_menu();
                self.handle_remove_from_queue_batch(&indices);
                Task::none()
            }
            Message::ContextMenuEditTrack => {
                let pos = match self.context_menu.as_ref() {
                    Some(menu) => menu.pos,
                    None => return Task::none(),
                };
                self.close_context_menu();
                self.open_edit_track(pos);
                Task::none()
            }
            Message::EditTrackField(field, value) => {
                if let Some(edit) = &mut self.edit_track {
                    match field {
                        EditTrackField::Title => edit.title = value,
                        EditTrackField::Artist => edit.artist = value,
                    }
                }
                Task::none()
            }
            Message::EditTrackSelectProvider(provider) => {
                if let Some(edit) = &mut self.edit_track {
                    edit.source = provider;
                }
                Task::none()
            }
            Message::EditTrackFindProvider(provider) => {
                self.handle_edit_track_find_provider(provider);
                Task::none()
            }
            Message::CloseEditTrack => {
                self.edit_track = None;
                Task::none()
            }
            Message::SaveEditTrack => {
                self.apply_edit_track();
                Task::none()
            }
            Message::CloseContextMenu => {
                self.close_context_menu();
                Task::none()
            }
            Message::DepToggle(kind) => {
                if let Some(dialog) = &mut self.dep_dialog {
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
                self.dep_dialog = None;
                // If the active source is no longer searchable (its tools were
                // not installed), fall back to one that is.
                if !self.search_provider.capabilities().search {
                    self.search_provider = ProviderId::searchable()
                        .iter()
                        .copied()
                        .find(|p| p.capabilities().search)
                        .unwrap_or(ProviderId::SoundCloud);
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
        let pending = match &self.dep_dialog {
            Some(dialog) => dialog.pending(&self.dep_ops),
            None => return Task::none(),
        };
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

    #[allow(clippy::unused_self)]
    pub fn subscription(&self) -> Subscription<Message> {
        let timer = iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick);

        let events = iced::event::listen_with(Self::event_to_message);

        Subscription::batch([timer, events])
    }

    // `iced::event::listen_with` hands the event over by value, so the
    // by-value parameter is mandated by the API.
    #[allow(clippy::needless_pass_by_value)]
    fn event_to_message(
        event: iced::Event,
        status: iced::event::Status,
        _window: iced::window::Id,
    ) -> Option<Message> {
        match event {
            iced::Event::Mouse(iced::mouse::Event::CursorMoved { position }) => {
                Some(Message::CursorMoved(position))
            }
            iced::Event::Mouse(iced::mouse::Event::ButtonReleased(iced::mouse::Button::Left)) => {
                Some(Message::LeftButtonReleased)
            }
            iced::Event::Keyboard(iced::keyboard::Event::KeyPressed {
                physical_key,
                modifiers,
                ..
            }) if status == iced::event::Status::Ignored => Some(Message::KeyPressed {
                key: physical_key,
                modifiers,
            }),
            iced::Event::Window(iced::window::Event::CloseRequested) => Some(Message::WindowClose),
            iced::Event::Window(iced::window::Event::Resized(size)) => {
                Some(Message::WindowResized(size))
            }
            _ => None,
        }
    }
}
