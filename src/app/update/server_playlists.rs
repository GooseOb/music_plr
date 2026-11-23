//! Server-side playlists: the sidebar index, opening one, and the writes that
//! go back to the server (add, remove, create, rename, delete).
//!
//! The index ([`ServerPlaylistStore`]) holds identity only; contents always
//! come from the provider's `browse` path, so a playlist edited elsewhere shows
//! the server's state. Every write runs on a background thread and reports
//! back through [`BackendResult::ServerPlaylistWritten`], after which the open
//! view is refetched — the server stays the source of truth. A rename is typed
//! into the page's name input and debounced: every keystroke updates the open
//! views and the cached index, and the write goes out once the typing pauses.

use std::time::Duration;

use iced::Task;

use super::{BackendResult, Message, MusicPlayer, TrackPos, ViewData, PREPEND};
use crate::{
    app::{
        pane::PaneId, Dialog, PendingPlaylistWrite, PendingServerRename, ServerPlaylistChange,
        ServerPlaylistNameDialog, ServerPlaylistUndo, ViewKind,
    },
    data::{
        library::{Card, LibraryItem, LibraryKind},
        server_playlists::{ServerPlaylistEntry, ServerPlaylistId},
        JsonStore,
    },
    load_state::LoadState,
    providers::{self, ProviderError, ProviderId, ServerPlaylist},
    types::Track,
};

/// Rows one write put on one pane's page: the index each took and the track it
/// was, so a failed write can find them again after the page moved.
type AddedRows = Vec<(PaneId, Vec<(usize, Track)>)>;

/// How long an index request may stay unanswered before the guard lets a new
/// one through. Every provider call times out well inside this, so an entry
/// that old belongs to a thread that will never report back.
const INDEX_REQUEST_TIMEOUT: Duration = Duration::from_mins(2);

/// The quiet period after the last rename keystroke before the write goes
/// out to the server. The tick flushes the due ones, so the real wait is
/// this plus up to a tick.
const RENAME_DEBOUNCE: Duration = Duration::from_secs(1);

impl MusicPlayer {
    /// Every configured server that has playlists, in provider order so the
    /// sidebar groups and the palette rows agree on what "row 3" means.
    pub fn server_playlist_providers() -> Vec<ProviderId> {
        ProviderId::all()
            .into_iter()
            .filter(|provider| provider.capabilities().server_playlists)
            .collect()
    }

    /// The sidebar's server rows in render order: each provider's playlists,
    /// grouped by provider. The captured row geometry indexes into this, so
    /// the rendered order and this list must be built the same way.
    pub fn server_playlist_rows(&self) -> Vec<(ProviderId, &ServerPlaylistEntry)> {
        Self::server_playlist_providers()
            .into_iter()
            .flat_map(|provider| {
                self.server_playlists
                    .for_provider(provider)
                    .map(move |entry| (provider, entry))
            })
            .collect()
    }

    /// The `index`-th row of [`server_playlist_rows`](Self::server_playlist_rows),
    /// resolved without building the list: drop hit-testing reads one row per
    /// pointer move.
    pub fn server_playlist_row(&self, index: usize) -> Option<(ProviderId, &ServerPlaylistEntry)> {
        let mut skip = index;
        for provider in Self::server_playlist_providers() {
            for entry in self.server_playlists.for_provider(provider) {
                if skip == 0 {
                    return Some((provider, entry));
                }
                skip -= 1;
            }
        }
        None
    }

    pub fn server_playlist_name(&self, target: &ServerPlaylistId) -> String {
        self.server_playlists
            .get(target)
            .map(|entry| entry.name.clone())
            .unwrap_or_default()
    }

    /// A server-playlist row as a card, so dragging it onto the local
    /// playlist list goes through the same "a dropped card becomes a
    /// playlist" path every other card uses.
    pub(crate) fn server_playlist_card(&self, target: &ServerPlaylistId) -> Card {
        std::sync::Arc::new(LibraryItem {
            kind: LibraryKind::Playlist,
            id: target.id.clone(),
            title: self.server_playlist_name(target),
            thumbnail: String::new(),
            provider: target.provider,
            url: String::new(),
        })
    }

    /// Reload one server's playlist index, or every configured server's.
    /// A provider with a request already in flight is skipped, so the boot
    /// refresh and a manual one share a single call.
    pub fn refresh_server_playlists(&mut self, provider: Option<ProviderId>) -> Task<Message> {
        let providers =
            provider.map_or_else(Self::server_playlist_providers, |provider| vec![provider]);
        for provider in providers {
            if self.server_playlist_index_busy(provider) {
                continue;
            }
            self.server_playlists_requested
                .insert(provider, std::time::Instant::now());
            let tx = self.result_tx.clone();
            std::thread::spawn(move || {
                let result = providers::list_playlists(provider).map_err(ProviderError::from);
                let _ = tx.send(BackendResult::ServerPlaylists { provider, result });
            });
        }
        Task::none()
    }

    /// Whether a write for `target` is in flight, which is what the sidebar row
    /// and the playlist header show in place of the track count.
    pub fn server_playlist_write_pending(&self, target: &ServerPlaylistId) -> bool {
        self.server_playlist_writes
            .values()
            .any(|write| &write.target == target)
    }

    /// Whether `provider`'s index is still being fetched, which is what the
    /// sidebar's refresh glyph shows. An entry older than
    /// [`INDEX_REQUEST_TIMEOUT`] is treated as gone.
    pub fn server_playlist_index_busy(&self, provider: ProviderId) -> bool {
        self.server_playlists_requested
            .get(&provider)
            .is_some_and(|sent| sent.elapsed() < INDEX_REQUEST_TIMEOUT)
    }

