use std::{
    fs,
    path::{Path, PathBuf},
};

use iced::widget::operation;

use super::{Message, MusicPlayer, Task, Track, ViewData, PREPEND};
use crate::{
    app::{pane::PaneId, Dialog, ImportMethod, ImportPlaylistDialog, PlaylistJump, ViewKind},
    data::JsonStore,
};

impl MusicPlayer {
    pub(crate) fn navigate_to_playlist(&mut self, index: usize) -> Task<Message> {
        let pane = self.focused_pane_id;
        self.pane_mut(pane).lyrics = None;
        self.clear_selection();
        self.drag.cleanup();
        let playlist_name = self.playlists.playlists[index].name.clone();
        let task = self.push_new_view(pane, ViewData::new_playlist(index, playlist_name));
        let view = self.view_data_in(pane).clone();
        self.seed_view_thumbnails(&view);
        self.save_session();
        task
    }

    fn finish_import(&mut self, name: &str, tracks: &[Track]) -> (bool, Task<Message>) {
        if tracks.is_empty() {
            self.notify(self.strings.import_no_tracks);
            return (false, Task::none());
        }
        let idx = self
            .playlists
            .create_at(name, self.playlists.playlists.len());
        self.playlists.insert_tracks_at(idx, tracks.iter(), PREPEND);
        let label = self.playlists.playlists[idx].name.clone();
        self.notify((self.strings.import_imported_into)(tracks.len(), &label));
        let task = self.open_imported_playlist(idx);
        (true, task)
    }

    pub fn handle_create_playlist(&mut self) {
        if self.playlist_create_name.trim().is_empty() {
            return;
        }
        let name = self.playlist_create_name.trim().to_string();
        self.playlists.create(&name);
        self.playlist_create_name.clear();
        let msg = (self.strings.playlist_created)(&name);
        self.notify(msg);
    }

    pub fn cycle_playlist(&mut self, dir: isize) -> Task<Message> {
        if self.playlists.playlists.is_empty() {
            self.notify(self.strings.nothing_here);
            return Task::none();
        }
        let len = self.playlists.playlists.len().cast_signed();
        let cur = match &self.view_data().kind {
            ViewKind::Playlist(p) => p.index.cast_signed(),
            _ => {
                if dir < 0 {
                    0
                } else {
                    -1
                }
            }
        };
        let next = ((cur + dir).rem_euclid(len)) as usize;
        self.handle_select_playlist(next)
    }

    pub fn open_playlist_jump(&mut self) -> Task<Message> {
        self.dialog = Some(Dialog::PlaylistJump(PlaylistJump::default()));
        operation::focus::<Message>(crate::app::ui::playlist_jump_input_id())
    }

    pub(crate) fn playlist_jump_filtered(&self) -> Vec<usize> {
        let names: Vec<String> = self
            .playlists
            .playlists
            .iter()
            .map(|p| p.name.clone())
            .collect();
        match &self.dialog {
            Some(Dialog::PlaylistJump(jump)) => jump.filtered(&names),
            _ => Vec::new(),
        }
    }

    pub fn step_playlist_jump(&mut self, dir: isize) -> Task<Message> {
        let filtered = self.playlist_jump_filtered();
        if filtered.is_empty() {
            return Task::none();
        }
        if let Some(Dialog::PlaylistJump(jump)) = &mut self.dialog {
            jump.selected = ((jump.selected.cast_signed() + dir)
                .rem_euclid(filtered.len().cast_signed()))
            .cast_unsigned();
        }
        Task::none()
    }

    pub fn confirm_playlist_jump(&mut self, play: bool) -> Task<Message> {
        let filtered = self.playlist_jump_filtered();
        let selected = match &self.dialog {
            Some(Dialog::PlaylistJump(jump)) => jump.selected,
            _ => return Task::none(),
        };
        let Some(&index) = filtered.get(selected) else {
            return Task::none();
        };
        self.dialog = None;
        if play {
            self.handle_open_and_play_playlist(index)
        } else {
            self.handle_select_playlist(index)
        }
    }

