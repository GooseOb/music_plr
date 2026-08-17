use super::{MusicPlayer, Track, TrackSource, ViewData};
use crate::{app::ViewKind, data::JsonStore};
use std::path::PathBuf;

impl MusicPlayer {
    pub fn handle_create_playlist(&mut self) {
        if self.playlist_create_name.trim().is_empty() {
            return;
        }
        let name = self.playlist_create_name.trim().to_string();
        self.playlists.create(&name);
        self.playlist_create_name.clear();
        self.notify(format!("Playlist \"{name}\" created"));
    }

    pub fn handle_select_playlist(&mut self, index: usize) {
        if index < self.playlists.playlists.len()
            && self.view_data_mut().selected_playlist_id() != Some(index)
        {
            self.playlist_picker = None;
            self.clear_selection();
            self.drag.cleanup();

            let playlist_name = self.playlists.playlists[index].name.clone();
            self.push_new_view(ViewData::new_playlist(Some(index), playlist_name, None));

            self.save_session();
        }
    }

    pub fn handle_rename_playlist(&mut self, new_name: &str) {
        if let Some(idx) = self.view_data_mut().selected_playlist_id() {
            if !new_name.trim().is_empty() {
                self.playlists.playlists[idx].name = new_name.trim().to_string();
                self.playlists.save();
                if let ViewKind::Playlist { playlist_name, .. } = &mut self.view_data_mut().kind {
                    *playlist_name = new_name.trim().to_string();
                }
            }
        }
    }

    pub fn handle_delete_playlist(&mut self, index: usize) {
        self.playlists.delete(index);

        // The currently viewed playlist may be the one being deleted. A
        // `Playlist` view must always have a selected playlist, so either keep
        // a valid adjacent selection or, if none remain, leave for a safe view.
        let mut navigate_away = false;
        let mut new_selection: Option<usize> = None;
        if let ViewKind::Playlist {
            selected_playlist, ..
        } = &self.view_data().kind
        {
            match *selected_playlist {
                Some(sp) if sp == index => {
                    if self.playlists.playlists.is_empty() {
                        navigate_away = true;
                    } else {
                        new_selection = Some(index.min(self.playlists.playlists.len() - 1));
                    }
                }
                // The deleted playlist was above the selected one; shift the
                // selection down by one so it still points at the same playlist.
                Some(sp) if sp > index => new_selection = Some(sp - 1),
                _ => {}
            }
        }

        if navigate_away {
            self.push_new_view(ViewData::new_search(String::new(), self.search_scope));
        } else if let Some(new_idx) = new_selection {
            let name = self.playlists.playlists[new_idx].name.clone();
            if let ViewKind::Playlist {
                selected_playlist,
                playlist_name,
            } = &mut self.view_data_mut().kind
            {
                *selected_playlist = Some(new_idx);
                *playlist_name = name;
            }
        }

        self.delete_confirm_index = None;
    }

    pub fn handle_add_local_music(&mut self, paths: &[PathBuf]) {
        let mut new_tracks = Vec::new();

        for path in paths {
            let path_str = path.to_string_lossy().to_string();
            if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
                let duration = crate::util::try_probe_duration(&path_str).unwrap_or(0);
                new_tracks.push(Track {
                    id: filename.to_string(),
                    title: filename.to_string(),
                    artist: "Unknown Artist".to_string(),
                    duration,
                    url: path_str,
                    source: TrackSource::Local,
                    thumbnail: String::new(),
                    download_path: None,
                    album: None,
                });
            }
        }

        let Some(idx) = self.view_data_mut().selected_playlist_id() else {
            let count = new_tracks.len();
            self.notify(format!(
                "Added {} local track{} (select a playlist to organize)",
                count,
                crate::util::plural_suffix(count)
            ));
            return;
        };

        for track in &new_tracks {
            self.playlists.insert_track_at(idx, track, usize::MAX);
        }
        self.playlists.save();