    /// Index entries for servers that no longer exist: a playlist created on a
    /// server deleted between the write and its reply has no group to show in,
    /// and the server's own removal normally handled it before it could land.
    pub(crate) fn drop_unregistered_server_playlists(&mut self) {
        let before = self.server_playlists.entries.len();
        self.server_playlists
            .entries
            .retain(|entry| match entry.provider {
                ProviderId::Custom(kind, id) => crate::data::custom_providers::contains(kind, id),
                _ => true,
            });
        if self.server_playlists.entries.len() != before {
            self.server_playlists.save();
        }
    }

    /// Open a server playlist in `pane`, fetching its contents. `play` starts
    /// playback once the tracks arrive (the sidebar's double click).
    pub fn open_server_playlist(
        &mut self,
        pane: PaneId,
        target: ServerPlaylistId,
        name: &str,
        play: bool,
    ) -> Task<Message> {
        let already_open = matches!(
            &self.view_data_in(pane).kind,
            ViewKind::PlaylistView(reference)
                if reference.provider == target.provider && reference.id == target.id
        );
        if already_open {
            return if play {
                self.play_first_track(pane)
            } else {
                Task::none()
            };
        }
        let provider = target.provider;
        let thumbnail = self
            .server_playlists
            .get(&target)
            .map(|entry| entry.thumbnail.clone())
            .unwrap_or_default();
        let kind = ViewKind::PlaylistView(crate::app::view_data::PlaylistRef {
            id: target.id,
            name: name.to_string(),
            provider,
            thumbnail,
            url: String::new(),
        });
        let task = self.handle_browse(pane, &kind, provider);
        if play {
            let rid = self.view_data_in(pane).request_id;
            self.play_after_browse.insert(rid);
        }
        task
    }

    /// Refetch the server playlist `pane` is showing, keeping the page (and
    /// its history entry) in place.
    pub fn refresh_open_server_playlist(&mut self, pane: PaneId) -> Task<Message> {
        let ViewKind::PlaylistView(reference) = &self.view_data_in(pane).kind else {
            return Task::none();
        };
        let provider = reference.provider;
        if !provider.capabilities().server_playlists {
            return Task::none();
        }
        let id = reference.id.clone();
        let rid = self.request_ids.next();
        providers::invalidate_server_playlist(provider, &id);
        let view = self.view_data_in_mut(pane);
        view.content = LoadState::Loading;
        view.request_id = rid;
        let tx = self.result_tx.clone();
        std::thread::spawn(move || match providers::browse(provider, &id, "playlist") {
            Ok((tracks, meta)) => {
                let _ = tx.send(BackendResult::BrowseResults(rid, tracks, meta));
            }
            Err(error) => {
                let _ = tx.send(BackendResult::SearchError(rid, provider, error.into()));
            }
        });
        Task::none()
    }

    /// Start playing the list the pane shows, when it has anything in it.
    fn play_first_track(&mut self, pane: PaneId) -> Task<Message> {
        if self.track_count_in(pane) == 0 {
            return Task::none();
        }
        self.handle_play_track(TrackPos::new(0, pane));
        Task::none()
    }

    /// Add `tracks` to a server playlist. Snapshot playlists put them at the
    /// beginning, like an add to a local playlist; id-based servers append,
    /// since their write API has no insertion parameter. The sidebar count and
    /// every open page update right away (no loading state); tracks without an
    /// id on that server are reported as skipped, and a failed write rolls
    /// both back.
    pub fn add_tracks_to_server_playlist(
        &mut self,
        target: ServerPlaylistId,
        tracks: Vec<Track>,
    ) -> Task<Message> {
        self.write_tracks_to_server_playlist(target, tracks, PREPEND)
    }

    /// Add `tracks` at `index` in a server playlist. Snapshot playlists (S3)
    /// keep the exact position; id-based servers append, since their write
    /// API has no insertion parameter.
    pub fn insert_tracks_into_server_playlist(
        &mut self,
        target: ServerPlaylistId,
        tracks: Vec<Track>,
        index: usize,
    ) -> Task<Message> {
        self.write_tracks_to_server_playlist(target, tracks, index)
    }

    fn write_tracks_to_server_playlist(
        &mut self,
        target: ServerPlaylistId,
        tracks: Vec<Track>,
        at: usize,
    ) -> Task<Message> {
        if tracks.is_empty() {
            return Task::none();
        }
        let provider = target.provider;
        let snapshots = provider.playlists_store_snapshots();
        // Only the tracks that can land are shown right away; the provider
        // repeats the same filter and reports the rest as skipped once the
        // write lands. A snapshot playlist (S3) stores whole tracks, so every
        // track fits, but it skips the ones it already holds — the optimistic
        // rows have to skip those too or the page would show rows the bucket
        // never stored.
        let added: Vec<Track> = if snapshots {
            let known = providers::snapshot_playlist_tracks(provider, &target.id);
            tracks
                .iter()
                .filter(|track| {
                    !known
                        .iter()
                        .any(|existing| existing.cache_key() == track.cache_key())
                })
                .cloned()
                .collect()
        } else {
            tracks
                .iter()
                .filter(|track| track.provider_id(provider).is_some_and(|id| !id.is_empty()))
                .cloned()
                .collect()
        };
        if added.is_empty() {
            if snapshots {
                let name = self.server_playlist_name(&target);
                self.notify((self.strings.added_to)(0, &name));
            } else {
                self.notify_error((self.strings.skipped_not_on_server)(tracks.len()));
            }
            return Task::none();
        }
        let (count, rows) = self.apply_inserted_tracks(&target, &added, snapshots.then_some(at));
        let undo = ServerPlaylistUndo::Added { count, rows };
        let playlist_id = target.id.clone();
        let request = if snapshots { added } else { tracks };
        self.spawn_playlist_write(target, Some(undo), move || {
            let write = if snapshots {
                providers::insert_into_playlist(provider, &playlist_id, at, &request)?
            } else {
                providers::add_to_playlist(provider, &playlist_id, &request)?
            };
            Ok(ServerPlaylistChange::Added {
                added: write.added,
                skipped: write.skipped,
            })
        });
        Task::none()
    }