    pub fn handle_select_playlist(&mut self, index: usize) -> Task<Message> {
        let already_selected =
            matches!(&self.view_data().kind, ViewKind::Playlist(p) if p.index == index);
        if index < self.playlists.playlists.len() && !already_selected {
            self.dialog = None;
            return self.navigate_to_playlist(index);
        }
        Task::none()
    }

    pub fn handle_open_and_play_playlist(&mut self, index: usize) -> Task<Message> {
        let task = self.handle_select_playlist(index);
        if let Some(playlist) = self.playlists.playlists.get(index) {
            if playlist.tracks.is_empty() {
                return task;
            }
            let tracks = playlist.tracks.clone();
            self.set_queue(tracks);
            self.record_now_playing_origin();
        }
        task
    }

    pub fn handle_rename_playlist(&mut self, new_name: &str) {
        let idx = match &self.view_data().kind {
            ViewKind::Playlist(entry) if !new_name.trim().is_empty() => entry.index,
            _ => return,
        };
        self.playlists.playlists[idx].name = new_name.trim().to_string();
        self.playlists.save();
        for pane in self.pane_ids() {
            if let ViewKind::Playlist(entry) = &mut self.view_data_in_mut(pane).kind {
                if entry.index == idx {
                    entry.name = new_name.trim().to_string();
                }
            }
        }
    }

    pub fn handle_delete_playlist(&mut self, index: usize) -> Task<Message> {
        self.playlists.delete(index);

        // Every pane viewing a playlist must stay valid: the deleted one
        // moves to an adjacent selection (or a safe view when none remain),
        // and views below it shift down by one. `push_new_view` focuses its
        // pane, so restore the previous focus afterwards.
        let prev_focus = self.focused_pane_id;
        let mut nav_task = Task::none();
        for pane in self.pane_ids() {
            let action = match &self.view_data_in(pane).kind {
                ViewKind::Playlist(entry) if entry.index == index => {
                    if self.playlists.playlists.is_empty() {
                        let provider = self.pane(pane).search_provider;
                        let scope = self.pane(pane).search_scope;
                        Some((
                            true,
                            0,
                            ViewData::new_search(String::new(), provider, scope),
                        ))
                    } else {
                        Some((
                            false,
                            index.min(self.playlists.playlists.len() - 1),
                            ViewData::new_playlist(0, String::new()),
                        ))
                    }
                }
                ViewKind::Playlist(entry) if entry.index > index => Some((
                    false,
                    entry.index - 1,
                    ViewData::new_playlist(0, String::new()),
                )),
                _ => None,
            };
            let Some((navigate_away, new_idx, blank)) = action else {
                continue;
            };
            if navigate_away {
                nav_task = nav_task.chain(self.push_new_view(pane, blank));
            } else {
                let new_name = self.playlists.playlists[new_idx].name.clone();
                if let ViewKind::Playlist(entry) = &mut self.view_data_in_mut(pane).kind {
                    entry.index = new_idx;
                    entry.name = new_name;
                }
                nav_task = nav_task.chain(self.capture_bounds_task());
            }
        }
        if self.panes.contains_key(&prev_focus) {
            self.focused_pane_id = prev_focus;
        }

        self.dialog = None;
        nav_task
    }

    pub fn handle_add_local_music(&mut self, paths: &[PathBuf]) {
        let mut new_tracks = Vec::new();

        for path in paths {
            let path_str = path.to_string_lossy().to_string();
            if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
                let duration = crate::util::try_probe_duration(&path_str).unwrap_or(0);
                let mut providers = std::collections::HashMap::new();
                providers.insert(
                    crate::providers::ProviderId::Local,
                    crate::types::ProviderTrack {
                        id: filename.to_string(),
                        url: path_str.clone(),
                        artist_id: None,
                        duration,
                        thumbnail: String::new(),
                        album: None,
                        play_count: 0,
                    },
                );
                new_tracks.push(Track {
                    title: filename.to_string(),
                    artist: "Unknown Artist".to_string(),
                    source: crate::providers::ProviderId::Local,
                    providers,
                });
            }
        }

