//! Playlist dialogs: the delete confirmation. The playlist picker lives in
//! [`super::palette`], which the add-to-playlist flow shares with the command
//! palette.

use iced::{
    alignment,
    widget::{text, Column},
    Element,
};

use super::{theme, view_dialog, Message};
use crate::{
    app::ui::{
        shared_components::dialog_button,
        styles::{button_style_danger, fg_secondary},
    },
    theme::AppTheme,
};

pub(super) fn view_delete_confirm(
    strings: &crate::i18n::Strings,
) -> Element<'_, Message, AppTheme> {
    let cancel_btn = dialog_button(strings.cancel).on_press(Message::CloseDialog);

    let delete_btn = dialog_button(strings.delete)
        .style(button_style_danger())
        .on_press(Message::ConfirmDeletePlaylist);

    view_dialog(
        Column::with_children([
            text(strings.delete_playlist_q)
                .size(theme::TEXT_SIZE_LG)
                .into(),
            text(strings.tracks_wont_be_deleted)
                .style(fg_secondary())
                .into(),
            strings
                .direction
                .row([cancel_btn.into(), delete_btn.into()])
                .spacing(theme::SPACING_XL)
                .align_y(alignment::Vertical::Center)
                .into(),
        ])
        .width(theme::DIALOG_WIDTH)
        .align_x(alignment::Horizontal::Center)
        .spacing(theme::SPACING_LG)
        .padding(theme::SPACING_XL)
        .into(),
        Message::CloseDialog,
    )
}
