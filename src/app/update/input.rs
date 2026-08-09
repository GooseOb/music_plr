use super::{Message, MusicPlayer, Task, ViewData};

impl MusicPlayer {
    pub fn search_input_geometry(&self) -> (f32, f32) {
        let queue_width = if self.show_queue {
            (self.window_width * crate::theme::QUEUE_WIDTH_RATIO).max(crate::theme::QUEUE_MIN_WIDTH)
        } else {
            0.0
        };
        let main_width = (self.window_width - crate::theme::SIDEBAR_WIDTH - queue_width).max(0.0);
        let input_x = crate::theme::SPACING_XL;
        let input_width = (main_width
            - 2.0 * crate::theme::SPACING_XL
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
        let count = self.last_filtered_history.len();
        let height = if count == 0 {
            crate::theme::SEARCH_HISTORY_ITEM_HEIGHT
        } else {
            (count as f32 * crate::theme::SEARCH_HISTORY_ITEM_HEIGHT)
                .min(crate::theme::SEARCH_DROPDOWN_MAX_HEIGHT)
        };
        iced::Rectangle {
            x: input_bounds.x,
            y: input_bounds.y + input_bounds.height,
            width: input_bounds.width,
            height,
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
    ) -> Task<Message> {
        use iced::keyboard::key::Named;
        let task = match key {
            iced::keyboard::Key::Named(Named::Space) => {
                self.toggle_play_pause();
                Task::none()
            }
            iced::keyboard::Key::Named(Named::Escape) => {
                if self.show_search_history {
                    self.show_search_history = false;
                } else if self.view_data_mut().selection.is_empty() {
                    self.handle_navigate_to(ViewData::new_search(String::new(), self.search_scope));
                } else {
                    self.clear_selection();
                }
                Task::none()
            }
            iced::keyboard::Key::Named(Named::Delete) => {
                if self.view_data_mut().selected_playlist_id().is_some() {
                    self.handle_delete_selected();
                }
                Task::none()
            }
            iced::keyboard::Key::Named(Named::Tab) => self.toggle_keyboard_list(),
            iced::keyboard::Key::Named(Named::ArrowUp) => {
                let is_queue = self.is_queue_hovered();
                let count = self.list_size(is_queue);
                if count == 0 {
                    Task::none()
                } else {
                    let first_idx = self.list_first_index(is_queue);

                    if let Some((idx, _)) = self.drag.hovered_track {
                        let new_idx = idx.saturating_sub(1);
                        if new_idx >= first_idx {
                            self.drag.hovered_track = Some((new_idx, is_queue));
                        }
                    } else {
                        self.drag.hovered_track = Some((first_idx, is_queue));
                    }
                    self.scroll_hovered_track_into_view()
                }
            }
            iced::keyboard::Key::Named(Named::ArrowDown) => {
                let is_queue = self.is_queue_hovered();
                let count = self.list_size(is_queue);
                if count == 0 {
                    Task::none()
                } else {
                    if let Some((idx, _)) = self.drag.hovered_track {
                        let new_idx = idx + 1;
                        if new_idx < count {
                            self.drag.hovered_track = Some((new_idx, is_queue));
                        }
                    } else {
                        self.drag.hovered_track = Some((self.list_first_index(is_queue), is_queue));
                    }
                    self.scroll_hovered_track_into_view()
                }
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
                Task::none()
            }
            _ => Task::none(),
        };

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
        task
    }

    fn toggle_keyboard_list(&mut self) -> Task<Message> {
        if !self.show_queue {
            return Task::none();
        }

        let target_is_queue = !self.is_queue_hovered();
        if self.list_size(target_is_queue) == 0 {
            return Task::none();
        }
        let first_idx = self.list_first_index(target_is_queue);
        self.drag.hovered_track = Some((first_idx, target_is_queue));
        self.scroll_hovered_track_into_view()
    }

    /// Scroll the hovered track into view if it's outside the visible
    /// viewport of the current scrollable list. Returns a `scroll_to` task.
    pub(super) fn scroll_hovered_track_into_view(&self) -> Task<Message> {
        let Some((index, is_queue)) = self.drag.hovered_track else {
            return Task::none();
        };

        let (bounds, scroll_offset) = if is_queue {
            (
                self.bounds.queue.map(|g| g.bounds),
                self.bounds.queue.map_or(0.0, |g| g.translation_y),
            )
        } else {
            (self.bounds.track.map(|g| g.bounds), self.view_data().scroll)
        };

        let Some(bounds) = bounds else {
            return Task::none();
        };

        // For the queue tab (Up Next), the visible list starts after the
        // "now playing" track (offset 1), so the visual position of track
        // `index` is `(index - 1) * ROW_HEIGHT`.
        let visual_index =
            if is_queue && matches!(self.queue.queue_tab, crate::types::QueueTab::Queue) {
                index.saturating_sub(1)
            } else {
                index
            };

        let row_y = visual_index as f32 * crate::theme::ROW_HEIGHT;
        let row_bottom = row_y + crate::theme::ROW_HEIGHT;

        if row_y < scroll_offset {
            // Item is above the viewport — scroll up to reveal it.
            iced::widget::operation::scroll_to::<Message>(
                if is_queue {
                    crate::app::ui::QUEUE_LIST_ID
                } else {
                    crate::app::ui::TRACK_LIST_ID
                },
                iced::widget::operation::AbsoluteOffset { x: 0.0, y: row_y },
            )
        } else if row_bottom > scroll_offset + bounds.height {
            // Item is below the viewport — scroll down to reveal it.
            let target = (row_bottom - bounds.height).max(0.0);
            iced::widget::operation::scroll_to::<Message>(
                if is_queue {
                    crate::app::ui::QUEUE_LIST_ID
                } else {
                    crate::app::ui::TRACK_LIST_ID
                },
                iced::widget::operation::AbsoluteOffset { x: 0.0, y: target },
            )
        } else {
            Task::none()
        }
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
        usize::from(is_queue && matches!(self.queue.queue_tab, crate::types::QueueTab::Queue))
    }
}
