use super::{Message, MusicPlayer, Task, Track, TrackListKind, TrackPos, DOUBLE_CLICK_MS};

impl MusicPlayer {
    fn cursor_in_sidebar(&self) -> bool {
        match self.bounds.sidebar {
            Some(b) => self.drag.cursor_pos.x < b.bounds.x + crate::theme::SIDEBAR_WIDTH,
            None => self.drag.cursor_pos.x < crate::theme::SIDEBAR_WIDTH,
        }
    }

    fn sidebar_playlist_at_cursor(&self) -> Option<usize> {
        if self.drag.cursor_pos.x >= crate::theme::SIDEBAR_WIDTH {
            return None;
        }
        let sidebar = self.bounds.sidebar?;
        let y_start = sidebar.bounds.y;
        let y_offset = self.drag.cursor_pos.y - y_start;
        if y_offset < 0.0 {
            return None;
        }
        let playlist_idx = ((y_offset + sidebar.translation_y)
            / (crate::theme::SIDEBAR_ITEM_HEIGHT + 2.0)) as usize;
        if playlist_idx < self.playlists.playlists.len() {
            Some(playlist_idx)
        } else {
            None
        }
    }

    pub fn handle_left_release(&mut self) {
        let Some(pressed) = self.drag.pressed_track else {
            self.cleanup_drag_state();
            return;
        };
        if self.drag.drag_active {
            self.handle_drag_drop(pressed.list);
        } else {
            self.toggle_selection(pressed);
        }

        if self.show_search_history && !self.is_cursor_in_search_area() {
            self.show_search_history = false;
        } else if !self.show_search_history
            && self.search_input_bounds().contains(self.drag.cursor_pos)
            && !self.search_history.get().is_empty()
        {
            self.update_search_history();
            self.show_search_history = true;
        }

        self.cleanup_drag_state();
    }

    pub(super) fn cleanup_drag_state(&mut self) {
        self.drag.cleanup();
    }

    pub fn handle_drag_update(&mut self) -> Task<Message> {
        self.drag.sidebar_hover_playlist = None;
        self.drag.drag_drop_target = None;
        self.drag.drag_target_list = None;

        if self.cursor_in_sidebar() {
            self.drag.sidebar_hover_playlist = self.sidebar_playlist_at_cursor();
            return Task::none();
        }

        let Some(source) = self.drag.pressed_track.map(|p| p.list) else {
            return Task::none();
        };

        // Check if cursor is over the queue list (for cross-list copy to queue).
        if self.show_queue {
            if let Some(q) = self.bounds.queue {
                if q.bounds.contains(self.drag.cursor_pos) {
                    let target = TrackListKind::Queue;
                    self.drag.drag_target_list = Some(target);
                    let drop_idx = self.compute_drop_idx(q.bounds, q.translation_y, target);
                    self.drag.drag_drop_target = Some(drop_idx);
                    return self.handle_drag_autoscroll(q.bounds, q.translation_y, target);
                }
            }
        }

        // Check if cursor is over the current track list (for same-list reorder
        // or copy from queue to track list). Track bounds come from the live
        // capture; the scroll offset stays on `view_data` (persisted per slot).
        if let Some(t) = self.bounds.track {
            let ls = self.view_data().scroll;
            let lb = t.bounds;
            if lb.contains(self.drag.cursor_pos) {
                let target = TrackListKind::Active;
                self.drag.drag_target_list = Some(target);
                let drop_idx = self.compute_drop_idx(lb, ls, target);
                self.drag.drag_drop_target = Some(drop_idx);

                // For same-list drags, prevent dropping onto a selected item
                // that would shift it (reorder guard). Skip this check for
                // cross-list copies where every drop position is valid.
                if source == target {
                    let sel = self.selection(target).to_vec();
                    if let (Some(min), Some(max)) =
                        (sel.iter().copied().min(), sel.iter().copied().max())
                    {
                        if drop_idx > min && drop_idx < max {
                            self.drag.drag_drop_target = None;
                            self.drag.drag_target_list = None;
                            return Task::none();
                        }
                    }
                }

                return self.handle_drag_autoscroll(lb, ls, target);
            }
        }

        Task::none()
    }

    /// Track count minus rows drawn outside the scrollable (the queue's
    /// now-playing header).
    fn visible_row_count(&self, list: TrackListKind) -> usize {
        self.track_count(list) - list.first_index().min(self.track_count(list))
    }

    /// Returns an index in `list`'s own space, with the queue's now-playing
    /// offset folded back in.
    fn compute_drop_idx(
        &self,
        list_bounds: iced::Rectangle,
        list_scroll: f32,
        list: TrackListKind,
    ) -> usize {
        let y_offset = self.drag.cursor_pos.y - list_bounds.y;
        let row_pos = ((y_offset + list_scroll) / crate::theme::ROW_HEIGHT).max(0.0);
        let row_idx = row_pos as usize;
        let row_count = self.visible_row_count(list);
        let drop_idx = if row_idx < row_count && row_pos.fract() >= 0.5 {
            row_idx + 1
        } else {
            row_idx
        };
        drop_idx.min(row_count) + list.first_index()
    }

    fn handle_drag_autoscroll(
        &self,
        list_bounds: iced::Rectangle,
        current_scroll: f32,
        list: TrackListKind,
    ) -> Task<Message> {
        let track_count = self.visible_row_count(list);
        let cursor = self.drag.cursor_pos;
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

        iced::widget::operation::scroll_by::<Message>(
            list.scrollable_id(),
            iced::widget::operation::AbsoluteOffset {
                x: 0.0,
                y: scroll_amount,
            },
        )
    }