    /// Rewrite a server playlist's track order. The caller has already
    /// reordered the rows it shows, so a success needs no refetch; `undo` puts
    /// the order it had back if the server rejects the write.
    pub fn reorder_server_playlist(
        &mut self,
        target: ServerPlaylistId,
        tracks: Vec<Track>,
        undo: ServerPlaylistUndo,
    ) {
        let provider = target.provider;
        let playlist_id = target.id.clone();
        self.spawn_playlist_write(target, Some(undo), move || {
            providers::reorder_playlist(provider, &playlist_id, &tracks)?;
            Ok(ServerPlaylistChange::Reordered)
        });
    }

    /// Save a card as a new playlist on `provider`. The server needs the track
    /// ids, so the card's contents are browsed first and the playlist is
    /// created with them; like the dialog's create, the row only appears once
    /// the server has the playlist (a new id exists only afterwards).
    pub fn create_server_playlist_from_card(&mut self, item: &LibraryItem, provider: ProviderId) {
        let name = item.title.clone();
        let id = item.id.clone();
        let kind = item.kind.browse_kind();
        self.spawn_playlist_write(
            ServerPlaylistId {
                provider,
                id: String::new(),
            },
            None,
            move || {
                let (tracks, _) = providers::browse(provider, &id, kind)?;
                let created = providers::create_playlist(provider, &name, &tracks)?;
                Ok(ServerPlaylistChange::Created {
                    id: created.id,
                    name: created.name,
                    track_count: created.track_count,
                    thumbnail: created.thumbnail,
                })
            },
        );
    }

    /// Remove the rows at `indices` from the server playlist the pane shows.
    /// The rows leave every open page immediately; a failed write puts them
    /// back where they were.
    pub fn remove_from_server_playlist(&mut self, pane: PaneId, indices: &[usize]) {
        if indices.is_empty() {
            return;
        }
        let ViewKind::PlaylistView(reference) = &self.view_data_in(pane).kind else {
            return;
        };
        let provider = reference.provider;
        if !provider.capabilities().server_playlists {
            return;
        }
        let target = ServerPlaylistId {
            provider,
            id: reference.id.clone(),
        };
        let rows = self.apply_removed_rows(&target, indices);
        let playlist_id = target.id.clone();
        let positions = indices.to_vec();
        self.spawn_playlist_write(
            target,
            Some(ServerPlaylistUndo::Removed {
                count: indices.len(),
                rows,
            }),
            move || {
                let removed = providers::remove_from_playlist(provider, &playlist_id, &positions)?;
                Ok(ServerPlaylistChange::Removed(removed))
            },
        );
    }

    /// Put `tracks` at `index` (`None` appends) in every open page showing the
    /// playlist, shifting the selection along with the rows that moved down.
    fn apply_inserted_tracks(
        &mut self,
        target: &ServerPlaylistId,
        tracks: &[Track],
        index: Option<usize>,
    ) -> (usize, AddedRows) {
        self.server_playlists
            .adjust_count(target, tracks.len().cast_signed());
        Self::seed_tracks_thumbnails(&mut self.thumbnail_index, tracks);
        let mut rows = Vec::new();
        for pane in self.panes_showing(target) {
            let view = self.view_data_in_mut(pane);
            let LoadState::Ready(list) = &mut view.content else {
                continue;
            };
            let at = index.unwrap_or(list.len()).min(list.len());
            list.splice(at..at, tracks.iter().cloned());
            for selected in &mut view.selection {
                if *selected >= at {
                    *selected += tracks.len();
                }
            }
            rows.push((
                pane,
                tracks
                    .iter()
                    .enumerate()
                    .map(|(offset, track)| (at + offset, track.clone()))
                    .collect(),
            ));
        }
        (tracks.len(), rows)
    }

    /// Drop `indices` from the cached count and from every open page showing
    /// the playlist, remapping each page's selection. Returns the rows it took
    /// out, grouped by pane, so a failed write can put them back.
    fn apply_removed_rows(
        &mut self,
        target: &ServerPlaylistId,
        indices: &[usize],
    ) -> Vec<(PaneId, Vec<(usize, Track)>)> {
        self.server_playlists
            .adjust_count(target, -(indices.len().cast_signed()));
        let mut removed = Vec::new();
        for pane in self.panes_showing(target) {
            let view = self.view_data_in_mut(pane);
            let Some(list) = view.tracks_mut() else {
                continue;
            };
            let mut rows: Vec<(usize, Track)> = indices
                .iter()
                .filter_map(|&index| list.get(index).map(|track| (index, track.clone())))
                .collect();
            rows.sort_unstable_by_key(|(index, _)| *index);
            crate::util::remove_at(list, indices);
            crate::util::remap_removed_indices(&mut view.selection, indices);
            removed.push((pane, rows));
        }
        removed
    }

    fn spawn_playlist_write<F>(
        &mut self,
        target: ServerPlaylistId,
        undo: Option<ServerPlaylistUndo>,
        run: F,
    ) where
        F: FnOnce() -> anyhow::Result<ServerPlaylistChange> + Send + 'static,
    {
        let write_id = self.request_ids.next();
        self.server_playlist_writes.insert(
            write_id,
            PendingPlaylistWrite {
                target: target.clone(),
                undo,
            },
        );
        let tx = self.result_tx.clone();
        std::thread::spawn(move || {
            let result = run().map_err(ProviderError::from);
            let _ = tx.send(BackendResult::ServerPlaylistWritten {
                write_id,
                target,
                result,
            });
        });
    }