        let active = match &self.view_data().kind {
            ViewKind::Playlist(p) => Some(p.index),
            _ => None,
        };
        let Some(idx) = active else {
            let msg = (self.strings.added_local)(new_tracks.len());
            self.notify(msg);
            return;
        };

        let count = self
            .playlists
            .insert_tracks_at(idx, new_tracks.iter(), PREPEND);
        let msg = (self.strings.added)(count);
        self.notify(msg);
    }

    pub fn handle_add_to_playlist(
        &mut self,
        pane: PaneId,
        playlist_idx: usize,
        indices: &[usize],
        list: super::TrackListKind,
    ) {
        if playlist_idx >= self.playlists.playlists.len() {
            return;
        }

        let tracks: Vec<Track> = indices
            .iter()
            .filter_map(|&i| self.get_track_at(super::TrackPos::new(i, list, pane)))
            .collect();
        let count = self
            .playlists
            .insert_tracks_at(playlist_idx, tracks.iter(), PREPEND);
        self.dialog = None;
        let name = self.playlists.playlists[playlist_idx].name.clone();
        let msg = (self.strings.added_to)(count, &name);
        self.notify(msg);
    }

    pub fn handle_remove_from_playlist_batch(&mut self, pane: PaneId, indices: &[usize]) {
        let ViewKind::Playlist(p) = &self.view_data_in(pane).kind else {
            return;
        };
        let removed = self.playlists.remove_tracks_at(p.index, indices);
        let msg = (self.strings.removed_n)(removed);
        self.notify(msg);
        self.clear_selection_if_touched_in(pane, indices, super::TrackListKind::Active);
    }

    pub fn handle_reorder_tracks_selected(
        &mut self,
        pane: PaneId,
        drop_idx: usize,
        indices: &[usize],
        selection: &[usize],
    ) -> Vec<usize> {
        let sp = match &self.view_data_in(pane).kind {
            ViewKind::Playlist(p) => p.index,
            _ => return Vec::new(),
        };
        let new_positions = if sp < self.playlists.playlists.len() {
            crate::util::reorder_tracks(
                &mut self.playlists.playlists[sp].tracks,
                drop_idx,
                indices,
                selection,
            )
        } else {
            Vec::new()
        };
        self.playlists.save();
        new_positions
    }

    pub fn handle_copy_selected(&mut self) {
        let pane = self.focused_pane_id;
        self.clipboard.clear();
        let selection: Vec<usize> = self.view_data_mut().selection.clone();
        for &i in &selection {
            if let Some(track) =
                self.get_track_at(super::TrackPos::new(i, super::TrackListKind::Active, pane))
            {
                self.clipboard.push(track);
            }
        }
    }

    pub fn handle_paste_clipboard(&mut self) -> Task<Message> {
        if self.clipboard.is_empty() {
            return Task::none();
        }
        let active = match &self.view_data().kind {
            ViewKind::Playlist(p) => Some(p.index),
            _ => None,
        };
        let Some(idx) = active else {
            return Task::none();
        };
        self.playlists
            .insert_tracks_at(idx, self.clipboard.iter(), PREPEND);
        self.playlists.save();
        let count = self.clipboard.len();
        let name = self.playlists.playlists[idx].name.clone();
        let msg = (self.strings.pasted_into)(count, &name);
        self.notify(msg);
        self.clipboard.clear();
        self.capture_bounds_task()
    }

    pub fn handle_delete_selected(&mut self) {
        if self.view_data_mut().selection.is_empty() {
            return;
        }
        let indices: Vec<usize> = self.view_data_mut().selection.clone();

        if matches!(self.view_data_mut().kind, ViewKind::Playlist(_)) {
            self.handle_remove_from_playlist_batch(self.focused_pane_id, &indices);
        } else if let ViewKind::Downloads = &self.view_data().kind {
            if let Some(tracks) = self.view_data_mut().tracks_mut() {
                let removed_urls: Vec<String> = indices
                    .iter()
                    .filter_map(|&i| tracks.get(i).map(|t| t.primary_url().to_string()))
                    .collect();
                let removed = crate::util::remove_at(tracks, &indices);
                let tr = self.strings;
                let msg = (tr.removed_from)(removed, tr.downloads);
                self.notify(msg);
                for url in removed_urls {
                    self.download_registry.remove(&url);
                }
            }
        }
        self.clear_selection_for(super::TrackListKind::Active);
    }

    pub fn handle_delete_in_hovered_list(&mut self) {
        let pane = self.focused_pane_id;
        let hovered = self.focused_hovered_track();
        let list = hovered.map_or(super::TrackListKind::Active, |h| h.list);
        let indices: Vec<usize> = {
            let sel = self.selection_in(pane, list);
            if !sel.is_empty() {
                sel.to_vec()
            } else if let Some(h) = hovered.filter(|h| h.list == list) {
                vec![h.index]
            } else {
                return;
            }
        };
        match list {
            super::TrackListKind::Queue => {
                self.handle_remove_from_queue_batch(&indices);
            }
            super::TrackListKind::Recent => {
                self.handle_remove_from_recent_batch(&indices);
            }
            super::TrackListKind::Active => {
                if !matches!(
                    self.view_data_in(pane).kind,
                    ViewKind::Playlist(_) | ViewKind::Downloads
                ) {
                    return;
                }
                if self
                    .selection_in(pane, super::TrackListKind::Active)
                    .is_empty()
                {
                    self.view_data_mut().selection = indices;
                }
                self.handle_delete_selected();
            }
        }
    }

    /// Open the file/folder picker for the current import method. The picked
    /// path is delivered back through `BackendResult::ImportPathsPicked`.
    pub fn handle_import_pick(&mut self) {
        let Some(Dialog::Import(dialog)) = &self.dialog else {
            return;
        };
        let method = dialog.method;
        let tx = self.result_tx.clone();
        std::thread::spawn(move || {
            let paths = match method {
                ImportMethod::Native => rfd::FileDialog::new()
                    .add_filter("Playlists", &["json"])
                    .pick_file()
                    .map(|p| vec![p]),
                ImportMethod::Csv => rfd::FileDialog::new()
                    .add_filter("CSV", &["csv"])
                    .pick_file()
                    .map(|p| vec![p]),
                ImportMethod::FileList => rfd::FileDialog::new().pick_folder().map(|p| vec![p]),
            };
            if let Some(paths) = paths.filter(|p| !p.is_empty()) {
                let _ = tx
                    .send(crate::app::message::BackendResult::ImportPathsPicked { method, paths });
            }
        });
    }

    /// Apply an import once the user has picked a source. Returns whether the
    /// dialog should close (true on success, false on a readable error so the
    /// user can correct and retry).
    pub fn handle_import_paths(
        &mut self,
        method: ImportMethod,
        paths: &[PathBuf],
    ) -> Task<Message> {
        let dialog = self.dialog.take();
        let Some(Dialog::Import(import)) = dialog else {
            self.dialog = dialog;
            return Task::none();
        };
        let (ok, task) = match method {
            ImportMethod::Native => self.import_native(&paths[0]),
            ImportMethod::Csv => self.import_csv(&paths[0], &import),
            ImportMethod::FileList => self.import_file_list(&paths[0], &import),
        };
        if !ok {
            self.dialog = Some(Dialog::Import(import));
        }
        task
    }

    fn import_native(&mut self, path: &Path) -> (bool, Task<Message>) {
        let content = match fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                self.notify_error(format!("{}: {e}", self.strings.import_bad_file));
                return (false, Task::none());
            }
        };
        let imported: crate::data::playlists::PlaylistStore = match serde_json::from_str(&content) {
            Ok(s) => s,
            Err(e) => {
                self.notify_error(format!("{}: {e}", self.strings.import_bad_file));
                return (false, Task::none());
            }
        };
        if imported.playlists.is_empty() {
            self.notify(self.strings.import_no_tracks);
            return (false, Task::none());
        }
        let count = imported.playlists.len();
        for pl in imported.playlists {
            let idx = self
                .playlists
                .create_at(&pl.name, self.playlists.playlists.len());
            self.playlists.playlists[idx].tracks = pl.tracks;
        }
        self.playlists.save();
        self.notify((self.strings.import_playlists_imported)(count));
        (true, Task::none())
    }

    fn import_csv(&mut self, path: &Path, dialog: &ImportPlaylistDialog) -> (bool, Task<Message>) {
        let mut rdr = match csv::Reader::from_path(path) {
            Ok(r) => r,
            Err(e) => {
                self.notify_error(format!("{}: {e}", self.strings.import_bad_file));
                return (false, Task::none());
            }
        };
        let headers: Vec<String> = match rdr.headers() {
            Ok(h) => h.iter().map(|s| s.trim().to_lowercase()).collect(),
            Err(e) => {
                self.notify_error(format!("{}: {e}", self.strings.import_bad_file));
                return (false, Task::none());
            }
        };
        let col_index = |name: &str| -> Option<usize> {
            if name.trim().is_empty() {
                return None;
            }
            let n = name.trim().to_lowercase();
            headers.iter().position(|h| h == &n)
        };
        let name_i = col_index(&dialog.csv_name_col);
        let artist_i = col_index(&dialog.csv_artist_col);
        let album_i = col_index(&dialog.csv_album_col);
        let mut tracks = Vec::new();
        for rec in rdr.records() {
            let Ok(rec) = rec else { continue };
            let get = |i: Option<usize>| -> String {
                i.and_then(|i| rec.get(i)).unwrap_or("").trim().to_string()
            };
            let title = get(name_i);
            let artist = get(artist_i);
            let album = get(album_i);
            if title.is_empty() && artist.is_empty() && album.is_empty() {
                continue;
            }
            tracks.push(crate::app::import::build_reference_track(
                title, artist, album,
            ));
        }
        let name = Self::import_playlist_name(dialog, path.file_stem().and_then(|s| s.to_str()));
        self.finish_import(&name, &tracks)
    }

    fn import_file_list(
        &mut self,
        dir: &Path,
        dialog: &ImportPlaylistDialog,
    ) -> (bool, Task<Message>) {
        let mut files = Vec::new();
        crate::app::import::gather_audio_files(dir, &mut files);
        if files.is_empty() {
            self.notify(self.strings.import_no_tracks);
            return (false, Task::none());
        }
        let mut tracks = Vec::new();
        for file in &files {
            let filename = file.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if let Some((name, artist, album)) =
                crate::app::import::parse_filename(&dialog.patterns, filename)
            {
                tracks.push(crate::app::import::build_file_track(
                    file, name, artist, album,
                ));
            }
        }
        if tracks.is_empty() {
            self.notify(self.strings.import_no_match);
            return (false, Task::none());
        }
        let name = Self::import_playlist_name(dialog, dir.file_name().and_then(|s| s.to_str()));
        self.finish_import(&name, &tracks)
    }

    /// Resolve the playlist name: the user's override if set, else the source
    /// file/folder stem, else a generic fallback.
    fn import_playlist_name(dialog: &ImportPlaylistDialog, stem: Option<&str>) -> String {
        if !dialog.playlist_name.trim().is_empty() {
            return dialog.playlist_name.trim().to_string();
        }
        stem.filter(|s| !s.is_empty())
            .map_or_else(|| "Imported".to_string(), std::string::ToString::to_string)
    }

    fn open_imported_playlist(&mut self, index: usize) -> Task<Message> {
        if index >= self.playlists.playlists.len() {
            return Task::none();
        }
        let pane = self.focused_pane_id;
        let name = self.playlists.playlists[index].name.clone();
        self.clear_selection();
        self.drag.cleanup();
        let task = self.push_new_view(pane, ViewData::new_playlist(index, name));
        self.save_session();
        task
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        app::{
            interaction::{HoverTarget, TrackListKind, TrackPos},
            Message, MusicPlayer, ViewData, ViewKind,
        },
        data::config,
        providers::ProviderId,
        types::Track,
    };

    fn player_with_playlists(names: &[&str]) -> MusicPlayer {
        let mut p = MusicPlayer::new_with(config::Config::default());
        p.playlists.playlists.clear();
        for n in names {
            p.playlists.create(n);
        }
        p.reset_test_pane(vec![ViewData::new_playlist(0, String::new())]);
        p
    }

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

    fn hover(p: &mut MusicPlayer, pos: TrackPos) {
        p.drag.set_hovered(HoverTarget::Track(pos));
    }

    #[test]
    fn cycle_playlist_wraps_and_handles_empty() {
        let mut p = player_with_playlists(&["A", "B"]);
        let _ = p.cycle_playlist(1);
        assert!(matches!(
            &p.view_data().kind,
            ViewKind::Playlist(e) if e.index == 1
        ));
        let _ = p.cycle_playlist(1);
        assert!(matches!(
            &p.view_data().kind,
            ViewKind::Playlist(e) if e.index == 0
        ));
        let _ = p.cycle_playlist(-1);
        assert!(matches!(
            &p.view_data().kind,
            ViewKind::Playlist(e) if e.index == 1
        ));
        p.playlists.playlists.clear();
        let _ = p.cycle_playlist(1);
    }

    #[test]
    fn jump_confirm_plays_only_with_shift_held() {
        use iced::keyboard::Modifiers;
        let mut p = player_with_playlists(&["A"]);
        // `new_with` restores the real on-disk session queue; drop it so the
        // test starts from an empty queue.
        p.queue.tracks.clear();
        // MusicBrainz is search-only: confirming play still fills the queue
        // but never spawns a streamer, keeping the test hermetic.
        p.playlists.playlists[0].tracks = vec![Track::from_provider(
            ProviderId::MusicBrainz,
            "x".into(),
            "https://example.com/x".into(),
            "Track x".to_string(),
            "Artist",
            10,
            String::new(),
            None,
            None,
        )];

        let _ = p.open_playlist_jump();
        let _ = p.update(Message::PlaylistJumpConfirm);
        assert!(matches!(
            &p.view_data().kind,
            ViewKind::Playlist(e) if e.index == 0
        ));
        assert!(p.queue.tracks.is_empty());

        let _ = p.open_playlist_jump();
        let _ = p.update(Message::ModifiersChanged(Modifiers::SHIFT));
        let _ = p.update(Message::PlaylistJumpConfirm);
        assert_eq!(p.queue.tracks.len(), 1);
        assert_eq!(p.queue.tracks[0].title, "Track x");
    }

    #[test]
    fn playlist_jump_filters_by_name() {
        use crate::app::PlaylistJump;
        let jump = PlaylistJump::default();
        assert_eq!(
            jump.filtered(&["A".to_string(), "B".to_string()]),
            vec![0, 1]
        );
        let jump = PlaylistJump {
            query: "bet".into(),
            selected: 0,
        };
        assert_eq!(
            jump.filtered(&["Alpha".to_string(), "Beta".to_string()]),
            vec![1]
        );
    }

    #[test]
    fn deleting_selected_playlist_keeps_view_valid() {
        let mut p = player_with_playlists(&["A", "B", "C"]);
        let pane = p.focused_pane_id;
        p.pane_mut(pane).nav_history = vec![ViewData::new_playlist(1, "B".into())];
        p.pane_mut(pane).nav_history_pos = 0;

        // Delete the playlist currently being viewed (B at index 1).
        let _ = p.handle_delete_playlist(1);
        match &p.view_data().kind {
            ViewKind::Playlist(entry) => {
                assert_eq!(entry.index, 1);
                assert_eq!(entry.name, "C");
            }
            other => panic!("expected Playlist view, got {other:?}"),
        }

        // Deleting a playlist above the selected one shifts the selection down.
        let pane = p.focused_pane_id;
        p.pane_mut(pane).nav_history = vec![ViewData::new_playlist(1, "C".into())];
        p.pane_mut(pane).nav_history_pos = 0;
        let _ = p.handle_delete_playlist(0);
        assert_eq!(
            p.view_data().kind,
            ViewKind::Playlist(crate::app::view_data::PlaylistEntry {
                index: 0,
                name: "C".into(),
            })
        );
    }

    #[test]
    fn deleting_last_playlist_navigates_away() {
        let mut p = player_with_playlists(&["A"]);
        let pane = p.focused_pane_id;
        p.pane_mut(pane).nav_history = vec![ViewData::new_playlist(0, "A".into())];
        p.pane_mut(pane).nav_history_pos = 0;

        // Deleting the only playlist (while viewing it) must leave the
        // Playlist view rather than leaving it with no selection.
        let _ = p.handle_delete_playlist(0);
        assert!(p.playlists.playlists.is_empty());
        assert!(!matches!(p.view_data().kind, ViewKind::Playlist(_)));
    }

    #[test]
    fn reorder_playlist_moves_row_and_keeps_active_selection() {
        let mut p = player_with_playlists(&["A", "B", "C", "D"]);
        let pane = p.focused_pane_id;
        p.pane_mut(pane).nav_history = vec![ViewData::new_playlist(1, "B".into())];
        p.pane_mut(pane).nav_history_pos = 0;

        // Drag playlist B (index 1) down to the end (insertion index 4).
        p.drag.drop_target =
            Some(crate::app::interaction::DropTarget::PlaylistReorder { from: 1, to: 4 });
        p.handle_playlist_drop();

        let names: Vec<&str> = p
            .playlists
            .playlists
            .iter()
            .map(|pl| pl.name.as_str())
            .collect();
        assert_eq!(names, vec!["A", "C", "D", "B"]);
        // The active view still points at B, now at index 3.
        assert_eq!(
            p.view_data().kind,
            ViewKind::Playlist(crate::app::view_data::PlaylistEntry {
                index: 3,
                name: "B".into(),
            })
        );
    }

    #[test]
    fn reorder_playlist_above_active_shifts_selection_down() {
        let mut p = player_with_playlists(&["A", "B", "C", "D"]);
        let pane = p.focused_pane_id;
        p.pane_mut(pane).nav_history = vec![ViewData::new_playlist(2, "C".into())];
        p.pane_mut(pane).nav_history_pos = 0;

        // Drag D (index 3) up to the front (insertion index 0).
        p.drag.drop_target =
            Some(crate::app::interaction::DropTarget::PlaylistReorder { from: 3, to: 0 });
        p.handle_playlist_drop();

        let names: Vec<&str> = p
            .playlists
            .playlists
            .iter()
            .map(|pl| pl.name.as_str())
            .collect();
        assert_eq!(names, vec!["D", "A", "B", "C"]);
        // C was at index 2; a row moved in above it, so it shifts to index 3.
        assert_eq!(
            p.view_data().kind,
            ViewKind::Playlist(crate::app::view_data::PlaylistEntry {
                index: 3,
                name: "C".into(),
            })
        );
    }

    #[test]
    fn delete_key_removes_hovered_queue_track() {
        let mut p = player_with_playlists(&["A"]);
        p.queue.tracks = vec![track("0"), track("1"), track("2")];
        {
            let pane = p.focused_pane_id;
            hover(&mut p, TrackPos::new(1, TrackListKind::Queue, pane));
        };
        p.handle_delete_in_hovered_list();
        let ids: Vec<_> = p.queue.tracks.iter().map(|t| t.title.clone()).collect();
        assert_eq!(ids, vec!["Track 0", "Track 2"]);
    }

    #[test]
    fn delete_key_removes_recent_selection() {
        let mut p = player_with_playlists(&["A"]);
        p.queue.recently_played = vec![track("1"), track("2")].into();
        p.recent_selected_indices = vec![0];
        {
            let pane = p.focused_pane_id;
            hover(&mut p, TrackPos::new(0, TrackListKind::Recent, pane));
        };
        p.handle_delete_in_hovered_list();
        assert_eq!(p.queue.recently_played.len(), 1);
        assert_eq!(p.queue.recently_played[0].title, "Track 2");
    }

    #[test]
    fn delete_key_removes_hovered_recent_without_selection() {
        let mut p = player_with_playlists(&["A"]);
        p.queue.recently_played = vec![track("1"), track("2")].into();
        {
            let pane = p.focused_pane_id;
            hover(&mut p, TrackPos::new(1, TrackListKind::Recent, pane));
        };
        p.handle_delete_in_hovered_list();
        assert_eq!(p.queue.recently_played.len(), 1);
        assert_eq!(p.queue.recently_played[0].title, "Track 1");
    }

    #[test]
    fn delete_key_in_search_view_is_noop() {
        let mut p = player_with_playlists(&["A"]);
        let pane = p.focused_pane_id;
        p.pane_mut(pane).nav_history = vec![ViewData::new_search(
            String::new(),
            ProviderId::YouTube,
            crate::providers::SearchScope::Songs,
        )];
        p.pane_mut(pane).nav_history_pos = 0;
        p.view_data_mut().set_tracks(vec![track("1")]);
        p.view_data_mut().selection = vec![0];
        {
            let pane = p.focused_pane_id;
            hover(&mut p, TrackPos::new(0, TrackListKind::Active, pane));
        };
        p.handle_delete_in_hovered_list();
        assert_eq!(p.view_tracks_in(p.focused_pane_id).len(), 1);
        assert_eq!(p.selection(TrackListKind::Active), &[0]);
    }

    #[test]
    fn delete_in_one_list_keeps_other_lists_selections() {
        let mut p = player_with_playlists(&["A"]);
        p.playlists.playlists[0].tracks = vec![track("a1"), track("a2")];
        p.queue.tracks = vec![track("0"), track("q1"), track("q2")];
        p.queue.recently_played = vec![track("r1"), track("r2")].into();
        p.view_data_mut().selection = vec![0];
        p.queue_selected_indices = vec![2];
        p.recent_selected_indices = vec![0];

        {
            let pane = p.focused_pane_id;
            hover(&mut p, TrackPos::new(2, TrackListKind::Queue, pane));
        };
        p.handle_delete_in_hovered_list();

        assert!(p.selection(TrackListKind::Queue).is_empty());
        assert_eq!(p.selection(TrackListKind::Active), &[0]);
        assert_eq!(p.selection(TrackListKind::Recent), &[0]);
    }

    #[test]
    fn delete_in_recent_keeps_other_lists_selections() {
        let mut p = player_with_playlists(&["A"]);
        p.playlists.playlists[0].tracks = vec![track("a1"), track("a2")];
        p.queue.tracks = vec![track("0"), track("q1")];
        p.queue.recently_played = vec![track("r1"), track("r2")].into();
        p.view_data_mut().selection = vec![1];
        p.queue_selected_indices = vec![1];
        p.recent_selected_indices = vec![0];

        {
            let pane = p.focused_pane_id;
            hover(&mut p, TrackPos::new(0, TrackListKind::Recent, pane));
        };
        p.handle_delete_in_hovered_list();

        assert!(p.selection(TrackListKind::Recent).is_empty());
        assert_eq!(p.selection(TrackListKind::Active), &[1]);
        assert_eq!(p.selection(TrackListKind::Queue), &[1]);
    }
}
