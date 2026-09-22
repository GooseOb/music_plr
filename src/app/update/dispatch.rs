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
        interaction::{DefaultCtxAction, TrackListKind},
        message::{BackendResult, EditTrackField, Message},
        update::operation::{CaptureContextMenu, CaptureSearchHistoryRows, ContextMenuGeometry},
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
                    let _ = id;
                    Task::none()
                }
            }
            Message::MediaHwnd(hwnd) => {
                self.init_media_controls(hwnd.map(|h| h as *mut std::ffi::c_void));
                Task::none()
            }
            Message::WindowClose => {
                self.flush_session();
                Task::none()
            }
            Message::CursorMoved(pos) => self.handle_cursor_moved(pos),
            Message::LeftButtonReleased => self.handle_left_release(),
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
            Message::ListScrolled {
                pane,
                list,
                translation_y,
            } => {
                let geo = match list {
                    TrackListKind::Queue => self.bounds.queue.as_mut(),
                    TrackListKind::Active => self.bounds.tracks.get_mut(&pane),
                    TrackListKind::Recent => self.bounds.recent.as_mut(),
                };
                if let Some(g) = geo {
                    g.translation_y = translation_y;
                }
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
            Message::KeyPressed { key, modifiers } => self.handle_key_press(key, modifiers),
            Message::ModifiersChanged(modifiers) => {
                self.modifiers = modifiers;
                Task::none()
            }
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
                let text = match &state.lyrics {
                    LoadState::Ready(lyrics) => {
                        if state.mode == crate::app::LyricsViewMode::Synced
                            && !lyrics.timed.is_empty()
                        {
                            lyrics.to_edit_text()
                        } else {
                            lyrics.plain.clone()
                        }
                    }
                    _ => return Task::none(),
                };
                if text.is_empty() {
                    return Task::none();
                }
                self.notify(self.strings.lyrics_copied);
                iced::clipboard::write(text)
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
            } => self.open_artist(pane, Some(&id), &name, source),
            Message::ArtistSectionProviderChanged(pane, section, provider) => {
                self.handle_artist_section_provider_changed(pane, section, provider);
                Task::none()
            }
            Message::ArtistHeaderProviderChanged(pane, provider) => {
                self.handle_artist_header_provider_changed(pane, provider);
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
            Message::OpenPlaylistJump => self.open_playlist_jump(),
            Message::PlaylistJumpInput(query) => {
                if let Some(Dialog::PlaylistJump(jump)) = &mut self.dialog {
                    jump.query = query;
                    jump.selected = 0;
                }
                Task::none()
            }
            Message::PlaylistJumpConfirm => {
                let play = self.modifiers.shift();
                self.confirm_playlist_jump(play)
            }
            Message::PlaylistJumpOpen(index) => {
                self.dialog = None;
                self.handle_select_playlist(index)
            }
            Message::PlaylistJumpOpenPlay(index) => {
                self.dialog = None;
                self.handle_open_and_play_playlist(index)
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
                if let Some(Dialog::Picker(picker)) = &self.dialog {
                    let indices = picker.indices.clone();
                    self.handle_add_to_playlist(picker.pane, playlist_idx, &indices, picker.list);
                }
                Task::none()
            }
            Message::TogglePicker(indices) => {
                let (pane, list) = match &self.dialog {
                    Some(Dialog::ContextMenu(m)) => (m.pos.pane, m.pos.list),
                    _ => (self.focused_pane_id, TrackListKind::Active),
                };
                self.handle_toggle_picker(pane, indices, list);
                Task::none()
            }
            Message::CloseDialog => {
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
                if let Some(Dialog::ContextMenu(menu)) = &mut self.dialog {
                    menu.hovered = focus;
                }
                Task::none()
            }
            Message::ContextMenuBoundsCaptured { panel, row_offsets } => {
                let prev = self.bounds.context_menu.take();
                let width_changed = prev
                    .as_ref()
                    .is_none_or(|p| (p.panel.width - panel.width).abs() > f32::EPSILON);
                let window = self.window_size;
                let moved = match &mut self.dialog {
                    Some(Dialog::ContextMenu(menu)) => menu.flip_position(panel, window),
                    _ => false,
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
            Message::ContextMenuAddToQueue(list, indices) => {
                let pane = match &self.dialog {
                    Some(Dialog::ContextMenu(m)) => m.pos.pane,
                    _ => self.focused_pane_id,
                };
                self.close_context_menu();
                self.handle_add_to_queue(pane, list, &indices);
                Task::none()
            }
            Message::ContextMenuRemoveFromList(list, indices) => {
                let pane = match &self.dialog {
                    Some(Dialog::ContextMenu(m)) => m.pos.pane,
                    _ => self.focused_pane_id,
                };
                self.close_context_menu();
                match list {
                    TrackListKind::Queue => self.handle_remove_from_queue_batch(&indices),
                    TrackListKind::Recent => self.handle_remove_from_recent_batch(&indices),
                    TrackListKind::Active => self.handle_remove_from_playlist_batch(pane, &indices),
                }
                Task::none()
            }
            Message::ContextMenuEditTrack => {
                let Some(Dialog::ContextMenu(menu)) = &self.dialog else {
                    return Task::none();
                };
                let pos = menu.pos;
                self.dialog = None;
                self.bounds.context_menu = None;
                self.open_edit_track(pos);
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

    #[allow(clippy::unused_self)]
    pub fn subscription(&self) -> Subscription<Message> {
        let timer = iced::time::every(Duration::from_millis(250)).map(|_| Message::Tick);

        let events = iced::event::listen_with(Self::event_to_message);

        Subscription::batch([timer, events, Self::media_hwnd_subscription()])
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
            iced::Event::Keyboard(iced::keyboard::Event::ModifiersChanged(modifiers)) => {
                Some(Message::ModifiersChanged(modifiers))
            }
            iced::Event::Window(iced::window::Event::CloseRequested) => Some(Message::WindowClose),
            iced::Event::Window(iced::window::Event::Resized(size)) => {
                Some(Message::WindowResized(size))
            }
            _ => None,
        }
    }
}
