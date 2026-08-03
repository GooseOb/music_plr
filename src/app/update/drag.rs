use super::*;

impl MusicPlayer {
    pub fn handle_left_release(&mut self) {
        if self.drag_active {
            let is_queue = self.pressed_track_is_queue;
            self.handle_drag_drop(is_queue);
        } else if let Some(track_idx) = self.pressed_track {
            let is_queue = self.pressed_track_is_queue;
            self.toggle_selection(track_idx, is_queue);
        }

        if self.show_search_history && !self.is_cursor_in_search_area() {
            self.show_search_history = false;
        } else if !self.show_search_history
            && self.search_input_bounds().contains(self.cursor_pos)
            && !self.config.search_history.is_empty()
        {
            self.update_search_history();
            self.show_search_history = true;
        }

        self.cleanup_drag_state();
    }

    pub(super) fn cleanup_drag_state(&mut self) {
        self.drag_active = false;
        self.drag_origin = None;
        self.pressed_track = None;
        self.drag_drop_target = None;
        self.sidebar_hover_playlist = None;
    }

    pub fn selection(&self, is_queue: bool) -> &[usize] {
        if is_queue {
            &self.queue_selected_indices
        } else {
            &self.selected_indices
        }
    }

    pub fn selection_mut(&mut self, is_queue: bool) -> &mut Vec<usize> {
        if is_queue {
            &mut self.queue_selected_indices
        } else {
            &mut self.selected_indices
        }
    }

    pub fn handle_drag_update(&mut self) -> Task<Message> {
        let cursor = self.cursor_pos;

        self.sidebar_hover_playlist = None;
        self.drag_drop_target = None;

        if let Some(sidebar_bounds) = self.sidebar_bounds {
            if cursor.x < sidebar_bounds.x + crate::theme::SIDEBAR_WIDTH {
                let y_offset = cursor.y - sidebar_bounds.y;
                if y_offset >= 0.0 {
                    let playlist_idx = ((y_offset + self.sidebar_list_scroll)
                        / crate::theme::SIDEBAR_ITEM_HEIGHT)
                        as usize;
                    if playlist_idx < self.playlists.playlists.len() {
                        self.sidebar_hover_playlist = Some(playlist_idx);
                    }
                }
                return Task::none();
            }
        }

        let is_queue_drag = self.pressed_track_is_queue;
        let (list_bounds, list_scroll, track_count) = if is_queue_drag && self.show_queue {
            match (self.queue_list_bounds, self.queue_list_scroll) {
                (Some(b), s) => (b, s, self.queue.tracks.len()),
                _ => return Task::none(),
            }
        } else {
            match (
                self.get_current_list_bounds(),
                self.get_current_list_scroll(),
            ) {
                (Some(b), s) => (b, s, self.current_track_count(false)),
                (None, _) => return Task::none(),
            }
        };

        let y_offset = cursor.y - list_bounds.y;
        let row_pos = ((y_offset + list_scroll) / crate::theme::ROW_HEIGHT).max(0.0);
        let row_idx = row_pos as usize;
        let drop_idx = if row_idx < track_count && row_pos.fract() >= 0.5 {
            row_idx + 1
        } else {
            row_idx
        };
        let drop_idx = drop_idx.min(track_count);

        let sel = self.selection(is_queue_drag).to_vec();

        if let (Some(min), Some(max)) = (sel.iter().copied().min(), sel.iter().copied().max()) {
            if drop_idx > min && drop_idx < max {
                self.drag_drop_target = None;
                return Task::none();
            }
        }

        self.drag_drop_target = Some(drop_idx);
        self.handle_drag_autoscroll(list_bounds, list_scroll, is_queue_drag, track_count)
    }

    fn handle_drag_autoscroll(
        &self,
        list_bounds: iced::Rectangle,
        current_scroll: f32,
        is_queue: bool,
        track_count: usize,
    ) -> Task<Message> {
        let cursor = self.cursor_pos;
        let y_offset = cursor.y - list_bounds.y;
        let list_height = list_bounds.height;

        let total_height = track_count as f32 * crate::theme::ROW_HEIGHT;
        let max_scroll = (total_height - list_height).max(0.0);

        if max_scroll <= 0.0 {
            return Task::none();
        }

        let edge_zone = crate::theme::DRAG_AUTO_SCROLL_ZONE;
        let scroll_speed = crate::theme::DRAG_AUTO_SCROLL_SPEED;

        let scroll_amount = if y_offset < edge_zone {
            -scroll_speed
        } else if y_offset > list_height - edge_zone {
            scroll_speed
        } else {
            0.0
        };

        if scroll_amount == 0.0 {
            return Task::none();
        }

        let new_scroll = (current_scroll + scroll_amount).clamp(0.0, max_scroll);
        if (new_scroll - current_scroll).abs() < 0.1 {
            return Task::none();
        }

        let list_id = if is_queue {
            iced::widget::Id::new("queue_list")
        } else {
            iced::widget::Id::new("track_list")
        };

        iced::widget::operation::scroll_by::<Message>(
            list_id,
            iced::widget::operation::AbsoluteOffset {
                x: 0.0,
                y: scroll_amount,
            },
        )
    }

