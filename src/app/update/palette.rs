//! Palette lifecycle: opening it (full, playlist picker or add-to-playlist),
//! filtering, keyboard stepping and running the selected row.

use iced::widget::operation;

use super::{Message, MusicPlayer, Task};
use crate::app::{
    palette::{Palette, PaletteItem, PaletteMode, PaletteRow, PendingAdd},
    Dialog, ViewKind,
};

impl MusicPlayer {
    pub fn open_palette(&mut self) -> Task<Message> {
        self.open_palette_mode(PaletteMode::All)
    }

    pub fn open_playlist_picker(&mut self) -> Task<Message> {
        self.open_palette_mode(PaletteMode::Playlists)
    }

    fn open_palette_mode(&mut self, mode: PaletteMode) -> Task<Message> {
        self.drag.claim_keyboard_hover();
        self.dialog = Some(Dialog::Palette(Palette {
            mode,
            ..Default::default()
        }));
        operation::focus::<Message>(crate::app::ui::palette_input_id())
    }

    pub fn open_playlist_add(&mut self, pending: PendingAdd) -> Task<Message> {
        self.open_palette_mode(PaletteMode::AddToPlaylist(pending))
    }

    /// The live rows, recomputed from the palette's query and the stores it
    /// searches. The view and the keyboard handlers both go through here so
    /// the selection index can't drift from what is drawn.
    pub(crate) fn palette_rows(&self) -> Vec<PaletteRow> {
        match &self.dialog {
            Some(Dialog::Palette(palette)) => palette.rows(self),
            _ => Vec::new(),
        }
    }

    pub fn step_palette(&mut self, dir: isize) -> Task<Message> {
        let len = self.palette_rows().len();
        if len == 0 {
            return Task::none();
        }
        self.drag.claim_keyboard_hover();
        if let Some(Dialog::Palette(palette)) = &mut self.dialog {
            palette.selected = ((palette.selected.cast_signed() + dir)
                .rem_euclid(len.cast_signed()))
            .cast_unsigned();
        }
        self.scroll_palette_to_selected()
    }

    pub fn move_palette_to(&mut self, first: bool) -> Task<Message> {
        let len = self.palette_rows().len();
        if len == 0 {
            return Task::none();
        }
        self.drag.claim_keyboard_hover();
        if let Some(Dialog::Palette(palette)) = &mut self.dialog {
            palette.selected = if first { 0 } else { len - 1 };
        }
        self.scroll_palette_to_selected()
    }

    /// Replace the query and reset the selection to the first row. Typing owns
    /// the selection, so scroll echoes under the stationary cursor can't move
    /// it.
    pub fn palette_input(&mut self, query: String) -> Task<Message> {
        self.drag.claim_keyboard_hover();
        if let Some(Dialog::Palette(palette)) = &mut self.dialog {
            palette.query = query;
            palette.selected = 0;
        }
        self.scroll_palette_to_selected()
    }

    /// Record the list's viewport height, reported by the scrollable once the
    /// content overflows (the only time scrolling is possible).
    pub fn palette_viewport(&mut self, height: f32) {
        if let Some(Dialog::Palette(palette)) = &mut self.dialog {
            palette.viewport = Some(height);
        }
    }

    /// Center the selected row in the list. Called wherever the selection
    /// moves by keyboard, and when a new query resets it to the first row.
    pub(crate) fn scroll_palette_to_selected(&self) -> Task<Message> {
        let Some(Dialog::Palette(palette)) = &self.dialog else {
            return Task::none();
        };
        let Some(viewport) = palette.viewport else {
            return Task::none();
        };
        let Some(y) = palette.scroll_target(&palette.rows(self), viewport) else {
            return Task::none();
        };
        operation::scroll_to::<Message>(
            crate::app::ui::palette_list_id(),
            operation::AbsoluteOffset { x: 0.0, y },
        )
    }

