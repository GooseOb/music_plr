use crate::theme::{self, AppTheme};

use iced::{
    alignment,
    widget::{self, button, Column, Container, Row, Stack},
    Element, Length,
};

use super::{ContextMenuState, DragTargetList, Message, MusicPlayer};

mod content;
mod overlays;
mod playbar;
mod playlist;
mod queue;
mod search;
mod shared_components;
mod sidebar;
mod styles;
mod track_list;

pub use queue::QUEUE_LIST_ID;
pub use track_list::TRACK_LIST_ID;

use track_list::view_track_list;

pub fn view(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let main_content = content::view_main_content(player);
    let sidebar = sidebar::view_sidebar(player);
    let queue = if player.show_queue {
        queue::view_queue_panel(player)
    } else {
        Container::new(Row::new()).width(0.0).into()
    };

    let body = Row::with_children(vec![sidebar, main_content, queue])
        .height(Length::Fill)
        .align_y(alignment::Vertical::Top);

    let layout = Column::with_children(vec![body.into(), playbar::view_playbar(player)]).spacing(0);

    let main = Container::new(layout)
        .width(Length::Fill)
        .height(Length::Fill);

    let mut stack = Stack::new()
        .width(Length::Fill)
        .height(Length::Fill)
        .push(main);

    if player.show_playlist_picker {
        stack = stack.push(overlays::view_playlist_picker(player));
    } else if player.show_delete_confirm {
        stack = stack.push(overlays::view_delete_confirm());
    } else if let Some(menu) = &player.context_menu {
        if menu.visible {
            stack = stack.push(overlays::view_context_menu(menu, &player.app_theme.palette));
        }
    }

    stack.into()
}
