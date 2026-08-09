use super::{BackendResult, ContextMenuState, MusicPlayer, Track, TrackSource};
use crate::app::interaction::{TrackListKind, TrackPos};
use crate::{
    app::{PlaylistPicker, ViewKind},
    util::plural_suffix,
};

impl MusicPlayer {
    /// Handle download / delete-download for a set of track indices.
    /// Tracks already downloaded get removed from the registry; tracks
    /// not yet downloaded get queued for download. `list` sources the tracks
    /// from the matching list (queue, active track list, or Recently Played).
    pub fn handle_download_or_remove_tracks(&mut self, indices: &[usize], list: TrackListKind) {
        let mut to_download = Vec::new();
        let mut to_remove = Vec::new();

        for &idx in indices {
            let track = self.get_track_at(TrackPos::new(idx, list));
            if let Some(track) = track {
                if self.download_registry.contains(&track.url) {
                    to_remove.push(track);
                } else if track.source == TrackSource::YouTube {
                    to_download.push(track);
                }
            }
        }

        for track in &to_remove {
            self.download_registry.remove(&track.url);
        }

        if !to_download.is_empty() {
            if to_download.len() == 1 {
                let track = to_download[0].clone();
                self.notify(format!("Downloading \"{}\"...", track.title));
                self.spawn_download_thread(track);
            } else {
                let count = to_download.len();
                self.notify(format!("Downloading {count} tracks..."));
                for track in &to_download {
                    let track = track.clone();
                    self.spawn_download_thread(track);
                }
            }
        }

        if !to_remove.is_empty() {
            let removed = to_remove.len();
            self.notify(format!(
                "Removed {} download{}",
                removed,
                plural_suffix(removed)
            ));
        }
        // Drop a now-stale selection if any operated-on index was selected.
        let sel = self.selection(list);
        if indices.iter().any(|&i| sel.contains(&i)) {
            self.clear_selection();
        }
    }

    fn spawn_download_thread(&self, track: Track) {
        let download_dir = self.config.download_dir.clone();
        let tx = self.result_tx.clone();
        let track_clone = track.clone();
        std::thread::spawn(move || {
            let result = crate::youtube::download(&track.url, &download_dir);
            match result {
                Ok(path) => {
                    let _ = tx.send(BackendResult::DownloadComplete(track_clone, path));
                }
                Err(e) => {
                    let _ = tx.send(BackendResult::DownloadError(e.to_string()));
                }
            }
        });
    }

    pub fn handle_toggle_picker(&mut self, indices: Vec<usize>, list: TrackListKind) {
        self.playlist_picker = if self.playlist_picker.is_some() {
            None
        } else {
            Some(PlaylistPicker { indices, list })
        }
    }

    pub fn show_context_menu(&mut self, pos: TrackPos) {
        let Some(track) = self.get_track_at(pos) else {
            return;
        };
        let TrackPos { index, list } = pos;

        let sel = self.selection(list);
        let target_indices = if sel.contains(&index) {
            sel.to_vec()
        } else {
            vec![index]
        };

        self.context_menu = Some(ContextMenuState {
            pos,
            target_indices,
            position: (self.drag.cursor_pos.x, self.drag.cursor_pos.y),
            is_youtube: track.source == TrackSource::YouTube,
            is_downloaded: self.download_registry.contains(&track.url),
            in_playlist: matches!(self.view_data_mut().kind, ViewKind::Playlist { .. }),
            track,
        });
    }
}