    /// Move the selection to the row under the mouse, unless keyboard
    /// navigation owns it.
    pub fn hover_palette(&mut self, pos: usize) {
        if !self.drag.accept_mouse_hover() {
            return;
        }
        if let Some(Dialog::Palette(palette)) = &mut self.dialog {
            palette.selected = pos;
        }
    }

    pub fn confirm_palette(&mut self, play: bool) -> Task<Message> {
        let Some(Dialog::Palette(palette)) = &self.dialog else {
            return Task::none();
        };
        let selected = palette.selected;
        self.run_palette_row(selected, play)
    }

    /// Run row `index`: commands and views close the palette and re-enter the
    /// update loop with the message that does the work, playlists and saved
    /// cards go through the navigation paths that already exist.
    pub fn run_palette_row(&mut self, index: usize, play: bool) -> Task<Message> {
        let Some(Dialog::Palette(palette)) = &self.dialog else {
            return Task::none();
        };
        let Some(row) = palette.rows(self).into_iter().nth(index) else {
            return Task::none();
        };
        if let PaletteMode::AddToPlaylist(add) = palette.mode.clone() {
            let PaletteItem::Playlist(playlist) = row.item else {
                return Task::none();
            };
            self.dialog = None;
            self.handle_add_to_playlist(add.pane, playlist, &add.indices);
            return Task::none();
        }

        match row.item {
            PaletteItem::Playlist(playlist) => {
                self.dialog = None;
                if play {
                    self.handle_open_and_play_playlist(playlist)
                } else {
                    self.handle_select_playlist(playlist)
                }
            }
            PaletteItem::Library(index) => {
                let Some(item) = self.library.items.get(index).cloned() else {
                    return Task::none();
                };
                self.dialog = None;
                let pane = self.focused_pane_id;
                let target = ViewKind::from(item.as_ref());
                self.open_card(pane, &target)
            }
            item => {
                let Some(message) = item.message(self) else {
                    return Task::none();
                };
                self.dialog = None;
                Task::done(message)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::{Duration, Instant};

    use super::*;
    use crate::data::{config, playlists::Playlist};

    fn player() -> MusicPlayer {
        let mut p = MusicPlayer::new_with(config::Config::default());
        p.playlists.playlists = vec![
            Playlist {
                name: "Chill".into(),
                tracks: Vec::new(),
            },
            Playlist {
                name: "Focus".into(),
                tracks: Vec::new(),
            },
        ];
        p
    }

    fn selected(p: &MusicPlayer) -> usize {
        match &p.dialog {
            Some(Dialog::Palette(palette)) => palette.selected,
            _ => panic!("the palette is open"),
        }
    }

    fn expire_hover_guard(p: &mut MusicPlayer) {
        p.drag.hover_guard_until = Some(
            Instant::now()
                .checked_sub(Duration::from_millis(1))
                .unwrap(),
        );
    }

    #[test]
    fn keyboard_stepping_owns_the_selection_until_the_mouse_reclaims_it() {
        let mut p = player();
        let _ = p.open_playlist_picker();
        p.drag.claim_mouse();

        let _ = p.step_palette(1);
        assert_eq!(selected(&p), 1);
        assert!(p.drag.is_hover_controlled);

        // A scroll echo reports the row that slid under the cursor; it must
        // not undo the keyboard step.
        p.hover_palette(0);
        assert_eq!(selected(&p), 1);

        // Once the grace window has passed, a real mouse report wins.
        expire_hover_guard(&mut p);
        p.hover_palette(0);
        assert_eq!(selected(&p), 0);
        assert!(!p.drag.is_hover_controlled);
    }

    #[test]
    fn opening_and_typing_own_the_selection() {
        let mut p = player();
        let _ = p.open_palette();
        assert!(p.drag.is_hover_controlled);
        p.drag.claim_mouse();

        let _ = p.palette_input(String::new());
        assert!(p.drag.is_hover_controlled);
        p.hover_palette(1);
        assert_eq!(selected(&p), 0);
    }
}
