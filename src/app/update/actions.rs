use super::{BackendResult, ContextMenuState, MusicPlayer, TrackSource, View};

impl MusicPlayer {
    pub fn handle_download_track(&mut self, index: usize, is_queue: bool) {
        if let Some(track) = self.get_track_at(index, is_queue) {
            self.downloading_index = Some(index);
            self.notify(format!("Downloading \"{}\"...", track.title));
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
    }

    pub fn handle_remove_download(&mut self, index: usize, is_queue: bool) {
        if let Some(track) = self.get_track_at(index, is_queue) {
            let url = &track.url;
            self.download_registry.remove(url);
        }
    }

    pub fn handle_toggle_picker(&mut self, index: usize) {
        if self.show_playlist_picker == Some(index) {
            self.show_playlist_picker = None;
        } else {
            self.show_playlist_picker = Some(index);
            self.picker_focused_index = 0;
        }
    }

    pub fn show_context_menu(&mut self, index: usize, is_queue: bool) {
        let track = self.get_track_at(index, is_queue);
        let Some(track) = track else {
            return;
        };
        self.context_menu = Some(ContextMenuState {
            visible: true,
            track_index: index,
            position: (self.drag.cursor_pos.x, self.drag.cursor_pos.y),
            is_youtube: track.source == TrackSource::YouTube,
            is_downloaded: self.download_registry.contains(&track.url),
            in_playlist: matches!(self.current_view, View::Playlist),
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
