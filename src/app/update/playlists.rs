use super::{MusicPlayer, Track, TrackSource, View};
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
        if index < self.playlists.playlists.len() && self.selected_playlist != Some(index) {
            self.show_playlist_picker = None;
            self.clear_selection();
            self.cleanup_drag_state();
            self.drag.hovered_track = None;

            self.current_view = View::Playlist;
            self.selected_playlist = Some(index);
            self.selected_playlist_name = self.playlists.playlists[index].name.clone();
            self.selected_indices.clear();

            self.push_nav_entry();
            self.save_session();
        }
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn handle_rename_playlist(&mut self, new_name: String) {
        if let Some(idx) = self.selected_playlist {
            if !new_name.trim().is_empty() {
                self.playlists.playlists[idx].name = new_name.trim().to_string();
                self.playlists.save();
                self.selected_playlist_name = new_name.trim().to_string();
            }
        }
    }

    pub fn handle_delete_playlist(&mut self, index: usize) {
        self.playlists.delete(index);
        if self.selected_playlist == Some(index) {
            self.selected_playlist = None;
            self.selected_playlist_name.clear();
        } else if self.selected_playlist > Some(index) {
            self.selected_playlist = self.selected_playlist.map(|sp| sp - 1);
        }
        self.show_delete_confirm = false;
        self.delete_confirm_index = None;
    }

    #[allow(clippy::needless_pass_by_value)]
    pub fn handle_add_local_music(&mut self, paths: Vec<String>) {
        let mut new_tracks = Vec::new();
        for path_str in &paths {
            let path = Path::new(path_str);
            if let Some(filename) = path.file_stem().and_then(|s| s.to_str()) {
                new_tracks.push(Track {
                    id: filename.to_string(),
                    title: filename.to_string(),
                    artist: "Unknown Artist".to_string(),
                    duration: 0,
                    url: path_str.clone(),
                    source: TrackSource::Local,
                    thumbnail: String::new(),
                });
            }
        }

        let Some(idx) = self.selected_playlist else {
            let count = new_tracks.len();
            self.notify(format!(
                "Added {} local track{} (select a playlist to organize)",
                count,
                if count == 1 { "" } else { "s" }
            ));
            return;
        };

        for track in &new_tracks {
            self.playlists.insert_track_at(idx, track, usize::MAX);
        }
        self.playlists.save();

        let count = new_tracks.len();
        self.notify(format!(
            "Added {} local track{}",
            count,
            if count == 1 { "" } else { "s" }
        ));
    }

    pub fn handle_add_to_playlist(&mut self, playlist_idx: usize) {
        if playlist_idx >= self.playlists.playlists.len() {
            return;
        }
        let indices: Vec<usize> = if self.selected_indices.is_empty() {
            if let Some(t) = self.drag.pressed_track {
                vec![t]
            } else {
                return;
            }
        } else {
            self.selected_indices.clone()
        };

        let tracks: Vec<Track> = indices
            .iter()
            .rev()
            .filter_map(|&i| self.get_track_at(i, false))
            .collect();
        let count = tracks.len();
        if count > 0 {
            self.playlists.insert_tracks_at(playlist_idx, &tracks, 0);
        }
        self.show_playlist_picker = None;
        let name = self.playlists.playlists[playlist_idx].name.clone();
        self.notify(format!(
            "Added {} track{} to {}",
            count,
            if count == 1 { "" } else { "s" },
            name
        ));
    }

    pub fn handle_remove_from_playlist(&mut self, index: usize) {
        if let Some(sp) = self.selected_playlist {
            if sp < self.playlists.playlists.len() {
                self.playlists.playlists[sp].tracks.remove(index);
                self.playlists.save();
            }
        }
    }

    pub fn handle_reorder_tracks_selected(
        &mut self,
        drop_idx: usize,
        indices: &[usize],
    ) -> Vec<usize> {
        let new_positions = if let Some(sp) = self.selected_playlist {
            if sp < self.playlists.playlists.len() {
                super::drag::reorder_tracks(
                    &mut self.playlists.playlists[sp].tracks,
                    drop_idx,
                    indices,
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
        for &i in &self.selected_indices {
            if let Some(track) = self.get_track_at(i, false) {
                self.clipboard.push(track.clone());
            }
        }
    }

    pub fn handle_paste_clipboard(&mut self) {
        if self.clipboard.is_empty() {
            return;
        }
        let Some(idx) = self.selected_playlist else {
            return;
        };
        for track in self.clipboard.iter().rev() {
            self.playlists.insert_track_at(idx, track, 0);
        }
        self.playlists.save();
        let count = self.clipboard.len();
        let name = self.playlists.playlists[idx].name.clone();
        self.notify(format!(
            "Pasted {} track{} into {}",
            count,
            if count == 1 { "" } else { "s" },
            name
        ));
        self.clipboard.clear();
    }

    pub fn handle_delete_selected(&mut self) {
        if self.selected_indices.is_empty() {
            return;
        }
        match &self.current_view {
            View::Playlist => {
                if let Some(sp) = self.selected_playlist {
                    if sp < self.playlists.playlists.len() {
                        let indices: Vec<usize> = self.selected_indices.clone();
                        let mut removed = 0;
                        for &i in indices.iter().rev() {
                            if i < self.playlists.playlists[sp].tracks.len() {
                                self.playlists.playlists[sp].tracks.remove(i);
                                removed += 1;
                            }
                        }
                        self.playlists.save();
                        self.notify(format!(
                            "Removed {} track{}",
                            removed,
                            if removed == 1 { "" } else { "s" }
                        ));
                    }
                }
            }
            View::Downloads => {
                let indices: Vec<usize> = self.selected_indices.clone();
                let mut removed = 0;
                for &i in indices.iter().rev() {
                    if i < self.downloaded_tracks.len() {
                        let track = self.downloaded_tracks.remove(i);
                        self.download_registry.remove(&track.url);
                        removed += 1;
                    }
                }
                self.notify(format!(
                    "Removed {} download{}",
                    removed,
                    if removed == 1 { "" } else { "s" }
                ));
            }
            _ => {}
        }
        self.clear_selection();
    }
}
