//! Playlist dialogs: the delete confirmation and the name field shared by
//! "new playlist" and "rename playlist". The playlist picker lives in
//! [`super::palette`], which the add-to-playlist flow shares with the command
//! palette.

use iced::{
    alignment,
    widget::{text, text_input, Column},
    Element, Length,
};

use super::{theme, view_dialog, Message, MusicPlayer};
use crate::{
    app::ui::{
        shared_components::dialog_button,
        styles::{button_style_danger, button_style_primary, fg_secondary},
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

/// One name field for both server-playlist naming flows: creating a playlist
/// (`id: None`) and renaming an existing one (`id: Some`).
pub(super) fn view_server_playlist_name<'a>(
    player: &'a MusicPlayer,
    dialog: &'a crate::app::ServerPlaylistNameDialog,
) -> Element<'a, Message, AppTheme> {
    let tr = player.strings;
    let dir = tr.direction;

    let input = text_input(tr.new_playlist_name, &dialog.name)
        .on_input(Message::ServerPlaylistNameChanged)
        .on_submit(Message::SubmitServerPlaylistName)
        .align_x(dir.align())
        .padding(theme::SPACING_SM)
        .width(Length::Fill);

    let cancel_btn = dialog_button(tr.cancel).on_press(Message::CloseDialog);
    let save_btn = dialog_button(tr.save)
        .style(button_style_primary())
        .on_press_maybe(
            (!dialog.name.trim().is_empty()).then_some(Message::SubmitServerPlaylistName),
        );

    view_dialog(
        Column::with_children([
            text(tr.playlists).size(theme::TEXT_SIZE_LG).into(),
            text(tr.provider(dialog.provider))
                .style(fg_secondary())
                .into(),
            input.into(),
            dir.row([cancel_btn.into(), save_btn.into()])
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

/// Confirmation for deleting a playlist on its server.
pub(super) fn view_server_playlist_delete<'a>(
    player: &'a MusicPlayer,
    dialog: &'a crate::app::ServerPlaylistDeleteDialog,
) -> Element<'a, Message, AppTheme> {
    let tr = player.strings;
    let dir = tr.direction;

    let cancel_btn = dialog_button(tr.cancel).on_press(Message::CloseDialog);
    let delete_btn = dialog_button(tr.delete)
        .style(button_style_danger())
        .on_press(Message::ConfirmDeleteServerPlaylist);

    view_dialog(
        Column::with_children([
            text(tr.delete_playlist_q).size(theme::TEXT_SIZE_LG).into(),
            text(dialog.name.as_str()).style(fg_secondary()).into(),
            text(format!(
                "{} \u{00b7} {}",
                tr.provider(dialog.target.provider),
                tr.tracks_wont_be_deleted
            ))
            .style(fg_secondary())
            .into(),
            dir.row([cancel_btn.into(), delete_btn.into()])
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
