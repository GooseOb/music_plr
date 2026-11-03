//! Server-side playlists: the sidebar index, opening one, and the writes that
//! go back to the server (add, remove, create, rename, delete).
//!
//! The index ([`ServerPlaylistStore`]) holds identity only; contents always
//! come from the provider's `browse` path, so a playlist edited elsewhere shows
//! the server's state. Every write runs on a background thread and reports
//! back through [`BackendResult::ServerPlaylistWritten`], after which the open
//! view is refetched — the server stays the source of truth.

use iced::Task;

use super::{BackendResult, Message, MusicPlayer, TrackPos, ViewData};
use crate::{
    app::{pane::PaneId, Dialog, ServerPlaylistChange, ServerPlaylistNameDialog, ViewKind},
    data::server_playlists::{ServerPlaylistEntry, ServerPlaylistId},
    load_state::LoadState,
    providers::{self, ProviderError, ProviderId, ServerPlaylist},
    types::Track,
};

impl MusicPlayer {
    /// Every configured server that has playlists, in provider order so the
    /// sidebar groups and the palette rows agree on what "row 3" means.
    pub fn server_playlist_providers() -> Vec<ProviderId> {
        ProviderId::all()
            .into_iter()
            .filter(|provider| {
                providers::playlists_supported(*provider) && provider.capabilities().search
            })
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

    pub fn server_playlist_name(&self, target: &ServerPlaylistId) -> String {
        self.server_playlists
            .get(target)
            .map(|entry| entry.name.clone())
            .unwrap_or_default()
    }

    /// Reload one server's playlist index, or every configured server's.
    /// A provider with a request already in flight is skipped, so the boot
    /// refresh and a manual one share a single call.
    pub fn refresh_server_playlists(&mut self, provider: Option<ProviderId>) -> Task<Message> {
        let providers: Vec<ProviderId> = match provider {
            Some(provider) => vec![provider],
            None => Self::server_playlist_providers(),
        };
        for provider in providers {
            if !self.server_playlists_requested.insert(provider) {
                continue;
            }
            let tx = self.result_tx.clone();
            std::thread::spawn(move || {
                let result = providers::list_playlists(provider).map_err(ProviderError::from);
                let _ = tx.send(BackendResult::ServerPlaylists { provider, result });
            });
        }
        Task::none()
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
        let kind = ViewKind::PlaylistView(crate::app::view_data::PlaylistRef {
            id: target.id,
            name: name.to_string(),
            provider,
            thumbnail: String::new(),
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
        if !providers::playlists_supported(provider) {
            return Task::none();
        }
        let id = reference.id.clone();
        let rid = self.request_ids.next();
        let view = self.view_data_in_mut(pane);
        view.content = LoadState::Loading;
        view.request_id = rid;
        let tx = self.result_tx.clone();
        std::thread::spawn(move || match providers::browse(provider, &id, "playlist") {
            Ok((tracks, meta)) => {
                let _ = tx.send(BackendResult::BrowseResults(rid, tracks, meta));
            }
            Err(error) => {
                let _ = tx.send(BackendResult::SearchError(rid, error.into()));
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

    /// Append `tracks` to a server playlist. Tracks without an id on that
    /// server are reported as skipped once the write lands.
    pub fn add_tracks_to_server_playlist(
        &mut self,
        target: ServerPlaylistId,
        tracks: Vec<Track>,
    ) -> Task<Message> {
        if tracks.is_empty() {
            return Task::none();
        }
        let provider = target.provider;
        let playlist_id = target.id.clone();
        self.spawn_playlist_write(target, move || {
            let write = providers::add_to_playlist(provider, &playlist_id, &tracks)?;
            Ok(ServerPlaylistChange::Added {
                added: write.added,
                skipped: write.skipped,
            })
        });
        Task::none()
    }

    /// Rewrite a server playlist's track order. The caller has already
    /// reordered the rows it shows, so a success needs no refetch; a failure
    /// refetches instead, restoring the server's order.
    pub fn reorder_server_playlist(&mut self, target: ServerPlaylistId, tracks: Vec<Track>) {
        let provider = target.provider;
        let playlist_id = target.id.clone();
        self.spawn_playlist_write(target, move || {
            providers::reorder_playlist(provider, &playlist_id, &tracks)?;
            Ok(ServerPlaylistChange::Reordered)
        });
    }

    /// Remove the rows at `indices` from the server playlist the pane shows.
    pub fn remove_from_server_playlist(&mut self, pane: PaneId, indices: &[usize]) {
        let ViewKind::PlaylistView(reference) = &self.view_data_in(pane).kind else {
            return;
        };
        let provider = reference.provider;
        if !providers::playlists_supported(provider) {
            return;
        }
        let target = ServerPlaylistId {
            provider,
            id: reference.id.clone(),
        };
        let playlist_id = target.id.clone();
        let positions = indices.to_vec();
        self.clear_selection_if_touched_in(pane, indices);
        self.spawn_playlist_write(target, move || {
            let removed = providers::remove_from_playlist(provider, &playlist_id, &positions)?;
            Ok(ServerPlaylistChange::Removed(removed))
        });
    }

    fn spawn_playlist_write<F>(&mut self, target: ServerPlaylistId, run: F)
    where
        F: FnOnce() -> anyhow::Result<ServerPlaylistChange> + Send + 'static,
    {
        self.server_playlist_writes.insert(target.clone());
        let tx = self.result_tx.clone();
        std::thread::spawn(move || {
            let result = run().map_err(ProviderError::from);
            let _ = tx.send(BackendResult::ServerPlaylistWritten { target, result });
        });
    }

    pub fn open_new_server_playlist(&mut self, provider: ProviderId) {
        self.dialog = Some(Dialog::ServerPlaylistName(
            ServerPlaylistNameDialog::new_playlist(provider),
        ));
    }

    pub fn open_rename_server_playlist(&mut self, target: &ServerPlaylistId, name: &str) {
        self.dialog = Some(Dialog::ServerPlaylistName(
            ServerPlaylistNameDialog::rename(target, name),
        ));
    }

    pub fn open_delete_server_playlist(&mut self, target: ServerPlaylistId, name: &str) {
        self.dialog = Some(Dialog::ServerPlaylistDelete(
            crate::app::ServerPlaylistDeleteDialog {
                target,
                name: name.to_string(),
            },
        ));
    }

    /// Create the playlist, or rename the one the dialog was opened for.
    pub fn submit_server_playlist_name(&mut self) -> Task<Message> {
        let Some(Dialog::ServerPlaylistName(dialog)) = &self.dialog else {
            return Task::none();
        };
        let name = dialog.name.trim().to_string();
        if name.is_empty() {
            return Task::none();
        }
        let provider = dialog.provider;
        let target = dialog.target();
        self.dialog = None;
        if let Some(target) = target {
            let playlist_id = target.id.clone();
            self.spawn_playlist_write(target, move || {
                providers::rename_playlist(provider, &playlist_id, &name)?;
                Ok(ServerPlaylistChange::Renamed(name))
            });
        } else {
            let tx = self.result_tx.clone();
            std::thread::spawn(move || {
                let result = providers::create_playlist(provider, &name, &[])
                    .map(|created| ServerPlaylistChange::Created {
                        id: created.id,
                        name: created.name,
                    })
                    .map_err(ProviderError::from);
                let _ = tx.send(BackendResult::ServerPlaylistWritten {
                    target: ServerPlaylistId {
                        provider,
                        id: String::new(),
                    },
                    result,
                });
            });
        }
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
        self.spawn_playlist_write(target, move || {
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
    ) {
        self.server_playlists_requested.remove(&provider);
        match result {
            Ok(playlists) => self.server_playlists.replace_provider(provider, &playlists),
            Err(error) => {
                let message = self.strings.error_reason(&error).to_string();
                self.notify_error(message);
            }
        }
    }

    /// Apply a finished write: update the index, report it, and refetch the
    /// open views of that playlist so they show the server's truth.
    pub(crate) fn apply_server_playlist_write(
        &mut self,
        target: &ServerPlaylistId,
        result: Result<ServerPlaylistChange, ProviderError>,
    ) -> Task<Message> {
        self.server_playlist_writes.remove(target);
        let change = match result {
            Ok(change) => change,
            Err(error) => {
                let message = self.strings.error_reason(&error).to_string();
                self.notify_error(message);
                // A write can fail halfway (a partial reorder, say), so put the
                // open pages back on the server's truth instead of the guess.
                return self.refetch_server_playlist_views(target);
            }
        };
        let name = self.server_playlist_name(target);
        match change {
            ServerPlaylistChange::Added { added, skipped } => {
                self.server_playlists
                    .adjust_count(target, added.cast_signed());
                self.notify((self.strings.added_to)(added, &name));
                if skipped > 0 {
                    let message = (self.strings.skipped_not_on_server)(skipped);
                    self.notify_error(message);
                }
                self.refetch_server_playlist_views(target)
            }
            ServerPlaylistChange::Removed(removed) => {
                self.server_playlists
                    .adjust_count(target, -removed.cast_signed());
                self.notify((self.strings.removed_n)(removed));
                self.refetch_server_playlist_views(target)
            }
            ServerPlaylistChange::Reordered => {
                self.notify(self.strings.reordered_playlist);
                Task::none()
            }
            ServerPlaylistChange::Created { id, name } => {
                self.server_playlists.insert(ServerPlaylistEntry {
                    provider: target.provider,
                    id,
                    name: name.clone(),
                    track_count: 0,
                });
                self.notify((self.strings.playlist_created)(&name));
                Task::none()
            }
            ServerPlaylistChange::Renamed(new_name) => {
                self.server_playlists.rename(target, &new_name);
                self.rename_server_playlist_views(target, &new_name);
                Task::none()
            }
            ServerPlaylistChange::Deleted => {
                self.server_playlists.remove(target);
                self.close_server_playlist_views(target)
            }
        }
    }

    fn refetch_server_playlist_views(&mut self, target: &ServerPlaylistId) -> Task<Message> {
        let panes: Vec<PaneId> = self
            .pane_ids()
            .into_iter()
            .filter(|pane| shows_playlist(self.view_data_in(*pane), target))
            .collect();
        Task::batch(
            panes
                .into_iter()
                .map(|pane| self.refresh_open_server_playlist(pane)),
        )
    }

    fn rename_server_playlist_views(&mut self, target: &ServerPlaylistId, name: &str) {
        for pane in self.pane_ids() {
            for view in &mut self.pane_mut(pane).nav_history {
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
        let panes: Vec<PaneId> = self
            .pane_ids()
            .into_iter()
            .filter(|pane| shows_playlist(self.view_data_in(*pane), target))
            .collect();
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