    /// Undo one failed write by reversing what it changed, so a write that
    /// overlapped it keeps its own optimistic state.
    fn undo_playlist_write(&mut self, target: &ServerPlaylistId, undo: ServerPlaylistUndo) {
        match undo {
            ServerPlaylistUndo::Added { count, rows } => {
                self.undo_added_tracks(target, count, &rows);
            }
            ServerPlaylistUndo::Removed { count, rows } => {
                self.undo_removed_rows(target, count, &rows);
            }
            ServerPlaylistUndo::Reordered {
                pane,
                tracks,
                selection,
            } => {
                if !self.shows_playlist_in(pane, target) {
                    return;
                }
                let view = self.view_data_in_mut(pane);
                view.content = LoadState::Ready(tracks);
                view.selection = selection;
            }
            ServerPlaylistUndo::Renamed { previous } => {
                self.server_playlists.rename(target, &previous);
                self.rename_server_playlist_views(target, &previous);
            }
        }
    }

    /// Take back the rows an add put on every page showing the playlist. The
    /// recorded index is used while the row still matches; a row that moved
    /// because the page changed under the write is found by identity instead,
    /// so an overlapping removal keeps its own rows.
    fn undo_added_tracks(&mut self, target: &ServerPlaylistId, count: usize, rows: &AddedRows) {
        self.server_playlists
            .adjust_count(target, -(count.cast_signed()));
        for (pane, rows) in rows {
            if !self.shows_playlist_in(*pane, target) {
                continue;
            }
            let view = self.view_data_in_mut(*pane);
            let Some(list) = view.tracks_mut() else {
                continue;
            };
            let mut targets = Vec::with_capacity(rows.len());
            for (index, track) in rows {
                let at = if list.get(*index) == Some(track) {
                    Some(*index)
                } else {
                    list.iter().position(|row| row == track)
                };
                if let Some(at) = at {
                    if !targets.contains(&at) {
                        targets.push(at);
                    }
                }
            }
            targets.sort_unstable();
            crate::util::remove_at(list, &targets);
            crate::util::remap_removed_indices(&mut view.selection, &targets);
        }
    }

    /// Put the rows a failed removal took out back where they were, and shift
    /// the selection back with them.
    fn undo_removed_rows(&mut self, target: &ServerPlaylistId, count: usize, rows: &AddedRows) {
        self.server_playlists
            .adjust_count(target, count.cast_signed());
        for pane in self.panes_showing(target) {
            let Some((_, rows)) = rows.iter().find(|(recorded, _)| *recorded == pane) else {
                continue;
            };
            let view = self.view_data_in_mut(pane);
            let Some(list) = view.tracks_mut() else {
                continue;
            };
            let mut restored = Vec::with_capacity(rows.len());
            for (index, track) in rows {
                let at = (*index).min(list.len());
                list.insert(at, track.clone());
                restored.push(at);
            }
            crate::util::remap_reinserted_indices(&mut view.selection, &restored);
        }
    }

    fn panes_showing(&self, target: &ServerPlaylistId) -> Vec<PaneId> {
        self.pane_ids()
            .into_iter()
            .filter(|pane| self.shows_playlist_in(*pane, target))
            .collect()
    }

    /// Whether `pane` still exists and shows `target` — a pane can be closed
    /// while a write is in flight.
    fn shows_playlist_in(&self, pane: PaneId, target: &ServerPlaylistId) -> bool {
        self.session
            .panes
            .get(&pane)
            .is_some_and(|pane| shows_playlist(pane.view_data(), target))
    }

    pub fn open_new_server_playlist(&mut self, provider: ProviderId) {
        self.dialog = Some(Dialog::ServerPlaylistName(
            ServerPlaylistNameDialog::new_playlist(provider),
        ));
    }

    /// Rename the server playlist `pane` shows while the user types. Every
    /// open view and the local index pick the name up at once (optimistic,
    /// like every other write); the write to the server waits out a pause in
    /// the typing.
    pub fn handle_server_playlist_rename(&mut self, pane: PaneId, name: &str) {
        let target = match &self.view_data_in(pane).kind {
            ViewKind::PlaylistView(reference)
                if reference.provider.capabilities().server_playlists
                    && !name.trim().is_empty() =>
            {
                ServerPlaylistId {
                    provider: reference.provider,
                    id: reference.id.clone(),
                }
            }
            _ => return,
        };
        let name = name.trim();

        // Arm or re-arm the debounce. An already-typed rename only updates
        // its own record; a first one must snapshot what the server still
        // holds, so a rejected write can restore it.
        let had_pending = self
            .pending_server_renames
            .get_mut(&target)
            .map(|pending| {
                pending.name = name.to_string();
                pending.due = std::time::Instant::now() + RENAME_DEBOUNCE;
            })
            .is_some();

        // The new name shows up in the sidebar and every open page header at
        // once; a rejected rename puts the old one back.
        let previous = self.server_playlist_name(&target);
        self.server_playlists.rename(&target, name);
        self.rename_server_playlist_views(&target, name);

        if !had_pending {
            self.pending_server_renames.insert(
                target,
                PendingServerRename {
                    previous,
                    name: name.to_string(),
                    due: std::time::Instant::now() + RENAME_DEBOUNCE,
                },
            );
        }
    }

    /// Send the renames whose quiet period has lapsed. Called from the tick,
    /// so the write can be up to one tick late.
    pub(crate) fn flush_server_playlist_renames(&mut self) {
        let due: Vec<ServerPlaylistId> = self
            .pending_server_renames
            .iter()
            .filter(|(_, pending)| pending.due <= std::time::Instant::now())
            .map(|(target, _)| target.clone())
            .collect();
        for target in due {
            let Some(pending) = self.pending_server_renames.remove(&target) else {
                continue;
            };
            self.write_server_playlist_rename(target, pending);
        }
    }

    /// Send one rename the user finished typing, through the ordinary write
    /// pipeline (its undo was recorded when the typing started).
    fn write_server_playlist_rename(
        &mut self,
        target: ServerPlaylistId,
        pending: PendingServerRename,
    ) {
        let name = pending.name;
        let provider = target.provider;
        let playlist_id = target.id.clone();
        self.spawn_playlist_write(
            target,
            Some(ServerPlaylistUndo::Renamed {
                previous: pending.previous,
            }),
            move || {
                providers::rename_playlist(provider, &playlist_id, &name)?;
                Ok(ServerPlaylistChange::Renamed(name))
            },
        );
    }

