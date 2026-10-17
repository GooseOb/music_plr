use iced::{
    widget::{self, scrollable, Column, Id, Row, Stack},
    Element,
};

use super::{Dialog, Message, MusicPlayer};
use crate::{
    app::pane::PaneId,
    theme::{self, AppTheme},
};

pub(crate) mod artist;
mod content;
pub(crate) mod lyrics;
mod overlays;
mod playbar;
mod playlist;
pub(crate) mod progress_bar;
mod queue;
pub(crate) mod right_click;
mod search;
mod settings;
mod shared_components;
mod sidebar;
pub(crate) mod spinner;
mod split;
mod styles;
mod track_list;
pub(super) mod track_list_search;
pub use lyrics::lyrics_scroll_id;
pub use overlays::playlist_jump_input_id;
pub use queue::{QUEUE_LIST_ID, QUEUE_RECENT_LIST_ID};
pub use search::{search_history_list_id, search_input_id};
pub use track_list::track_list_id;
use track_list::view_track_list;

/// Id of a pane's card-hosting scrollable: the card tab of a search view, or
/// an artist page. A pane renders either a track list or cards, never both, so
/// one id per pane serves both card hosts; it is captured only for its scroll
/// offset (card right-clicks), never for drop hit-testing.
pub fn card_list_id(pane: PaneId) -> Id {
    Id::from(format!("card_list:{pane}"))
}

/// Id of an artist page's horizontal card shelf (one section's row). Keyed by
/// pane *and* section: every section lays its shelf out on each pass, so one id
/// per pane would collide — and the card menu needs each shelf's own horizontal
/// offset to un-shift a right-click.
pub fn card_row_id(pane: PaneId, section: crate::providers::ArtistSectionKind) -> Id {
    Id::from(format!("card_row:{pane}:{}", section as usize))
}

/// The message a scrollable reports when it moves to `offset`.
pub(crate) fn scrolled(
    target: crate::app::update::operation::ScrollTarget,
    offset: iced::widget::scrollable::AbsoluteOffset,
) -> Message {
    Message::Scrolled(target, iced::Vector::new(offset.x, offset.y))
}

/// The pane's card host (see [`card_list_id`]): the scrollable itself, so a
/// host can't forget the id its offset is captured under or the message that
/// keeps it fresh.
pub(crate) fn card_scrollable<'a>(
    pane: PaneId,
    content: impl Into<Element<'a, Message, AppTheme>>,
) -> iced::widget::Scrollable<'a, Message, AppTheme> {
    use crate::app::update::operation::ScrollTarget;

    scrollable(content)
        .id(card_list_id(pane))
        .on_scroll(move |vp| scrolled(ScrollTarget::Cards(pane), vp.absolute_offset()))
}

/// Id of the context-menu panel and its rows. Rows all share one id;
/// `CaptureBounds` records their bounds in visit (top-to-bottom) order so a
/// submenu can be aligned with its parent row.
pub const CONTEXT_MENU_PANEL_ID: Id = Id::new("context_menu_panel");
pub const CONTEXT_MENU_ROW_ID: Id = Id::new("context_menu_row");

pub fn view(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let mut body = vec![
        sidebar::view_sidebar(player),
        split::view_split_tree(player, &player.split_root),
    ];
    if player.show_queue {
        body.push(queue::view_queue_panel(player));
    }

    let layout = Column::with_children([
        Row::with_children(body).into(),
        playbar::view_playbar(player),
    ]);

    let mut stack = Stack::new().push(layout);

    match &player.dialog {
        Some(Dialog::Dependencies(dialog)) => {
            stack = stack.push(overlays::view_dependency_dialog(player, dialog));
        }
        Some(Dialog::PlaylistJump(jump)) => {
            stack = stack.push(overlays::view_playlist_jump(player, jump));
        }
        Some(Dialog::Shortcuts) => {
            stack = stack.push(overlays::view_shortcuts(player));
        }
        Some(Dialog::DeleteConfirm(_)) => {
            stack = stack.push(overlays::view_delete_confirm(player.strings));
        }
        Some(Dialog::Edit(edit)) => {
            stack = stack.push(overlays::view_edit_track(player, edit));
        }
        Some(Dialog::Import(dialog)) => {
            stack = stack.push(overlays::view_import_playlist(player, dialog));
        }
        Some(Dialog::Translate(dialog)) => {
            stack = stack.push(overlays::view_translate_dialog(player, dialog));
        }
        Some(Dialog::ContextMenu(context_menu)) => {
            stack = stack.push(overlays::view_context_menu(player, context_menu));
        }
        Some(Dialog::CardMenu(card_menu)) => {
            stack = stack.push(overlays::view_card_menu(player, card_menu));
        }
        None => {
            if let Some(rect) = player.drop_indicator_rect() {
                stack = stack.push(overlays::view_drop_indicator(rect));
            }
        }
    }
    for pane in player.pane_ids() {
        if player.pane(pane).show_search_history {
            if let Some(input_rect) = player.bounds.search_inputs.get(&pane).copied() {
                stack = stack.push(search::view_search_history(player, pane, input_rect));
            }
        }
    }
    if let Some(notification) = &player.notification {
        stack = stack.push(overlays::view_notification(notification));
    }

    let cursor = player
        .drag
        .cursor_interaction()
        .unwrap_or(iced::mouse::Interaction::None);
    widget::MouseArea::new(stack).interaction(cursor).into()
}