    pub fn handle_drag_drop(&mut self, is_queue: bool) {
        let Some(track_idx) = self.pressed_track else {
            return;
        };

        let cursor = self.cursor_pos;

        let was_in_selection = {
            let sel = self.selection(is_queue);
            !sel.is_empty() && sel.contains(&track_idx)
        };

        let indices: Vec<usize> = if was_in_selection {
            self.selection(is_queue).to_vec()
        } else {
            vec![track_idx]
        };

        if let Some(sidebar_bounds) = self.sidebar_bounds {
            if cursor.x < sidebar_bounds.x + crate::theme::SIDEBAR_WIDTH {
                let y_offset = cursor.y - sidebar_bounds.y;
                if y_offset >= 0.0 {
                    let playlist_idx = ((y_offset + self.sidebar_list_scroll)
                        / crate::theme::SIDEBAR_ITEM_HEIGHT)
                        as usize;
                    if playlist_idx < self.playlists.playlists.len() {
                        let mut count = 0;
                        for &i in indices.iter().rev() {
                            if let Some(track) = self.get_track_at(i, is_queue) {
                                let track = track.clone();
                                self.playlists.insert_track_at(playlist_idx, &track, 0);
                                count += 1;
                            }
                        }
                        self.playlists.save();
                        let name = self.playlists.playlists[playlist_idx].name.clone();
                        self.notify(format!(
                            "Added {} track{} to {}",
                            count,
                            if count == 1 { "" } else { "s" },
                            name
                        ));
                        return;
                    }
                }
            }
        }

        if let Some(drop_idx) = self.drag_drop_target {
            let min_idx = *indices.iter().min().unwrap();
            let max_idx = *indices.iter().max().unwrap();
            let is_valid_drop = drop_idx > max_idx || drop_idx < min_idx;

            if is_queue {
                let count = self.queue.tracks.len();
                if drop_idx <= count && is_valid_drop {
                    let new_positions = self.handle_reorder_queue(drop_idx, &indices);
                    if was_in_selection {
                        let sel = self.selection_mut(is_queue);
                        *sel = new_positions;
                    }
                    self.save_session();
                }
            } else {
                let count = self.current_track_count(false);
                if drop_idx <= count && is_valid_drop {
                    let new_positions = self.handle_reorder_tracks_selected(drop_idx, &indices);
                    if was_in_selection {
                        self.selected_indices = new_positions;
                    }
                }
            }
        }
    }

    pub fn handle_track_pressed(&mut self, index: usize, is_queue: bool) {
        let now = std::time::Instant::now();
        let is_double = self.last_click_index == Some(index)
            && now.duration_since(self.last_click_time).as_millis() < DOUBLE_CLICK_MS;

        self.last_click_index = Some(index);
        self.last_click_time = now;
        self.pressed_track = Some(index);
        self.pressed_track_is_queue = is_queue;
        self.drag_origin = Some(self.cursor_pos);
        self.drag_active = false;

        if is_double {
            self.pressed_track = None;
            self.drag_origin = None;
            self.handle_play_track(index, is_queue);
            self.toggle_selection(index, is_queue);
        }
    }

    pub fn toggle_selection(&mut self, index: usize, is_queue: bool) {
        let sel = self.selection_mut(is_queue);
        if let Some(pos) = sel.iter().position(|&i| i == index) {
            sel.remove(pos);
        } else {
            sel.push(index);
        }
    }

    pub fn clear_selection(&mut self) {
        self.selected_indices.clear();
        self.queue_selected_indices.clear();
        self.show_playlist_picker = None;
    }

    pub fn get_track_at(&self, index: usize, is_queue: bool) -> Option<Track> {
        if is_queue {
            return self.queue.tracks.get(index).cloned();
        }
        match &self.current_view {
            View::Search => self.search_results.get(index).cloned(),
            View::SongRadio | View::ArtistRadio => self.radio_tracks.get(index).cloned(),
            View::Playlist | View::Downloads => self
                .selected_playlist
                .and_then(|sp| self.playlists.playlists.get(sp))
                .and_then(|p| p.tracks.get(index))
                .cloned(),
        }
    }

    pub fn current_track_count(&self, is_queue: bool) -> usize {
        if is_queue {
            return self.queue.tracks.len();
        }
        match &self.current_view {
            View::Search => self.search_results.len(),
            View::SongRadio | View::ArtistRadio => self.radio_tracks.len(),
            View::Playlist | View::Downloads => self
                .selected_playlist
                .and_then(|sp| self.playlists.playlists.get(sp))
                .map(|p| p.tracks.len())
                .unwrap_or(0),
        }
    }

    pub fn get_current_list_bounds(&self) -> Option<iced::Rectangle> {
        if self.current_view.is_search_like() {
            self.search_list_bounds
        } else {
            self.playlist_list_bounds
        }
    }

    pub fn get_current_list_scroll(&self) -> f32 {
        if self.current_view.is_search_like() {
            self.search_list_scroll
        } else {
            self.playlist_list_scroll
        }
    }
}