    pub fn open_delete_server_playlist(&mut self, target: ServerPlaylistId, name: &str) {
        self.dialog = Some(Dialog::ServerPlaylistDelete(
            crate::app::ServerPlaylistDeleteDialog {
                target,
                name: name.to_string(),
            },
        ));
    }

    /// Create the playlist on its server (no id yet — the write is reported
    /// against an empty one and the entry lands once the server answers).
    pub fn submit_server_playlist_name(&mut self) -> Task<Message> {
        let Some(Dialog::ServerPlaylistName(dialog)) = &self.dialog else {
            return Task::none();
        };
        let name = dialog.name.trim().to_string();
        if name.is_empty() {
            return Task::none();
        }
        let provider = dialog.provider;
        self.dialog = None;
        self.spawn_playlist_write(
            ServerPlaylistId {
                provider,
                id: String::new(),
            },
            None,
            move || {
                providers::create_playlist(provider, &name, &[]).map(|created| {
                    ServerPlaylistChange::Created {
                        id: created.id,
                        name: created.name,
                        track_count: created.track_count,
                        thumbnail: created.thumbnail,
                    }
                })
            },
        );
        Task::none()
    }

    pub fn confirm_delete_server_playlist(&mut self) -> Task<Message> {
        let Some(Dialog::ServerPlaylistDelete(dialog)) = &self.dialog else {
            return Task::none();
        };
        let target = dialog.target.clone();
        self.dialog = None;
        let provider = target.provider;
        let playlist_id = target.id.clone();
        self.spawn_playlist_write(target, None, move || {
            providers::delete_playlist(provider, &playlist_id)?;
            Ok(ServerPlaylistChange::Deleted)
        });
        Task::none()
    }

    /// Apply a loaded playlist index. A failure keeps the rows already cached
    /// (the server may be briefly unreachable) and only reports it.
    pub(crate) fn apply_server_playlists(
        &mut self,
        provider: ProviderId,
        result: Result<Vec<ServerPlaylist>, ProviderError>,
    ) -> Task<Message> {
        self.server_playlists_requested.remove(&provider);
        match result {
            Ok(playlists) => {
                // The sidebar's rows are measured by the capture pass, so a
                // changed listing has to be re-measured before drag and drop
                // can address them.
                if self.server_playlists.replace_provider(provider, &playlists) {
                    self.capture_bounds_task()
                } else {
                    Task::none()
                }
            }
            Err(error) => {
                let message = self.strings.error_reason_on(provider, &error);
                self.notify_error(message);
                Task::none()
            }
        }
    }

    /// Apply a finished write. Every write touches the index and the open pages
    /// as it starts, so success only reports it and re-syncs the count; failure
    /// reverses what this write changed, leaving any sibling write's state
    /// alone.
    pub(crate) fn apply_server_playlist_write(
        &mut self,
        write_id: u64,
        target: &ServerPlaylistId,
        result: Result<ServerPlaylistChange, ProviderError>,
    ) -> Task<Message> {
        let pending = self.server_playlist_writes.remove(&write_id);
        let change = match result {
            Ok(change) => change,
            Err(error) => {
                let message = self.strings.error_reason_on(target.provider, &error);
                self.notify_error(message);
                if let Some(undo) = pending.and_then(|pending| pending.undo) {
                    self.undo_playlist_write(target, undo);
                }
                return Task::none();
            }
        };
        let name = self.server_playlist_name(target);
        match change {
            ServerPlaylistChange::Added { added, skipped } => {
                self.notify((self.strings.added_to)(added, &name));
                if skipped > 0 {
                    let message = (self.strings.skipped_not_on_server)(skipped);
                    self.notify_error(message);
                }
                self.sync_open_playlist_count(target);
                Task::none()
            }
            ServerPlaylistChange::Removed(removed) => {
                self.notify((self.strings.removed_n)(removed));
                self.sync_open_playlist_count(target);
                Task::none()
            }
            ServerPlaylistChange::Reordered => {
                self.sync_open_playlist_count(target);
                self.notify(self.strings.reordered_playlist);
                Task::none()
            }
            ServerPlaylistChange::Created {
                id,
                name,
                track_count,
                thumbnail,
            } => {
                self.notify((self.strings.playlist_created)(&name));
                // The server may have been deleted or switched off between the
                // write and its reply; with no sidebar group to draw the row,
                // the entry is dropped instead of lingering in the index.
                if target.provider.enabled() && target.provider.capabilities().server_playlists {
                    self.server_playlists.insert(ServerPlaylistEntry::new(
                        target.provider,
                        crate::providers::ServerPlaylist {
                            id,
                            name,
                            track_count,
                            thumbnail,
                        },
                    ));
                    // A new row was added to the measured sidebar list.
                    self.capture_bounds_task()
                } else {
                    Task::none()
                }
            }
            ServerPlaylistChange::Renamed(new_name) => {
                self.server_playlists.rename(target, &new_name);
                self.rename_server_playlist_views(target, &new_name);
                Task::none()
            }
            ServerPlaylistChange::Deleted => {
                // A rename typed just before may still sit in the debounce,
                // and there is no playlist left to rename.
                self.pending_server_renames.remove(target);
                self.server_playlists.remove(target);
                self.close_server_playlist_views(target)
            }
        }
    }

    /// Take the cached track count from an open page, whose rows are what the
    /// server confirmed (and what the user sees).
    fn sync_open_playlist_count(&mut self, target: &ServerPlaylistId) {
        let Some(pane) = self.panes_showing(target).first().copied() else {
            return;
        };
        let count = self.track_count_in(pane);
        self.server_playlists.set_count(target, count);
    }