    pub fn handle_drag_drop(&mut self, source: TrackListKind) {
        let Some(pressed) = self.drag.pressed_track else {
            return;
        };
        let track_idx = pressed.index;

        let was_in_selection = {
            let sel = self.selection(source);
            !sel.is_empty() && sel.contains(&track_idx)
        };

        let indices: Vec<usize> = if was_in_selection {
            self.selection(source).to_vec()
        } else {
            vec![track_idx]
        };

        if self.cursor_in_sidebar() {
            if let Some(playlist_idx) = self.sidebar_playlist_at_cursor() {
                let tracks: Vec<Track> = indices
                    .iter()
                    .rev()
                    .filter_map(|&i| self.get_track_at(TrackPos::new(i, source)))
                    .collect();
                let count = tracks.len();
                if count > 0 {
                    self.playlists.insert_tracks_at(playlist_idx, &tracks, 0);
                }
                let name = self.playlists.playlists[playlist_idx].name.clone();
                self.notify_tracks("Added", count, &format!("to {name}"));
                return;
            }
        }

        let Some(drop_idx) = self.drag.drag_drop_target else {
            return;
        };

        // Determine if this is a cross-list copy or a same-list reorder.
        match self.drag.drag_target_list {
            Some(target) if target == source => {
                self.handle_same_list_reorder(drop_idx, &indices, source);
            }
            Some(TrackListKind::Queue) => self.copy_to_queue(&indices, drop_idx),
            Some(TrackListKind::Active) => self.copy_from_queue(&indices, drop_idx),
            _ => {}
        }
    }

    fn copy_to_queue(&mut self, indices: &[usize], drop_idx: usize) {
        let clamped = drop_idx.min(self.queue.tracks.len());
        let tracks: Vec<Track> = indices
            .iter()
            .filter_map(|&i| self.get_track_at(TrackPos::new(i, TrackListKind::Active)))
            .collect();
        let inserted = tracks.len();
        for (j, track) in tracks.into_iter().enumerate() {
            self.queue.tracks.insert(clamped + j, track);
        }
        self.save_session();
        self.notify_tracks("Added", inserted, "to queue");
    }

    /// Insert tracks from the queue into the current playlist at the given
    /// drop index. `indices` are positions in the queue's up-next list
    /// (starting after index 0, i.e. the current track).
    fn copy_from_queue(&mut self, indices: &[usize], drop_idx: usize) {
        let Some(sp) = self.view_data_mut().selected_playlist_id() else {
            if !self.view_data_mut().is_search_like() {
                self.notify("Select a playlist to drop tracks into");
            }
            return;
        };
        if sp >= self.playlists.playlists.len() {
            return;
        }

        let clamped = drop_idx.min(self.playlists.playlists[sp].tracks.len());
        let tracks: Vec<Track> = indices
            .iter()
            .filter_map(|&queue_idx| self.queue.tracks.get(queue_idx).cloned())
            .collect();
        let inserted = tracks.len();
        if inserted > 0 {
            self.playlists.insert_tracks_at(sp, &tracks, clamped);
        }
        self.save_session();
        let name = self.playlists.playlists[sp].name.clone();
        self.notify_tracks("Added", inserted, &format!("to {name}"));
    }

    /// Handle reordering within the same list. The selection is always
    /// remapped to reflect the new positions of all selected tracks — both
    /// the moved ones and any that merely shifted.
    fn handle_same_list_reorder(
        &mut self,
        drop_idx: usize,
        indices: &[usize],
        source: TrackListKind,
    ) {
        let min_idx = *indices.iter().min().unwrap();
        let max_idx = *indices.iter().max().unwrap();
        let is_valid_drop = drop_idx > max_idx || drop_idx < min_idx;
        if drop_idx > self.track_count(source) || !is_valid_drop {
            return;
        }

        match source {
            TrackListKind::Queue => {
                let selection = self.queue_selected_indices.clone();
                self.queue_selected_indices =
                    self.handle_reorder_queue(drop_idx, indices, &selection);
                self.save_session();
            }
            TrackListKind::Active => {
                let selection = self.view_data_mut().selection.clone();
                self.view_data_mut().selection =
                    self.handle_reorder_tracks_selected(drop_idx, indices, &selection);
            }
            TrackListKind::Recent => {}
        }
    }

    pub fn handle_track_pressed(&mut self, pos: TrackPos) {
        let now = std::time::Instant::now();
        // Comparing the full position, not just the index, keeps a click in
        // one list from completing a double-click begun in another.
        let is_double = self.last_click == Some(pos)
            && now.duration_since(self.last_click_time).as_millis() < DOUBLE_CLICK_MS;

        self.last_click = Some(pos);
        self.last_click_time = now;
        // Read-only lists neither select nor drag, so they never arm the
        // press state that `CursorMoved` promotes into a drag.
        if pos.list.is_interactive() {
            self.drag.pressed_track = Some(pos);
            self.drag.drag_origin = Some(self.drag.cursor_pos);
        }
        self.drag.drag_active = false;

        if is_double {
            self.drag.pressed_track = None;
            self.drag.drag_origin = None;
            self.handle_play_track(pos);
            self.toggle_selection(pos);
        }
    }
}
