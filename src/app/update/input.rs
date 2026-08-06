use super::{MusicPlayer, View};

impl MusicPlayer {
    pub fn search_input_geometry(&self) -> (f32, f32) {
        let queue_width = if self.show_queue {
            (self.window_width * crate::theme::QUEUE_WIDTH_RATIO).max(crate::theme::QUEUE_MIN_WIDTH)
        } else {
            0.0
        };
        let main_width = (self.window_width - crate::theme::SIDEBAR_WIDTH - queue_width).max(0.0);
        let input_x = crate::theme::SPACING_XL;
        let input_width = (2.0f32.mul_add(-crate::theme::SPACING_XL, main_width)
            - crate::theme::SEARCH_BTN_SIZE
            - crate::theme::SPACING_SM)
            .max(100.0);
        (input_x, input_width)
    }

    pub fn search_input_bounds(&self) -> iced::Rectangle {
        let (input_x, input_width) = self.search_input_geometry();
        iced::Rectangle {
            x: crate::theme::SIDEBAR_WIDTH + input_x,
            y: 0.0,
            width: input_width,
            height: crate::theme::SEARCH_BAR_HEIGHT,
        }
    }

    pub fn search_dropdown_bounds(&self) -> iced::Rectangle {
        let input_bounds = self.search_input_bounds();
        iced::Rectangle {
            x: input_bounds.x,
            y: input_bounds.y + input_bounds.height,
            width: input_bounds.width,
            height: crate::theme::SEARCH_DROPDOWN_MAX_HEIGHT,
        }
    }

    pub fn is_cursor_in_search_area(&self) -> bool {
        let cursor = self.drag.cursor_pos;
        if self.search_input_bounds().contains(cursor) {
            return true;
        }
        self.search_dropdown_bounds().contains(cursor)
    }

    pub fn handle_key_press(
        &mut self,
        key: &iced::keyboard::key::Key,
        modifiers: iced::keyboard::Modifiers,
    ) {
        use iced::keyboard::key::Named;
        match key {
            iced::keyboard::Key::Named(Named::Space) => {
                self.toggle_play_pause();
            }
            iced::keyboard::Key::Named(Named::Escape) => {
                if self.show_search_history {
                    self.show_search_history = false;
                } else if self.selected_indices.is_empty() {
                    self.handle_navigate_to(View::Search);
                } else {
                    self.clear_selection();
                }
            }
            iced::keyboard::Key::Named(Named::Delete) => {
                if self.selected_playlist.is_some() {
                    self.handle_delete_selected();
                }
            }
            iced::keyboard::Key::Named(Named::Tab) => {
                self.toggle_keyboard_list();
            }
            iced::keyboard::Key::Named(Named::ArrowUp) => {
                let is_queue = self.is_queue_hovered();
                let count = self.list_size(is_queue);
                if count == 0 {
                    return;
                }

                let first_idx = self.list_first_index(is_queue);

                if let Some((idx, _)) = self.drag.hovered_track {
                    let new_idx = idx.saturating_sub(1);
                    if new_idx >= first_idx {
                        self.drag.hovered_track = Some((new_idx, is_queue));
                    }
                } else {
                    self.drag.hovered_track = Some((first_idx, is_queue));
                };
            }
            iced::keyboard::Key::Named(Named::ArrowDown) => {
                let is_queue = self.is_queue_hovered();
                let count = self.list_size(is_queue);
                if count == 0 {
                    return;
                }

                if let Some((idx, _)) = self.drag.hovered_track {
                    let new_idx = idx + 1;
                    if new_idx < count {
                        self.drag.hovered_track = Some((new_idx, is_queue));
                    }
                } else {
                    self.drag.hovered_track = Some((self.list_first_index(is_queue), is_queue));
                };
            }
            iced::keyboard::Key::Named(Named::Enter) => {
                if let Some((index, is_queue)) = self.drag.hovered_track {
                    if is_queue
                        && matches!(self.queue.queue_tab, crate::types::QueueTab::RecentlyPlayed)
                    {
                        if let Some(track) = self.queue.recently_played.get(index).cloned() {
                            self.play_recent_track(track);
                        }
                    } else {
                        self.handle_play_track(index, is_queue);
                    }
                }
            }
            _ => {}
        }
        if modifiers.control() || modifiers.logo() {
            match key {
                iced::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("c") => {
                    self.handle_copy_selected();
                }
                iced::keyboard::Key::Character(c) if c.eq_ignore_ascii_case("v") => {
                    self.handle_paste_clipboard();
                }
                _ => {}
            }
        }
    }

    fn toggle_keyboard_list(&mut self) {
        if !self.show_queue {
            return;
        }

        let target_is_queue = !self.is_queue_hovered();
        if self.list_size(target_is_queue) == 0 {
            return;
        }
        let first_idx = self.list_first_index(target_is_queue);
        self.drag.hovered_track = Some((first_idx, target_is_queue));
    }

    fn is_queue_hovered(&self) -> bool {
        self.drag
            .hovered_track
            .is_some_and(|(_, is_queue)| is_queue)
    }

    fn list_size(&self, is_queue: bool) -> usize {
        if is_queue {
            match self.queue.queue_tab {
                crate::types::QueueTab::Queue => self.queue.tracks.len(),
                crate::types::QueueTab::RecentlyPlayed => self.queue.recently_played.len(),
            }
        } else {
            self.current_track_count(false)
        }
    }

    fn list_first_index(&self, is_queue: bool) -> usize {
        if is_queue && matches!(self.queue.queue_tab, crate::types::QueueTab::Queue) {
            1
        } else {
            0
        }
    }
}
