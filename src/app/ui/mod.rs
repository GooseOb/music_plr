use iced::{
    widget::{self, Column, Id, Row, Stack},
    Element,
};

use super::{ContextMenuState, Message, MusicPlayer};
use crate::theme::{self, AppTheme};

pub(crate) mod artist;
mod content;
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
pub(super) mod track_list_search;
pub use queue::{QUEUE_LIST_ID, QUEUE_RECENT_LIST_ID};
pub use search::{SEARCH_HISTORY_LIST_ID, SEARCH_INPUT_ID};
use track_list::view_track_list;
pub use track_list::TRACK_LIST_ID;

/// Id of the context-menu panel and its rows. Rows all share one id;
/// `CaptureBounds` records their bounds in visit (top-to-bottom) order so a
/// submenu can be aligned with its parent row.
pub const CONTEXT_MENU_PANEL_ID: Id = Id::new("context_menu_panel");
pub const CONTEXT_MENU_ROW_ID: Id = Id::new("context_menu_row");

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
        stack = stack.push(overlays::view_delete_confirm(player.strings));
    } else if player.edit_track.is_some() {
        stack = stack.push(overlays::view_edit_track(player));
    } else if let Some(context_menu) = &player.context_menu {
        stack = stack.push(overlays::view_context_menu(player, context_menu));
    } else if let Some(rect) = player.drop_indicator_rect() {
        stack = stack.push(overlays::view_drop_indicator(rect));
    }
    if player.show_search_history {
        if let Some(input_rect) = player.bounds.search_input {
            stack = stack.push(search::view_search_history(player, input_rect));
        }
    }

    let cursor = player
        .drag
        .cursor_interaction()
        .unwrap_or(iced::mouse::Interaction::None);
    widget::MouseArea::new(stack).interaction(cursor).into()
}
