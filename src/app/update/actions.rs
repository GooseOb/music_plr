use super::{BackendResult, ContextMenuState, MusicPlayer, Track, TrackSource, View};
use crate::util::plural_suffix;

impl MusicPlayer {
    /// Handle download / delete-download for a set of track indices.
    /// Tracks already downloaded get removed from the registry; tracks
    /// not yet downloaded get queued for download.
    pub fn handle_download_or_remove_tracks(&mut self, indices: &[usize], is_queue: bool) {
        let mut to_download = Vec::new();
        let mut to_remove = Vec::new();

        for &idx in indices {
            if let Some(track) = self.get_track_at(idx, is_queue) {
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
        // Clear selection if any of the operated-on indices were selected,
        // since the list state has changed.
        let sel = self.selection(is_queue);
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

    pub fn handle_toggle_picker(&mut self, indices: Vec<usize>, is_queue: bool) {
        if self.show_playlist_picker {
            self.show_playlist_picker = false;
            self.picker_target_indices.clear();
        } else {
            self.show_playlist_picker = true;
            self.picker_is_queue = is_queue;
            self.picker_target_indices = indices;
        }
    }

    pub fn show_context_menu(&mut self, index: usize, is_queue: bool) {
        let track = self.get_track_at(index, is_queue);
        let Some(track) = track else {
            return;
        };

        let sel = self.selection(is_queue);
        let target_indices = if sel.is_empty() {
            vec![index]
        } else if sel.contains(&index) {
            sel.to_vec()
        } else {
            vec![index]
        };

        self.context_menu = Some(ContextMenuState {
            visible: true,
            track_index: index,
            target_indices,
            position: (self.drag.cursor_pos.x, self.drag.cursor_pos.y),
            is_youtube: track.source == TrackSource::YouTube,
            is_downloaded: self.download_registry.contains(&track.url),
            in_playlist: self.current_view() == View::Playlist,
            is_queue,
        });
    }

    /// Extracts the context menu state (clearing it), returning `None` if
    /// no menu was open.  This avoids the repeated
    /// `context_menu.as_ref().map(|m| m.is_queue).unwrap_or(false)` pattern.
    pub fn take_context_menu(&mut self) -> Option<ContextMenuState> {
        self.context_menu.take().filter(|m| m.visible)
    }
}
