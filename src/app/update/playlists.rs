use super::{MusicPlayer, Track, TrackSource, ViewData};
use crate::{app::ViewKind, data::JsonStore};
use std::path::Path;

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
            self.cleanup_drag_state();
            self.drag.hovered_track = None;

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
        if let ViewKind::Playlist {
            selected_playlist,
            playlist_name,
            ..
        } = &mut self.view_data_mut().kind
        {
            if *selected_playlist == Some(index) {
                *selected_playlist = None;
                playlist_name.clear();
            } else if *selected_playlist > Some(index) {
                *selected_playlist = selected_playlist.map(|sp| sp - 1);
            }
        }
        self.show_delete_confirm = false;
        self.delete_confirm_index = None;
    }

    pub fn handle_add_local_music(&mut self, paths: &[String]) {
        let mut new_tracks = Vec::new();
        for path_str in paths {
            let path = Path::new(path_str);
            if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
                let duration = crate::util::try_probe_duration(path_str).unwrap_or(0);
                new_tracks.push(Track {
                    id: filename.to_string(),
                    title: filename.to_string(),
                    artist: "Unknown Artist".to_string(),
                    duration,
                    url: path_str.clone(),
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
                // Drop a now-stale selection if any removed index was selected.
                let sel = self.view_data_mut().selection.clone();
                if indices.iter().any(|&i| sel.contains(&i)) {
                    self.clear_selection();
                }
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
