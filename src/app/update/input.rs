use iced::widget::operation;

use super::{Message, MusicPlayer, Task, Track, TrackListKind, TrackPos, ViewData};
use crate::app::FloatingSearch;

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
            iced::keyboard::Key::Character(c)
                if c.eq_ignore_ascii_case("f") && (modifiers.control() || modifiers.logo()) =>
            {
                self.open_floating_search()
            }
            iced::keyboard::Key::Named(Named::Space) => {
                self.toggle_play_pause();
                Task::none()
            }
            iced::keyboard::Key::Named(Named::Escape) => {
                if self.floating_search.is_some() {
                    self.floating_search = None;
                } else if self.show_search_history {
                    self.show_search_history = false;
                } else if let Some(hovered) = self.drag.hovered_track() {
                    self.clear_selection_for(hovered.list);
                } else if self.has_selection() {
                    self.clear_selection();
                } else {
                    self.handle_navigate_to(ViewData::new_search(
                        String::new(),
                        self.search_provider,
                        self.search_scope,
                    ));
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
                if self.floating_search.is_some() {
                    return self.handle_floating_search_step(-1);
                }
                self.step_hovered_track(-1)
            }
            iced::keyboard::Key::Named(Named::ArrowDown) => {
                if self.floating_search.is_some() {
                    return self.handle_floating_search_step(1);
                }
                self.step_hovered_track(1)
            }
            iced::keyboard::Key::Named(Named::Enter) => {
                if let Some(hovered) = self.drag.hovered_track() {
                    self.handle_play_track(hovered);
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

        let target = if self.hovered_list().in_queue_panel() {
            TrackListKind::Active
        } else {
            self.queue.queue_tab.into()
        };
        if self.track_count(target) == 0 {
            return Task::none();
        }
        self.move_hovered(TrackPos::new(target.first_index(), target))
    }

    /// Scroll `pos` into view of its list. `center` forces the row to the
    /// middle of the viewport; otherwise the list only scrolls when `pos` is
    /// outside the visible viewport (reveal).
    fn scroll_track_into_view(&self, pos: TrackPos) -> Task<Message> {
        let TrackPos { index, list } = pos;

        let bounds = if list.in_queue_panel() {
            self.bounds.queue.as_ref().map(|g| g.bounds)
        } else {
            self.bounds.track.as_ref().map(|g| g.bounds)
        };

        let Some(bounds) = bounds else {
            return Task::none();
        };

        // The queue's now-playing track renders in its own header, so the
        // scrollable's rows are shifted down by `first_index`.
        let visual_index = index - list.first_index().min(index);
        let row_y = visual_index as f32 * crate::theme::ROW_HEIGHT;

        // `scroll_to` with `AbsoluteOffset` sets the scroll position
        // directly, so center the row within the viewport height.
        let absolute = (row_y + crate::theme::ROW_HEIGHT / 2.0 - bounds.height / 2.0).max(0.0);
        operation::scroll_to::<Message>(
            list,
            operation::AbsoluteOffset {
                x: 0.0,
                y: absolute,
            },
        )
    }

    /// Move the hovered track by `dir` (-1 up, +1 down) within its list, looping
    /// at both edges, and center it. Starts from the first row when nothing is
    /// hovered yet.
    fn step_hovered_track(&mut self, dir: isize) -> Task<Message> {
        let list = self.hovered_list();
        let count = self.track_count(list);
        if count == 0 {
            return Task::none();
        }
        let first = list.first_index();
        let new_idx = match self.drag.hovered_track() {
            Some(h) if h.list == list => {
                let span = count - first;
                ((h.index - first).cast_signed() + dir).rem_euclid(span.cast_signed()) as usize
                    + first
            }
            _ => first,
        };
        self.move_hovered(TrackPos::new(new_idx, list))
    }

    /// Set the hovered track and center it in its list.
    fn move_hovered(&mut self, pos: TrackPos) -> Task<Message> {
        self.drag.is_hover_controlled = true;
        self.drag.set_hovered_track(pos);
        self.scroll_track_into_view(pos)
    }

    fn hovered_list(&self) -> TrackListKind {
        self.drag
            .hovered_track()
            .map_or(TrackListKind::Active, |h| h.list)
    }

    pub(crate) fn open_floating_search(&mut self) -> Task<Message> {
        let Some(pos) = self.drag.hovered_track() else {
            return Task::none();
        };
        if !pos.list.is_interactive() {
            return Task::none();
        }
        let list = pos.list;
        let matches: Vec<usize> = (0..self.track_count(list)).collect();
        self.floating_search = Some(FloatingSearch {
            list,
            query: String::new(),
            matches,
        });
        // Anchor the hovered track to the closest match so there is a current
        // occurrence immediately (and scroll it into view).
        let from = pos.index;
        let anchored = self.closest_match(from).unwrap_or(from);
        Task::batch([
            self.move_hovered(TrackPos::new(anchored, list)),
            operation::focus::<Message>(crate::app::ui::floating_search::FLOATING_SEARCH_ID),
        ])
    }

    /// The matched track index nearest to `from` (by absolute row distance,
    /// ties resolved to the smaller index), or `None` when there are no
    /// matches.
    fn closest_match(&self, from: usize) -> Option<usize> {
        let fs = self.floating_search.as_ref()?;
        let mut best: Option<(usize, usize)> = None;
        for &m in &fs.matches {
            let dist = m.abs_diff(from);
            match best {
                Some((b_dist, b_idx)) if b_dist < dist || (b_dist == dist && b_idx < m) => {}
                _ => best = Some((dist, m)),
            }
        }
        best.map(|(_, idx)| idx)
    }

    /// Recompute the match set for the active floating search against the
    /// live query. The hovered track is re-anchored to the closest match
    /// (kept as-is when it still matches) so the current occurrence follows
    /// the query, and the new current is scrolled into view.
    pub(crate) fn handle_floating_search_input(&mut self, query: &str) -> Task<Message> {
        let list = match &self.floating_search {
            Some(fs) => fs.list,
            None => return Task::none(),
        };
        let tracks: &[Track] = match list {
            TrackListKind::Queue => &self.queue.tracks,
            TrackListKind::Active => self.view_tracks(),
            TrackListKind::Recent => self.queue.recently_played.as_slices().0,
        };
        let matches: Vec<usize> = tracks
            .iter()
            .enumerate()
            .filter(|(_, t)| {
                crate::util::fuzzy_match(query, &t.title)
                    || crate::util::fuzzy_match(query, &t.artist.name)
            })
            .map(|(i, _)| i)
            .collect();
        let fs = self.floating_search.as_mut().expect("checked above");
        fs.query = query.to_string();
        fs.matches = matches;
        let from = match self.drag.hovered_track() {
            Some(h) if h.list == list => h.index,
            _ => 0,
        };
        let anchored = self.closest_match(from).unwrap_or(from);
        self.move_hovered(TrackPos::new(anchored, list))
    }

    /// Move the hovered track to the next (`dir = 1`) or previous (`dir = -1`)
    /// match relative to its current row, wrapping around the match list.
    /// The hovered track is the current occurrence, so this is how the user
    /// walks between matches; the new current is scrolled into view.
    pub(crate) fn handle_floating_search_step(&mut self, dir: isize) -> Task<Message> {
        let Some(fs) = self.floating_search.as_ref() else {
            return Task::none();
        };
        if fs.matches.len() <= 1 {
            return Task::none();
        }
        let from = match self.drag.hovered_track() {
            Some(h) if h.list == fs.list => h.index,
            _ => 0,
        };
        let target = {
            let back = dir < 0;
            let mut lo = 0usize;
            let mut hi = fs.matches.len();
            while lo < hi {
                let mid = lo + (hi - lo) / 2;
                // forward: skip the exact match; backward: keep it (step back)
                if fs.matches[mid] < from || (!back && fs.matches[mid] == from) {
                    lo = mid + 1; // match is at or before `from`
                } else {
                    hi = mid; // match is after `from`
                }
            }
            if dir < 0 {
                // previous match; wrap to last past the start
                *fs.matches
                    .get(lo.wrapping_sub(1))
                    .unwrap_or_else(|| fs.matches.last().unwrap())
            } else {
                // next match; wrap to first past the end
                fs.matches.get(lo).copied().unwrap_or(fs.matches[0])
            }
        };
        self.move_hovered(TrackPos::new(target, fs.list))
    }
}
