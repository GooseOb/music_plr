use crate::theme::{self, AppTheme};

use iced::{
    widget::{self, Column, Row, Stack},
    Element,
};

use super::{ContextMenuState, Message, MusicPlayer};

mod content;
pub(super) mod floating_search;
mod lyrics;
mod overlays;
mod playbar;
mod playlist;
mod queue;
mod search;
mod settings;
mod shared_components;
mod sidebar;
mod styles;
mod track_list;

pub use queue::{QUEUE_LIST_ID, QUEUE_RECENT_LIST_ID};
pub use track_list::TRACK_LIST_ID;

use track_list::view_track_list;

pub fn view(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let mut body = vec![
        sidebar::view_sidebar(player),
        content::view_main_content(player),
    ];
    if player.show_queue {
        body.push(queue::view_queue_panel(player));
    }

    let layout = Column::with_children([
        Row::with_children(body).into(),
        playbar::view_playbar(player),
    ]);

    let mut stack = Stack::new().push(layout);

    if player.playlist_picker.is_some() {
        stack = stack.push(overlays::view_playlist_picker(player));
    } else if player.delete_confirm_index.is_some() {
        stack = stack.push(overlays::view_delete_confirm());
    } else if let Some(menu) = &player.context_menu {
        stack = stack.push(overlays::view_context_menu(menu, &player.app_theme.palette));
    } else if let Some(rect) = player.drop_indicator_rect() {
        stack = stack.push(overlays::view_drop_indicator(
            rect,
            &player.app_theme.palette,
        ));
    }
    if player.show_search_history {
        if let Some(input_rect) = player.bounds.search_input {
            stack = stack.push(search::view_search_history(player, input_rect));
        }
    }

    match player.drag.cursor_interaction() {
        Some(interaction) => widget::MouseArea::new(stack)
            .interaction(interaction)
            .into(),
        None => stack.into(),
    }
}
