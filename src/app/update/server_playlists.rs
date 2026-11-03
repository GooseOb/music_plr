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
    app::{
        pane::PaneId, Dialog, ServerPlaylistChange, ServerPlaylistNameDialog, ServerPlaylistUndo,
        ServerPlaylistViewUndo, ViewKind,
    },
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

    /// Append `tracks` to a server playlist. The sidebar count and every open
    /// page update right away (no loading state); tracks without an id on that
    /// server are reported as skipped, and a failed write rolls both back.
    pub fn add_tracks_to_server_playlist(
        &mut self,
        target: ServerPlaylistId,
        tracks: Vec<Track>,
    ) -> Task<Message> {
        if tracks.is_empty() {
            return Task::none();
        }
        let provider = target.provider;
        // Only the tracks that carry an id on this server can land, so those
        // are what the pages show right away; the provider repeats the same
        // filter and reports the rest as skipped once the write lands.
        let added: Vec<Track> = tracks
            .iter()
            .filter(|track| track.provider_id(provider).is_some_and(|id| !id.is_empty()))
            .cloned()
            .collect();
        if added.is_empty() {
            let message = (self.strings.skipped_not_on_server)(tracks.len());
            self.notify_error(message);
            return Task::none();
        }
        self.begin_playlist_write(&target);
        self.apply_added_tracks(&target, &added);
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
    /// puts the order it had back.
    pub fn reorder_server_playlist(&mut self, target: ServerPlaylistId, tracks: Vec<Track>) {
        let provider = target.provider;
        let playlist_id = target.id.clone();
        self.begin_playlist_write(&target);
        self.spawn_playlist_write(target, move || {
            providers::reorder_playlist(provider, &playlist_id, &tracks)?;
            Ok(ServerPlaylistChange::Reordered)
        });
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
        if !providers::playlists_supported(provider) {
            return;
        }
        let target = ServerPlaylistId {
            provider,
            id: reference.id.clone(),
        };
        self.begin_playlist_write(&target);
        self.apply_removed_rows(&target, indices);
        let playlist_id = target.id.clone();
        let positions = indices.to_vec();
        self.spawn_playlist_write(target, move || {
            let removed = providers::remove_from_playlist(provider, &playlist_id, &positions)?;
            Ok(ServerPlaylistChange::Removed(removed))
        });
    }

    /// Append `tracks` to the cached count and to every open page showing the
    /// playlist, as if the server had already stored them.
    fn apply_added_tracks(&mut self, target: &ServerPlaylistId, tracks: &[Track]) {
        self.server_playlists
            .adjust_count(target, tracks.len().cast_signed());
        Self::seed_tracks_thumbnails(&mut self.thumbnail_index, tracks);
        for pane in self.panes_showing(target) {
            let Some(list) = self.view_data_in_mut(pane).tracks_mut() else {
                continue;
            };
            list.extend_from_slice(tracks);
        }
    }

    /// Drop `indices` from the cached count and from every open page showing
    /// the playlist, remapping each page's selection.
    fn apply_removed_rows(&mut self, target: &ServerPlaylistId, indices: &[usize]) {
        self.server_playlists
            .adjust_count(target, -(indices.len().cast_signed()));
        for pane in self.panes_showing(target) {
            let view = self.view_data_in_mut(pane);
            let Some(list) = view.tracks_mut() else {
                continue;
            };
            crate::util::remove_at(list, indices);
            crate::util::remap_removed_indices(&mut view.selection, indices);
        }
    }

    fn spawn_playlist_write<F>(&mut self, target: ServerPlaylistId, run: F)
    where
        F: FnOnce() -> anyhow::Result<ServerPlaylistChange> + Send + 'static,
    {
        *self
            .server_playlist_writes
            .entry(target.clone())
            .or_insert(0) += 1;
        let tx = self.result_tx.clone();
        std::thread::spawn(move || {
            let result = run().map_err(ProviderError::from);
            let _ = tx.send(BackendResult::ServerPlaylistWritten { target, result });
        });
    }

    /// Remember what `target` looks like before an optimistic write touches it,
    /// so the write's failure can put it back. Only the first write of a burst
    /// records it: the state to return to is the one before any of them.
    pub(crate) fn begin_playlist_write(&mut self, target: &ServerPlaylistId) {
        if self.server_playlist_undo.contains_key(target) {
            return;
        }
        let entry = self.server_playlists.get(target).cloned();
        let views = self
            .panes_showing(target)
            .into_iter()
            .filter_map(|pane| {
                let view = self.view_data_in(pane);
                let LoadState::Ready(tracks) = &view.content else {
                    return None;
                };
                Some(ServerPlaylistViewUndo {
                    pane,
                    tracks: tracks.clone(),
                    selection: view.selection.clone(),
                })
            })
            .collect();
        self.server_playlist_undo
            .insert(target.clone(), ServerPlaylistUndo { entry, views });
    }

    /// Put the pre-write state back after a failed write: the index row (name
    /// and count, so the sidebar agrees) and the tracks and selection of every
    /// page that was showing the playlist.
    fn rollback_playlist_write(&mut self, target: &ServerPlaylistId) {
        let Some(undo) = self.server_playlist_undo.remove(target) else {
            return;
        };
        match undo.entry {
            Some(entry) => {
                let name = entry.name.clone();
                self.server_playlists.restore(entry);
                self.rename_server_playlist_views(target, &name);
            }
            None => {
                self.server_playlists.remove(target);
            }
        }
        for view in undo.views {
            if !shows_playlist(self.view_data_in(view.pane), target) {
                continue;
            }
            let data = self.view_data_in_mut(view.pane);
            data.content = LoadState::Ready(view.tracks);
            data.selection = view.selection;
        }
    }

    fn panes_showing(&self, target: &ServerPlaylistId) -> Vec<PaneId> {
        self.pane_ids()
            .into_iter()
            .filter(|pane| shows_playlist(self.view_data_in(*pane), target))
            .collect()
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
            // The new name shows up in the sidebar and the page headers at
            // once; a rejected rename puts the old one back.
            self.begin_playlist_write(&target);
            self.server_playlists.rename(&target, &name);
            self.rename_server_playlist_views(&target, &name);
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

    /// Apply a finished write. Every write touches the index and the open pages
    /// as it starts, so success only reports it and re-syncs the count; failure
    /// rolls that optimistic state back.
    pub(crate) fn apply_server_playlist_write(
        &mut self,
        target: &ServerPlaylistId,
        result: Result<ServerPlaylistChange, ProviderError>,
    ) -> Task<Message> {
        if let Some(pending) = self.server_playlist_writes.get_mut(target) {
            *pending -= 1;
            if *pending == 0 {
                self.server_playlist_writes.remove(target);
            }
        }
        let change = match result {
            Ok(change) => change,
            Err(error) => {
                let message = self.strings.error_reason(&error).to_string();
                self.notify_error(message);
                self.rollback_playlist_write(target);
                return Task::none();
            }
        };
        // Only drop the rollback state once the playlist has no write left in
        // flight; an earlier success must not disable a later failure's undo.
        if !self.server_playlist_writes.contains_key(target) {
            self.server_playlist_undo.remove(target);
        }
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

#[cfg(test)]
mod tests {
    use crate::{
        app::{MusicPlayer, ViewData, ViewKind},
        data::{
            config,
            custom_providers::CustomProviderKind,
            server_playlists::{ServerPlaylistEntry, ServerPlaylistId},
        },
        providers::{ErrorKind, ProviderError, ServerPlaylist},
        types::Track,
    };

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

    /// A player whose single pane shows a server playlist holding `ids`.
    fn player_with_playlist(ids: &[&str]) -> MusicPlayer {
        let mut p = MusicPlayer::new_with(config::Config::default());
        p.server_playlists.entries.clear();
        p.server_playlists.replace_provider(
            provider(),
            &[ServerPlaylist {
                id: "pl".into(),
                name: "Server".into(),
                track_count: ids.len(),
            }],
        );
        let mut view = ViewData {
            kind: ViewKind::PlaylistView(crate::app::view_data::PlaylistRef {
                id: "pl".into(),
                name: "Server".into(),
                provider: provider(),
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
        p.server_playlists
            .get(&target())
            .map(|entry| entry.track_count)
    }

    fn fail(p: &mut MusicPlayer) {
        let error = ProviderError::new(ErrorKind::Unavailable, "gone");
        let _ = p.apply_server_playlist_write(&target(), Err(error));
    }

    #[test]
    fn added_tracks_show_up_before_the_write_lands() {
        let mut p = player_with_playlist(&["a"]);
        let _ = p.add_tracks_to_server_playlist(target(), vec![track("b")]);

        assert_eq!(titles(&p), ["Track a", "Track b"]);
        assert_eq!(count(&p), Some(2));
        // The optimistic state is remembered for a possible rollback.
        assert!(p.server_playlist_undo.contains_key(&target()));
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
        assert!(!p.server_playlist_undo.contains_key(&target()));
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
        p.view_data_mut().selection = vec![0, 2];
        let pane = p.focused_pane_id;
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
        let pane = p.focused_pane_id;
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
        p.dialog = Some(crate::app::Dialog::ServerPlaylistName(
            crate::app::ServerPlaylistNameDialog::rename(&target(), "Server"),
        ));
        if let Some(crate::app::Dialog::ServerPlaylistName(dialog)) = &mut p.dialog {
            dialog.name = "Renamed".into();
        }
        let _ = p.submit_server_playlist_name();

        assert_eq!(p.server_playlist_name(&target()), "Renamed");
        assert_eq!(
            p.server_playlists
                .get(&target())
                .map(|entry| entry.name.clone()),
            Some("Renamed".to_string())
        );

        fail(&mut p);

        assert_eq!(p.server_playlist_name(&target()), "Server");
        assert_eq!(
            p.server_playlists
                .get(&target())
                .map(|entry| entry.name.clone()),
            Some("Server".to_string())
        );
    }

    #[test]
    fn a_successful_write_keeps_the_optimistic_state() {
        let mut p = player_with_playlist(&["a"]);
        let _ = p.add_tracks_to_server_playlist(target(), vec![track("b")]);

        let _ = p.apply_server_playlist_write(
            &target(),
            Ok(crate::app::ServerPlaylistChange::Added {
                added: 1,
                skipped: 0,
            }),
        );

        assert_eq!(titles(&p), ["Track a", "Track b"]);
        assert_eq!(count(&p), Some(2));
        assert!(p.server_playlist_undo.is_empty());
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
                    track_count: 1,
                },
                ServerPlaylist {
                    id: "pl".into(),
                    name: "Server".into(),
                    track_count: 1,
                },
            ],
        );
        assert_eq!(
            p.server_playlists.entries.last(),
            Some(&ServerPlaylistEntry {
                provider: provider(),
                id: "pl".into(),
                name: "Server".into(),
                track_count: 1,
            })
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
}