        self.notify_tracks("Added", new_tracks.len(), "");
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
        let count = tracks.len();
        if count > 0 {
            self.playlists.insert_tracks_at(playlist_idx, &tracks, 0);
        }
        self.playlist_picker = None;
        let name = self.playlists.playlists[playlist_idx].name.clone();
        self.notify_tracks("Added", count, &format!("to {name}"));
    }

    pub fn handle_remove_from_playlist_batch(&mut self, indices: &[usize]) {
        if let Some(sp) = self.view_data_mut().selected_playlist_id() {
            if sp < self.playlists.playlists.len() {
                let removed = self.playlists.remove_tracks_at(sp, indices);
                self.notify_tracks("Removed", removed, "");
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
        let new_positions = if let Some(sp) = self.view_data_mut().selected_playlist_id() {
            if sp < self.playlists.playlists.len() {
                crate::util::reorder_tracks(
                    &mut self.playlists.playlists[sp].tracks,
                    drop_idx,
                    indices,
                    selection,
                )
            } else {
                Vec::new()
            }
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
        let Some(idx) = self.view_data_mut().selected_playlist_id() else {
            return;
        };
        for track in self.clipboard.iter().rev() {
            self.playlists.insert_track_at(idx, track, 0);
        }
        self.playlists.save();
        let count = self.clipboard.len();
        let name = self.playlists.playlists[idx].name.clone();
        self.notify_tracks("Pasted", count, &format!("into {name}"));
        self.clipboard.clear();
    }

    pub fn handle_delete_selected(&mut self) {
        if self.view_data_mut().selection.is_empty() {
            return;
        }
        let indices: Vec<usize> = self.view_data_mut().selection.clone();

        if matches!(self.view_data_mut().kind, ViewKind::Playlist { .. }) {
            if let Some(sp) = self.view_data_mut().selected_playlist_id() {
                if sp < self.playlists.playlists.len() {
                    let removed = self.playlists.remove_tracks_at(sp, &indices);
                    self.notify_tracks("Removed", removed, "");
                }
            }
        } else if let ViewKind::Downloads = &self.view_data_mut().kind {
            let tracks = &mut self.view_data_mut().tracks;
            let removed_urls: Vec<String> = indices
                .iter()
                .filter_map(|&i| tracks.get(i).map(|t| t.url.clone()))
                .collect();
            let removed = crate::util::remove_at(tracks, &indices);
            self.notify_tracks("Removed", removed, "from downloads");
            for url in removed_urls {
                self.download_registry.remove(&url);
            }
        }
        self.clear_selection();
    }
}

#[cfg(test)]
mod tests {
    use crate::app::{MusicPlayer, ViewData, ViewKind};
    use crate::data::config;

    fn player_with_playlists(names: &[&str]) -> MusicPlayer {
        let mut p = MusicPlayer::new_with(config::Config::default());
        p.playlists.playlists.clear();
        for n in names {
            p.playlists.create(n);
        }
        p.nav_history = vec![ViewData::new_playlist(Some(0), String::new(), None)];
        p.nav_history_pos = 0;
        p
    }

    #[test]
    fn deleting_selected_playlist_keeps_view_valid() {
        let mut p = player_with_playlists(&["A", "B", "C"]);
        p.nav_history = vec![ViewData::new_playlist(Some(1), "B".into(), None)];
        p.nav_history_pos = 0;

        // Delete the playlist currently being viewed (B at index 1).
        p.handle_delete_playlist(1);
        match &p.view_data().kind {
            ViewKind::Playlist {
                selected_playlist: Some(sp),
                playlist_name,
            } => {
                assert_eq!(*sp, 1);
                assert_eq!(playlist_name, "C");
            }
            other => panic!("expected Playlist view, got {other:?}"),
        }

        // Deleting a playlist above the selected one shifts the selection down.
        p.nav_history = vec![ViewData::new_playlist(Some(1), "C".into(), None)];
        p.nav_history_pos = 0;
        p.handle_delete_playlist(0);
        assert_eq!(
            p.view_data().kind,
            ViewKind::Playlist {
                selected_playlist: Some(0),
                playlist_name: "C".into(),
            }
        );
    }

    #[test]
    fn deleting_last_playlist_navigates_away() {
        let mut p = player_with_playlists(&["A"]);
        p.nav_history = vec![ViewData::new_playlist(Some(0), "A".into(), None)];
        p.nav_history_pos = 0;

        // Deleting the only playlist (while viewing it) must leave the
        // Playlist view rather than leaving it with no selection.
        p.handle_delete_playlist(0);
        assert!(p.playlists.playlists.is_empty());
        assert!(!matches!(p.view_data().kind, ViewKind::Playlist { .. }));
    }

    #[test]
    fn reorder_playlist_moves_row_and_keeps_active_selection() {
        let mut p = player_with_playlists(&["A", "B", "C", "D"]);
        p.nav_history = vec![ViewData::new_playlist(Some(1), "B".into(), None)];
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
            ViewKind::Playlist {
                selected_playlist: Some(3),
                playlist_name: "B".into(),
            }
        );
    }

    #[test]
    fn reorder_playlist_above_active_shifts_selection_down() {
        let mut p = player_with_playlists(&["A", "B", "C", "D"]);
        p.nav_history = vec![ViewData::new_playlist(Some(2), "C".into(), None)];
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
            ViewKind::Playlist {
                selected_playlist: Some(3),
                playlist_name: "C".into(),
            }
        );
    }
}
