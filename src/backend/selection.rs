use super::to_slint_track;
use slint::Model;

impl super::Backend {
    pub fn is_selected(&self, index: usize) -> bool {
        self.selected_indices.contains(&index)
    }

    pub fn clear_selection(&mut self) {
        self.selected_indices.clear();
    }

    fn update_selection_row(&mut self, index: usize) {
        let tracks = self.get_current_tracks();
        if index >= tracks.len() {
            return;
        }
        let model = if self.selected_playlist.is_some() {
            self.playlist_model_handle.as_ref()
        } else if self.current_view == super::View::Radio {
            self.radio_model_handle.as_ref()
        } else {
            self.search_model_handle.as_ref()
        };
        if let Some(model) = model {
            if let Some(track) = tracks.get(index) {
                let registry = &self.download_registry;
                let t = to_slint_track(track, registry, self.is_selected(index));
                model.set_row_data(index, t);
            }
        }
    }

    pub fn handle_toggle_select(&mut self, index: usize) {
        let now = std::time::Instant::now();
        let is_double = self.last_click_index == Some(index)
            && now.duration_since(self.last_click_time).as_millis() < 300;

        self.last_click_index = Some(index);
        self.last_click_time = now;

        if is_double {
            self.handle_play_track(index);
        }
        if let Some(pos) = self.selected_indices.iter().position(|&i| i == index) {
            self.selected_indices.remove(pos);
        } else {
            self.selected_indices.push(index);
        }
        self.update_selection_row(index);
        self.update_playback_ui();
    }

    pub fn handle_copy_selected(&mut self) {
        self.clipboard.clear();
        let tracks = self.get_current_tracks();
        for &i in &self.selected_indices {
            if let Some(t) = tracks.get(i) {
                self.clipboard.push(t.clone());
            }
        }
        if !self.clipboard.is_empty() {
            self.notification = Some(format!(
                "Copied {} track{}",
                self.clipboard.len(),
                if self.clipboard.len() == 1 { "" } else { "s" }
            ));
            self.update_ui();
        }
    }

    pub fn handle_delete_selected(&mut self) {
        if self.selected_playlist.is_none() {
            self.notification = Some("Only playlist tracks can be deleted".into());
            self.update_ui();
            return;
        }
        let pl_idx = match self.selected_playlist {
            Some(i) => i,
            None => return,
        };
        let mut indices: Vec<usize> = self.selected_indices.clone();
        indices.sort_unstable_by(|a, b| b.cmp(a));
        for &i in &indices {
            self.playlists.remove_track(pl_idx, i);
        }
        let count = indices.len();
        self.clear_selection();
        self.notification = Some(format!(
            "Deleted {} track{}",
            count,
            if count == 1 { "" } else { "s" }
        ));
        self.update_ui();
    }

    pub fn handle_paste_clipboard(&mut self) {
        if self.clipboard.is_empty() {
            self.notification = Some("Nothing to paste".into());
            self.update_ui();
            return;
        }
        let pl_idx = match self.selected_playlist {
            Some(i) => i,
            None => {
                self.notification = Some("Select a playlist first".into());
                self.update_ui();
                return;
            }
        };
        let count = self.clipboard.len();
        for t in &self.clipboard {
            self.playlists.add_track(pl_idx, t);
        }
        self.notification = Some(format!(
            "Pasted {} track{}",
            count,
            if count == 1 { "" } else { "s" }
        ));
        self.update_ui();
    }
}
