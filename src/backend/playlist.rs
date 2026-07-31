use super::PlaylistState;
use crate::types::{Track as RustTrack, TrackSource};
use slint::ComponentHandle;
use tracing::debug;

impl super::Backend {
    pub fn handle_add_local_music(&mut self, paths: Vec<String>) {
        let tracks: Vec<RustTrack> = paths
            .iter()
            .map(|path_str| {
                let file_stem = std::path::Path::new(path_str)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .unwrap_or("unknown")
                    .to_string();
                let id = format!("local:{}", path_str);
                RustTrack {
                    id,
                    title: file_stem,
                    artist: "Local File".to_string(),
                    duration: 0,
                    url: path_str.clone(),
                    source: TrackSource::Local,
                    thumbnail: String::new(),
                }
            })
            .collect();

        let count = tracks.len();
        if let Some(pl_idx) = self.selected_playlist {
            for t in tracks {
                self.playlists.add_track(pl_idx, &t);
            }
            self.sync_playlist_sidebar();
            self.sync_playlist_content();
            self.notify(format!(
                "Added {} file{}",
                count,
                if count == 1 { "" } else { "s" }
            ));
        } else if count > 0 {
            let name = "Local Music".to_string();
            self.playlists.create(&name);
            self.selected_playlist = Some(0);
            for t in tracks {
                self.playlists.add_track(0, &t);
            }
            self.sync_playlist_sidebar();
            self.sync_playlist_content();
            self.notify(format!(
                "Created playlist and added {} file{}",
                count,
                if count == 1 { "" } else { "s" }
            ));
        }
    }

    pub fn handle_create_playlist(&mut self) {
        let name = self.playlist_create_name.trim().to_string();
        if !name.is_empty() {
            self.playlists.create(&name);
            self.playlist_create_name.clear();
            if self.playlists.playlists.len() == 1 {
                self.selected_playlist = Some(0);
            }
        }
        self.sync_playlist_sidebar();
        if self.selected_playlist.is_some() {
            self.sync_playlist_content();
        }
    }

    pub fn handle_delete_playlist(&mut self, index: usize) {
        self.playlists.delete(index);
        if self.selected_playlist == Some(index) {
            self.selected_playlist = None;
        } else if let Some(i) = self.selected_playlist {
            if i > index {
                self.selected_playlist = Some(i - 1);
            }
        }
        self.clear_selection();
        self.sync_playlist_sidebar();
        self.sync_playlist_content();
    }

    pub fn handle_select_playlist(&mut self, index: usize) {
        if cfg!(debug_assertions) {
            debug!("select playlist {}", index);
        }
        if self.selected_playlist == Some(index) {
            self.selected_playlist = None;
            self.selected_playlist_name.clear();
        } else {
            self.selected_playlist = Some(index);
            self.selected_playlist_name = self
                .playlists
                .playlists
                .get(index)
                .map(|pl| pl.name.clone())
                .unwrap_or_default();
        }
        let pl_tracks: Vec<RustTrack> = self
            .selected_playlist
            .and_then(|i| self.playlists.playlists.get(i))
            .map(|pl| pl.tracks.clone())
            .unwrap_or_default();
        self.clear_selection();
        self.sync_playlist_content();
        self.spawn_thumbnail_downloads(&pl_tracks);
    }

    pub fn handle_toggle_picker(&mut self, index: usize) {
        if self.show_playlist_picker == Some(index) {
            self.show_playlist_picker = None;
            if let Some(window) = self.ui.upgrade() {
                window.global::<PlaylistState>().set_show_picker(false);
            }
        } else if self.playlists.playlists.is_empty() {
            self.notification = Some("Create a playlist first".into());
        } else {
            self.show_playlist_picker = Some(index);
            if let Some(window) = self.ui.upgrade() {
                let playlist_state = window.global::<PlaylistState>();
                playlist_state.set_picker_track_idx(index as i32);
                playlist_state.set_show_picker(true);
            }
        }
    }

    pub fn handle_add_to_playlist(&mut self, playlist_idx: usize) {
        let track_idx = self.show_playlist_picker.unwrap_or(0);
        let track = match self.get_track_at(track_idx) {
            Some(t) => t,
            None => return,
        };
        self.playlists.add_track(playlist_idx, &track);
        self.show_playlist_picker = None;
        if let Some(window) = self.ui.upgrade() {
            window.global::<PlaylistState>().set_show_picker(false);
        }
        self.sync_playlist_sidebar();
        self.sync_playlist_content();
        self.notify(format!(
            "Added to {}",
            self.playlists.playlists[playlist_idx].name
        ));
    }

    pub fn handle_drag_add_to_playlist(&mut self, track_idx: usize, playlist_idx: usize) {
        let indices: Vec<usize> = if self.selected_indices.is_empty() {
            vec![track_idx]
        } else {
            self.selected_indices.clone()
        };

        let mut count = 0;
        for &i in &indices {
            if let Some(track) = self.get_track_at(i) {
                self.playlists.add_track(playlist_idx, &track);
                count += 1;
            }
        }

        self.sync_playlist_sidebar();
        self.sync_playlist_content();
        self.notify(format!(
            "Added {} track{} to {}",
            count,
            if count == 1 { "" } else { "s" },
            self.playlists.playlists[playlist_idx].name
        ));
    }

    pub fn handle_remove_from_playlist(&mut self, track_idx: usize) {
        if let Some(pl_idx) = self.selected_playlist {
            self.playlists.remove_track(pl_idx, track_idx);
        }
        self.clear_selection();
        self.sync_playlist_sidebar();
        self.sync_playlist_content();
    }

    pub fn handle_reorder_tracks(&mut self, from_idx: usize, to_idx: usize) {
        let pl_idx = match self.selected_playlist {
            Some(i) => i,
            None => return,
        };

        let indices = if self.selected_indices.is_empty() {
            vec![from_idx]
        } else if self.selected_indices.contains(&from_idx) {
            self.selected_indices.clone()
        } else {
            self.selected_indices.clear();
            vec![from_idx]
        };

        let count_before = indices.iter().filter(|&&i| i < to_idx).count();
        let adj_target = to_idx.saturating_sub(count_before);

        self.playlists.move_tracks(pl_idx, &indices, to_idx);

        if !self.selected_indices.is_empty() {
            self.selected_indices = (adj_target..adj_target + indices.len()).collect();
        }

        self.sync_playlist_sidebar();
        self.sync_playlist_content();
    }
}