    fn rename_server_playlist_views(&mut self, target: &ServerPlaylistId, name: &str) {
        for pane in self.pane_ids() {
            for entry in &mut self.pane_mut(pane).nav_history {
                let view = std::sync::Arc::make_mut(entry);
                if let ViewKind::PlaylistView(reference) = &mut view.kind {
                    if reference.provider == target.provider && reference.id == target.id {
                        reference.name = name.to_string();
                    }
                }
            }
        }
    }

    /// Replace every page showing the deleted playlist with a search view
    /// (the playlist no longer exists on the server).
    fn close_server_playlist_views(&mut self, target: &ServerPlaylistId) -> Task<Message> {
        let panes = self.panes_showing(target);
        let mut task = Task::none();
        for pane in panes {
            let provider = self.pane(pane).search_provider;
            let scope = self.pane(pane).search_scope;
            task = task.chain(
                self.push_new_view(pane, ViewData::new_search(String::new(), provider, scope)),
            );
        }
        task
    }
}

fn shows_playlist(view: &ViewData, target: &ServerPlaylistId) -> bool {
    matches!(
        &view.kind,
        ViewKind::PlaylistView(reference)
            if reference.provider == target.provider && reference.id == target.id
    )
}

#[cfg(test)]
mod tests {
    use crate::{
        app::{MusicPlayer, ViewData, ViewKind},
        data::{
            config,
            custom_providers::{self, CustomProviderKind},
            server_playlists::{ServerPlaylistEntry, ServerPlaylistId},
        },
        providers::{ErrorKind, ProviderError, ServerPlaylist},
        types::Track,
    };

    /// Trades the test servers into the registry for the body's duration, then
    /// restores whatever was registered before it.
    fn with_test_server(body: impl FnOnce()) {
        let _guard = custom_providers::test_registry();
        body();
    }

    fn provider() -> crate::providers::ProviderId {
        crate::providers::ProviderId::Custom(CustomProviderKind::Subsonic, 9)
    }

