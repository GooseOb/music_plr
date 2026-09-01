use std::{
    fs,
    path::{Path, PathBuf},
};

use super::{Message, MusicPlayer, Task, Track, ViewData};
use crate::{
    app::{ImportMethod, ImportPlaylistDialog, ViewKind},
    data::JsonStore,
};

impl MusicPlayer {
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

    pub fn handle_select_playlist(&mut self, index: usize) -> Task<Message> {
        let already_selected =
            matches!(&self.view_data().kind, ViewKind::Playlist(p) if p.index == index);
        if index < self.playlists.playlists.len() && !already_selected {
            self.playlist_picker = None;
            self.lyrics = None;
            self.clear_selection();
            self.drag.cleanup();

            let playlist_name = self.playlists.playlists[index].name.clone();
            let task = self.push_new_view(ViewData::new_playlist(index, playlist_name));
            let view = self.view_data().clone();
            self.seed_view_thumbnails(&view);

            self.save_session();
            return task;
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
            let first = tracks[0].clone();
            self.queue
                .set_queue(tracks, self.config.max_recently_played);
            self.record_now_playing_origin();
            self.play_track_internal(&first, first.source);
            self.save_session();
            self.mpris_dirty = true;
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
        if let ViewKind::Playlist(entry) = &mut self.view_data_mut().kind {
            entry.name = new_name.trim().to_string();
        }
    }

    pub fn handle_delete_playlist(&mut self, index: usize) -> Task<Message> {
        self.playlists.delete(index);

        // The currently viewed playlist may be the one being deleted. A
        // `Playlist` view must always have a selected playlist, so either keep
        // a valid adjacent selection or, if none remain, leave for a safe view.
        let mut navigate_away = false;
        let mut new_selection: Option<usize> = None;
        if let ViewKind::Playlist(entry) = &self.view_data().kind {
            let sp = entry.index;
            if sp == index {
                if self.playlists.playlists.is_empty() {
                    navigate_away = true;
                } else {
                    new_selection = Some(index.min(self.playlists.playlists.len() - 1));
                }
            } else if sp > index {
                // The deleted playlist was above the selected one; shift the
                // selection down by one so it still points at the same playlist.
                new_selection = Some(sp - 1);
            }
        }

        let mut nav_task = Task::none();
        if navigate_away {
            nav_task = self.push_new_view(ViewData::new_search(
                String::new(),
                self.search_provider,
                self.search_scope,
            ));
        } else if let Some(new_idx) = new_selection {
            let new_name = self.playlists.playlists[new_idx].name.clone();
            if let ViewKind::Playlist(entry) = &mut self.view_data_mut().kind {
                entry.index = new_idx;
                entry.name = new_name;
            }
        }

        self.delete_confirm_index = None;
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

        let count = self.playlists.insert_tracks_at(idx, new_tracks.iter(), 0);
        let msg = (self.strings.added)(count);
        self.notify(msg);
    }

    pub fn handle_add_to_playlist(
        &mut self,
        playlist_idx: usize,
        indices: &[usize],
        list: super::TrackListKind,
    ) {
        if playlist_idx >= self.playlists.playlists.len() {
            return;
        }

        let tracks: Vec<Track> = indices
            .iter()
            .filter_map(|&i| self.get_track_at(super::TrackPos::new(i, list)))
            .collect();
        let count = self
            .playlists
            .insert_tracks_at(playlist_idx, tracks.iter(), 0);
        self.playlist_picker = None;
        let name = self.playlists.playlists[playlist_idx].name.clone();
        let msg = (self.strings.added_to)(count, &name);
        self.notify(msg);
    }

    pub fn handle_remove_from_playlist_batch(&mut self, indices: &[usize]) {
        if let ViewKind::Playlist(p) = &self.view_data().kind {
            if p.index < self.playlists.playlists.len() {
                let removed = self.playlists.remove_tracks_at(p.index, indices);
                let msg = (self.strings.removed_n)(removed);
                self.notify(msg);
                self.clear_selection_if_touched(indices, super::TrackListKind::Active);
            }
        }
    }

    pub fn handle_reorder_tracks_selected(
        &mut self,
        drop_idx: usize,
        indices: &[usize],
        selection: &[usize],
    ) -> Vec<usize> {
        let sp = match &self.view_data().kind {
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
        self.clipboard.clear();
        let selection: Vec<usize> = self.view_data_mut().selection.clone();
        for &i in &selection {
            if let Some(track) =
                self.get_track_at(super::TrackPos::new(i, super::TrackListKind::Active))
            {
                self.clipboard.push(track.clone());
            }
        }
    }

    pub fn handle_paste_clipboard(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        let active = match &self.view_data().kind {
            ViewKind::Playlist(p) => Some(p.index),
            _ => None,
        };
        let Some(idx) = active else {
            return;
        };
        self.playlists
            .insert_tracks_at(idx, self.clipboard.iter().rev(), 0);
        self.playlists.save();
        let count = self.clipboard.len();
        let name = self.playlists.playlists[idx].name.clone();
        let msg = (self.strings.pasted_into)(count, &name);
        self.notify(msg);
        self.clipboard.clear();
    }

    pub fn handle_delete_selected(&mut self) {
        if self.view_data_mut().selection.is_empty() {
            return;
        }
        let indices: Vec<usize> = self.view_data_mut().selection.clone();

        if matches!(self.view_data_mut().kind, ViewKind::Playlist(_)) {
            if let ViewKind::Playlist(p) = &self.view_data().kind {
                if p.index < self.playlists.playlists.len() {
                    let removed = self.playlists.remove_tracks_at(p.index, &indices);
                    let msg = (self.strings.removed_n)(removed);
                    self.notify(msg);
                }
            }
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
        self.clear_selection();
    }

    /// Open the file/folder picker for the current import method. The picked
    /// path is delivered back through `BackendResult::ImportPathsPicked`.
    pub fn handle_import_pick(&mut self) {
        let Some(dialog) = &self.import_dialog else {
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
        let Some(dialog) = self.import_dialog.clone() else {
            return Task::none();
        };
        let (ok, task) = match method {
            ImportMethod::Native => self.import_native(&paths[0]),
            ImportMethod::Csv => self.import_csv(&paths[0], &dialog),
            ImportMethod::FileList => self.import_file_list(&paths[0], &dialog),
        };
        if ok {
            self.import_dialog = None;
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
            self.playlists.save();
        }
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
        if tracks.is_empty() {
            self.notify(self.strings.import_no_tracks);
            return (false, Task::none());
        }
        let name = Self::import_playlist_name(dialog, path.file_stem().and_then(|s| s.to_str()));
        let idx = self
            .playlists
            .create_at(&name, self.playlists.playlists.len());
        self.playlists.insert_tracks_at(idx, tracks.iter(), 0);
        let label = self.playlists.playlists[idx].name.clone();
        self.notify((self.strings.import_imported_into)(tracks.len(), &label));
        let task = self.open_imported_playlist(idx);
        (true, task)
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
        let idx = self
            .playlists
            .create_at(&name, self.playlists.playlists.len());
        self.playlists.insert_tracks_at(idx, tracks.iter(), 0);
        let label = self.playlists.playlists[idx].name.clone();
        self.notify((self.strings.import_imported_into)(tracks.len(), &label));
        let task = self.open_imported_playlist(idx);
        (true, task)
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
        let name = self.playlists.playlists[index].name.clone();
        self.clear_selection();
        self.drag.cleanup();
        let task = self.push_new_view(ViewData::new_playlist(index, name));
        self.save_session();
        task
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        app::{MusicPlayer, ViewData, ViewKind},
        data::config,
    };

    fn player_with_playlists(names: &[&str]) -> MusicPlayer {
        let mut p = MusicPlayer::new_with(config::Config::default());
        p.playlists.playlists.clear();
        for n in names {
            p.playlists.create(n);
        }
        p.nav_history = vec![ViewData::new_playlist(0, String::new())];
        p.nav_history_pos = 0;
        p
    }

    #[test]
    fn deleting_selected_playlist_keeps_view_valid() {
        let mut p = player_with_playlists(&["A", "B", "C"]);
        p.nav_history = vec![ViewData::new_playlist(1, "B".into())];
        p.nav_history_pos = 0;

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
        p.nav_history = vec![ViewData::new_playlist(1, "C".into())];
        p.nav_history_pos = 0;
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
        p.nav_history = vec![ViewData::new_playlist(0, "A".into())];
        p.nav_history_pos = 0;

        // Deleting the only playlist (while viewing it) must leave the
        // Playlist view rather than leaving it with no selection.
        let _ = p.handle_delete_playlist(0);
        assert!(p.playlists.playlists.is_empty());
        assert!(!matches!(p.view_data().kind, ViewKind::Playlist(_)));
    }

    #[test]
    fn reorder_playlist_moves_row_and_keeps_active_selection() {
        let mut p = player_with_playlists(&["A", "B", "C", "D"]);
        p.nav_history = vec![ViewData::new_playlist(1, "B".into())];
        p.nav_history_pos = 0;

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
        p.nav_history = vec![ViewData::new_playlist(2, "C".into())];
        p.nav_history_pos = 0;

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
}
