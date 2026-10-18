//! Window overlays: the exclusive dialogs (`view_dialog_overlay`) plus the
//! chrome they share (backdrop, absolute positioning, drop line, toast). One
//! module per dialog lives alongside; `ui::view` dispatches through here.

mod deps;
mod edit;
mod import;
mod menus;
mod playlist;
mod shortcuts;
mod translate;

pub use edit::edit_manual_search_input_id;
use iced::{
    widget::{column, container, opaque, text, Column, Container, MouseArea, Row, Space},
    Element, Length, Rectangle,
};
// `ui::view` pushes the open menu itself; every other overlay goes through
// `view_dialog_overlay`.
pub(super) use menus::view_context_menu;
pub use playlist::playlist_jump_input_id;

use self::{
    deps::view_dependency_dialog,
    edit::view_edit_track,
    import::view_import_playlist,
    menus::view_card_menu,
    playlist::{view_delete_confirm, view_playlist_jump},
    shortcuts::view_shortcuts,
    translate::view_translate_dialog,
};
use super::{
    styles::{bg_overlay, bg_popup, bg_toast},
    theme, Message, MusicPlayer,
};
use crate::{app::dialog::Dialog, theme::AppTheme};

pub(super) fn view_drop_indicator(rect: Rectangle) -> Element<'static, Message, AppTheme> {
    pos_absolute(
        Container::new(
            Space::new()
                .width(rect.width)
                .height(crate::theme::DROP_LINE_HEIGHT),
        )
        .style(|theme: &AppTheme| container::Style {
            background: Some(theme.palette.accent.into()),
            ..Default::default()
        })
        .into(),
        rect.x,
        rect.y,
    )
    .into()
}

pub fn pos_absolute(
    content: Element<'_, Message, AppTheme>,
    pos_x: impl Into<Length>,
    pos_y: impl Into<Length>,
) -> Column<'_, Message, AppTheme> {
    column![
        Space::new().height(pos_y),
        Row::with_children([Space::new().width(pos_x).into(), content])
    ]
}

pub(super) fn view_notification(toast: &crate::app::Toast) -> Element<'_, Message, AppTheme> {
    pos_absolute(
        Container::new(opaque(
            Container::new(text(toast.message.as_ref()).size(theme::TEXT_SIZE_LG))
                .padding([theme::SPACING_MD, theme::SPACING_LG])
                .style(bg_toast(toast.is_error))
                .max_width(theme::DIALOG_WIDTH_LG),
        ))
        .padding(theme::SPACING_2XL)
        .into(),
        Length::Fill,
        Length::Fill,
    )
    .into()
}

/// The overlay element for one dialog. Shared by the top-level `Dialog` slot
/// and by a context menu's `return_to`, which renders the dialog it covered
/// underneath itself.
pub(super) fn view_dialog_overlay<'a>(
    player: &'a MusicPlayer,
    dialog: &'a Dialog,
) -> Element<'a, Message, AppTheme> {
    match dialog {
        Dialog::Dependencies(dialog) => view_dependency_dialog(player, dialog),
        Dialog::PlaylistJump(jump) => view_playlist_jump(player, jump),
        Dialog::Shortcuts => view_shortcuts(player),
        Dialog::DeleteConfirm(_) => view_delete_confirm(player.strings),
        Dialog::Edit(edit) => view_edit_track(player, edit),
        Dialog::Import(dialog) => view_import_playlist(player, dialog),
        Dialog::Translate(dialog) => view_translate_dialog(player, dialog),
        // A menu's own `return_to` is rendered by its caller, so a menu inside
        // one renders just its panel.
        Dialog::ContextMenu(menu) => view_context_menu(player, menu),
        Dialog::CardMenu(menu) => view_card_menu(player, menu),
    }
}

fn view_dialog(
    dialog: Element<'_, Message, AppTheme>,
    close_msg: Message,
) -> Element<'_, Message, AppTheme> {
    let dialog = opaque(Container::new(dialog).style(bg_popup()));

    let backdrop = MouseArea::new(Container::new(dialog).center(Length::Fill)).on_press(close_msg);

    Container::new(opaque(backdrop)).style(bg_overlay()).into()
}