    fn track(id: &str) -> Track {
        Track::from_provider(
            provider(),
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

    fn target() -> ServerPlaylistId {
        ServerPlaylistId {
            provider: provider(),
            id: "pl".into(),
        }
    }

    fn s3_provider() -> crate::providers::ProviderId {
        crate::providers::ProviderId::Custom(CustomProviderKind::S3, 8)
    }

    fn s3_target() -> ServerPlaylistId {
        ServerPlaylistId {
            provider: s3_provider(),
            id: "pl".into(),
        }
    }

    /// A player whose single pane shows a server playlist holding `ids`.
    fn player_with_playlist(ids: &[&str]) -> MusicPlayer {
        player_with_playlist_of(provider(), ids)
    }

    fn player_with_s3_playlist(ids: &[&str]) -> MusicPlayer {
        player_with_playlist_of(s3_provider(), ids)
    }

    fn player_with_playlist_of(
        provider: crate::providers::ProviderId,
        ids: &[&str],
    ) -> MusicPlayer {
        let mut p = MusicPlayer::new_with(config::Config::default());
        p.server_playlists.entries.clear();
        p.server_playlists.replace_provider(
            provider,
            &[ServerPlaylist {
                id: "pl".into(),
                name: "Server".into(),
                thumbnail: String::new(),
                track_count: ids.len(),
            }],
        );
        let mut view = ViewData {
            kind: ViewKind::PlaylistView(crate::app::view_data::PlaylistRef {
                id: "pl".into(),
                name: "Server".into(),
                provider,
                thumbnail: String::new(),
                url: String::new(),
            }),
            ..Default::default()
        };
        view.set_tracks(ids.iter().map(|id| track(id)).collect());
        p.reset_test_pane(vec![view]);
        p
    }

    fn titles(p: &MusicPlayer) -> Vec<String> {
        p.view_data()
            .tracks()
            .iter()
            .map(|t| t.title.clone())
            .collect()
    }

    fn count(p: &MusicPlayer) -> Option<usize> {
        count_for(p, &target())
    }

    fn count_for(p: &MusicPlayer, target: &ServerPlaylistId) -> Option<usize> {
        p.server_playlists
            .get(target)
            .map(|entry| entry.track_count)
    }

    fn fail(p: &mut MusicPlayer) {
        fail_for(p, &target());
    }

    fn fail_for(p: &mut MusicPlayer, target: &ServerPlaylistId) {
        settle_for(
            p,
            target,
            Err(ProviderError::new(ErrorKind::Unavailable, "gone")),
        );
    }

    /// Answer the newest write still in flight, the way the background thread
    /// would.
    fn settle(
        p: &mut MusicPlayer,
        result: Result<crate::app::ServerPlaylistChange, ProviderError>,
    ) {
        settle_for(p, &target(), result);
    }

    fn settle_for(
        p: &mut MusicPlayer,
        target: &ServerPlaylistId,
        result: Result<crate::app::ServerPlaylistChange, ProviderError>,
    ) {
        let write_id = *p
            .server_playlist_writes
            .keys()
            .max()
            .expect("a write in flight");
        let _ = p.apply_server_playlist_write(write_id, target, result);
    }

    #[test]
    fn added_tracks_show_up_before_the_write_lands() {
        let mut p = player_with_playlist(&["a"]);
        let _ = p.add_tracks_to_server_playlist(target(), vec![track("b")]);

        assert_eq!(titles(&p), ["Track a", "Track b"]);
        assert_eq!(count(&p), Some(2));
        // The write and the inverse of what it changed are tracked together.
        assert!(p.server_playlist_write_pending(&target()));
    }

    #[test]
    fn a_failed_add_rolls_the_rows_and_count_back() {
        let mut p = player_with_playlist(&["a"]);
        p.view_data_mut().selection = vec![0];
        let _ = p.add_tracks_to_server_playlist(target(), vec![track("b")]);
        fail(&mut p);

        assert_eq!(titles(&p), ["Track a"]);
        assert_eq!(count(&p), Some(1));
        assert_eq!(p.view_data().selection, [0]);
        assert!(!p.server_playlist_write_pending(&target()));
    }

    #[test]
    fn a_failed_write_leaves_a_sibling_write_alone() {
        let mut p = player_with_playlist(&["a"]);
        let _ = p.add_tracks_to_server_playlist(target(), vec![track("b")]);
        let first = *p.server_playlist_writes.keys().min().expect("a write");
        let _ = p.add_tracks_to_server_playlist(target(), vec![track("c")]);

        let error = ProviderError::new(ErrorKind::Unavailable, "gone");
        let _ = p.apply_server_playlist_write(first, &target(), Err(error));

        // Only the failed append is undone; the other write is still in flight
        // and keeps its rows.
        assert_eq!(titles(&p), ["Track a", "Track c"]);
        assert_eq!(count(&p), Some(2));
        assert!(p.server_playlist_write_pending(&target()));
    }

    #[test]
    fn inserted_tracks_land_at_the_drop_position() {
        let mut p = player_with_s3_playlist(&["a", "b"]);
        let _guard = custom_providers::test_registry();
        p.view_data_mut().selection = vec![1];
        let _ = p.insert_tracks_into_server_playlist(s3_target(), vec![track("x")], 1);

        assert_eq!(titles(&p), ["Track a", "Track x", "Track b"]);
        assert_eq!(count_for(&p, &s3_target()), Some(3));
        assert_eq!(p.view_data().selection, [2]);
    }

    #[test]
    fn a_failed_insert_rolls_the_rows_and_selection_back() {
        let mut p = player_with_s3_playlist(&["a", "b"]);
        let _guard = custom_providers::test_registry();
        p.view_data_mut().selection = vec![1];
        let _ = p.insert_tracks_into_server_playlist(s3_target(), vec![track("x")], 1);
        fail_for(&mut p, &s3_target());

        assert_eq!(titles(&p), ["Track a", "Track b"]);
        assert_eq!(count_for(&p, &s3_target()), Some(2));
        assert_eq!(p.view_data().selection, [1]);
    }

    #[test]
    fn adding_to_a_snapshot_playlist_prepends() {
        let mut p = player_with_s3_playlist(&["a", "b"]);
        let _guard = custom_providers::test_registry();
        let _ = p.add_tracks_to_server_playlist(s3_target(), vec![track("x")]);

        assert_eq!(titles(&p), ["Track x", "Track a", "Track b"]);
        assert_eq!(count_for(&p, &s3_target()), Some(3));
    }

    #[test]
    fn pasting_into_a_server_playlist_prepends_the_clipboard() {
        let mut p = player_with_s3_playlist(&["a", "b"]);
        let _guard = custom_providers::test_registry();
        p.clipboard = vec![track("x")];
        let _ = p.handle_paste_clipboard();

        assert_eq!(titles(&p), ["Track x", "Track a", "Track b"]);
        assert_eq!(count_for(&p, &s3_target()), Some(3));
    }

    #[test]
    fn a_dropped_server_playlist_becomes_a_local_playlist() {
        use crate::app::interaction::{DropTarget, Pressed, PressedDrag};

        let mut p = player_with_playlist(&["a"]);
        p.playlists.playlists.clear();
        p.drag.pressed = Some(PressedDrag {
            what: Pressed::ServerPlaylist(target()),
            origin: iced::Point::ORIGIN,
        });
        p.drag.drag_active = true;
        p.drag.drop_target = Some(DropTarget::Playlist(0));

        let _ = p.handle_left_release();

        // The tracks arrive from the server afterwards, exactly like a card
        // dropped on the local list: the playlist exists first, then fills.
        assert_eq!(p.playlists.playlists.len(), 1);
        assert_eq!(p.playlists.playlists[0].name, "Server");
        assert!(p.playlists.playlists[0].tracks.is_empty());
    }

    #[test]
    fn a_created_playlist_lands_in_the_index_with_its_count() {
        let mut p = player_with_playlist(&["a"]);
        with_test_server(|| {
            let created = ServerPlaylistId {
                provider: provider(),
                id: String::new(),
            };
            let write_id = p.request_ids.next();
            let _ = p.apply_server_playlist_write(
                write_id,
                &created,
                Ok(crate::app::ServerPlaylistChange::Created {
                    id: "new".into(),
                    name: "New".into(),
                    track_count: 7,
                    thumbnail: String::new(),
                }),
            );

            let entry = p.server_playlists.get(&ServerPlaylistId {
                provider: provider(),
                id: "new".into(),
            });
            assert_eq!(entry.map(|entry| entry.track_count), Some(7));
        });
    }

    #[test]
    fn a_track_the_server_cannot_address_is_not_shown() {
        let mut p = player_with_playlist(&["a"]);
        let foreign = Track::from_provider(
            crate::providers::ProviderId::YouTube,
            "y".into(),
            "https://example.com/y".into(),
            String::from("YouTube"),
            "Artist",
            10,
            String::new(),
            None,
            None,
        );
        let _ = p.add_tracks_to_server_playlist(target(), vec![foreign]);

        assert_eq!(titles(&p), ["Track a"]);
        assert_eq!(count(&p), Some(1));
    }

    #[test]
    fn removed_rows_leave_the_page_at_once_and_come_back_on_failure() {
        let mut p = player_with_playlist(&["a", "b", "c"]);
        let _guard = custom_providers::test_registry();
        p.view_data_mut().selection = vec![0, 2];
        let pane = p.session.focused;
        p.remove_from_server_playlist(pane, &[1]);

        assert_eq!(titles(&p), ["Track a", "Track c"]);
        assert_eq!(count(&p), Some(2));
        assert_eq!(p.view_data().selection, [0, 1]);

        fail(&mut p);

        assert_eq!(titles(&p), ["Track a", "Track b", "Track c"]);
        assert_eq!(count(&p), Some(3));
        assert_eq!(p.view_data().selection, [0, 2]);
    }

    #[test]
    fn a_failed_reorder_restores_the_old_order() {
        let mut p = player_with_playlist(&["a", "b", "c"]);
        let _guard = custom_providers::test_registry();
        let pane = p.session.focused;
        p.view_data_mut().selection = vec![0];

        let positions = p.handle_reorder_tracks_selected(pane, 3, &[0]);
        assert_eq!(titles(&p), ["Track b", "Track c", "Track a"]);
        assert_eq!(positions, [2]);

        fail(&mut p);

        assert_eq!(titles(&p), ["Track a", "Track b", "Track c"]);
        assert_eq!(p.view_data().selection, [0]);
    }

    #[test]
    fn a_failed_rename_restores_the_name_everywhere() {
        let mut p = player_with_playlist(&["a"]);
        let _guard = custom_providers::test_registry();
        p.handle_server_playlist_rename(p.session.focused, " Renamed ");

        // The rename is optimistic: the index record and the open page show
        // the typed name at once.
        assert_eq!(p.server_playlist_name(&target()), "Renamed");
        assert_eq!(
            p.server_playlists
                .get(&target())
                .map(|entry| entry.name.clone()),
            Some("Renamed".to_string())
        );
        match &p.view_data().kind {
            ViewKind::PlaylistView(reference) => assert_eq!(reference.name, "Renamed"),
            other => panic!("expected server playlist view, got {other:?}"),
        }

        // The write waits out the debounce; answering its failure puts the
        // name back everywhere.
        let target = target();
        p.pending_server_renames
            .get_mut(&target)
            .expect("a typed rename")
            .due = std::time::Instant::now();
        p.flush_server_playlist_renames();

        fail(&mut p);

        assert_eq!(p.server_playlist_name(&target), "Server");
        assert_eq!(
            p.server_playlists
                .get(&target)
                .map(|entry| entry.name.clone()),
            Some("Server".to_string())
        );
        match &p.view_data().kind {
            ViewKind::PlaylistView(reference) => assert_eq!(reference.name, "Server"),
            other => panic!("expected server playlist view, got {other:?}"),
        }
    }

    #[test]
    fn typed_renames_debounce_into_one_server_write() {
        let mut p = player_with_playlist(&["a"]);
        let _guard = custom_providers::test_registry();
        for name in ["R", "Re", "Renamed"] {
            p.handle_server_playlist_rename(p.session.focused, name);
        }

        // Nothing went out while the typing was under way.
        assert!(p.server_playlist_writes.is_empty());
        let target = target();
        assert_eq!(p.server_playlist_name(&target), "Renamed");

        p.pending_server_renames
            .get_mut(&target)
            .expect("a typed rename")
            .due = std::time::Instant::now();
        p.flush_server_playlist_renames();

        // One write carries the name as it read when the typing stopped, and
        // its success keeps the optimistic state as is.
        assert_eq!(p.server_playlist_writes.len(), 1);
        settle(
            &mut p,
            Ok(crate::app::ServerPlaylistChange::Renamed("Renamed".into())),
        );
        assert_eq!(p.server_playlist_name(&target), "Renamed");
        assert!(p.pending_server_renames.is_empty());
    }

    #[test]
    fn deleting_a_playlist_drops_its_typed_rename() {
        let mut p = player_with_playlist(&["a"]);
        let _guard = custom_providers::test_registry();
        p.handle_server_playlist_rename(p.session.focused, "R");
        assert!(p.pending_server_renames.contains_key(&target()));

        let write_id = p.request_ids.next();
        let _ = p.apply_server_playlist_write(
            write_id,
            &target(),
            Ok(crate::app::ServerPlaylistChange::Deleted),
        );

        assert!(p.pending_server_renames.is_empty());
    }

    #[test]
    fn a_successful_write_keeps_the_optimistic_state() {
        let mut p = player_with_playlist(&["a"]);
        let _ = p.add_tracks_to_server_playlist(target(), vec![track("b")]);

        settle(
            &mut p,
            Ok(crate::app::ServerPlaylistChange::Added {
                added: 1,
                skipped: 0,
            }),
        );

        assert_eq!(titles(&p), ["Track a", "Track b"]);
        assert_eq!(count(&p), Some(2));
        assert!(p.server_playlist_writes.is_empty());
    }

    #[test]
    fn the_index_row_keeps_its_place_after_a_rollback() {
        let mut p = player_with_playlist(&["a"]);
        p.server_playlists.replace_provider(
            provider(),
            &[
                ServerPlaylist {
                    id: "first".into(),
                    name: "First".into(),
                    thumbnail: String::new(),
                    track_count: 1,
                },
                ServerPlaylist {
                    id: "pl".into(),
                    name: "Server".into(),
                    thumbnail: String::new(),
                    track_count: 1,
                },
            ],
        );
        assert_eq!(
            p.server_playlists.entries.last(),
            Some(&ServerPlaylistEntry::new(
                provider(),
                ServerPlaylist {
                    id: "pl".into(),
                    name: "Server".into(),
                    thumbnail: String::new(),
                    track_count: 1,
                },
            ))
        );

        let _ = p.add_tracks_to_server_playlist(target(), vec![track("b")]);
        fail(&mut p);

        let order: Vec<&str> = p
            .server_playlists
            .entries
            .iter()
            .map(|entry| entry.id.as_str())
            .collect();
        assert_eq!(order, ["first", "pl"]);
    }

    #[test]
    fn a_stale_index_request_lets_the_next_refresh_through() {
        let mut p = player_with_playlist(&["a"]);

        p.server_playlists_requested
            .insert(provider(), std::time::Instant::now());
        assert!(p.server_playlist_index_busy(provider()));

        let stale = std::time::Instant::now()
            .checked_sub(super::INDEX_REQUEST_TIMEOUT)
            .expect("the timeout fits in an instant");
        p.server_playlists_requested.insert(provider(), stale);
        assert!(!p.server_playlist_index_busy(provider()));
    }
}
