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
            iced::keyboard::Key::Named(Named::ArrowUp) => {
                if self.focused_list_index > 0 {
                    self.focused_list_index -= 1;
                }
            }
            iced::keyboard::Key::Named(Named::ArrowDown) => {
                let count = self.current_track_count(false);
                if self.focused_list_index < count.saturating_sub(1) {
                    self.focused_list_index += 1;
                }
            }
            iced::keyboard::Key::Named(Named::Enter) => {
                let count = self.current_track_count(false);
                if self.focused_list_index < count {
                    self.handle_play_track(self.focused_list_index, false);
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
}
