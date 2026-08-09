use super::{BackendResult, ContextMenuState, MusicPlayer, Track, TrackSource};
use crate::app::interaction::TrackListKind;
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
            let track = self.track_at(list, idx);
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
        let sel = self.selection(list.is_queue());
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
        self.picker = if self.picker.is_some() {
            None
        } else {
            Some(PlaylistPicker { indices, is_queue })
        }
    }

    pub fn show_context_menu(&mut self, index: usize, list: TrackListKind) {
        let Some(track) = self.track_at(list, index) else {
            return;
        };

        let sel = if list == TrackListKind::Recent {
            &[][..]
        } else {
            self.selection(list.is_queue())
        };
        let target_indices = if sel.contains(&index) {
            sel.to_vec()
        } else {
            vec![index]
        };

        self.context_menu = Some(ContextMenuState {
            track_index: index,
            target_indices,
            position: (self.drag.cursor_pos.x, self.drag.cursor_pos.y),
            is_youtube: track.source == TrackSource::YouTube,
            is_downloaded: self.download_registry.contains(&track.url),
            in_playlist: matches!(self.view_data_mut().kind, ViewKind::Playlist { .. }),
            list,
            track,
        });
    }

    /// Resolve the track a context-menu op targets from whichever list it
    /// lives in.
    fn track_at(&self, list: TrackListKind, index: usize) -> Option<Track> {
        match list {
            TrackListKind::Queue => self.get_track_at(index, true),
            TrackListKind::Active => self.get_track_at(index, false),
            TrackListKind::Recent => self.queue.recently_played.get(index).cloned(),
        }
    }
}
