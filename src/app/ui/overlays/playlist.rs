//! The Ctrl+K playlist jumper (which doubles as the add-to-playlist picker)
//! and the delete-playlist confirmation.

use iced::{
    alignment,
    widget::{container, scrollable, text, text_input, Button, Column, Container, MouseArea, Row},
    Element, Length,
};

use super::{theme, view_dialog, Message, MusicPlayer};
use crate::{
    app::ui::styles::{button_style_danger, fg_secondary},
    theme::AppTheme,
};

pub fn playlist_jump_input_id() -> iced::widget::Id {
    iced::widget::Id::new("playlist_jump_input")
}

pub(super) fn view_playlist_jump<'a>(
    player: &'a MusicPlayer,
    jump: &'a crate::app::PlaylistJump,
) -> Element<'a, Message, AppTheme> {
    let names: Vec<String> = player
        .playlists
        .playlists
        .iter()
        .map(|p| p.name.clone())
        .collect();
    let filtered = jump.filtered(&names);
    let title: String = match &jump.pending {
        Some(add) if add.indices.len() > 1 => {
            (player.strings.ctx_add_to_playlist_n)(add.indices.len())
        }
        Some(_) => player.strings.ctx_add_to_playlist.to_string(),
        None => player.strings.playlists.to_string(),
    };
    let items = filtered.iter().enumerate().map(|(pos, &i)| {
        let pl = &player.playlists.playlists[i];
        let row = Row::with_children([
            text(&pl.name).width(Length::Fill).into(),
            text(pl.tracks.len()).style(fg_secondary()).into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .width(Length::Fill);
        let padded = Container::new(row).padding([theme::SPACING_SM, theme::SPACING_MD]);
        let highlighted = Container::new(
            MouseArea::new(padded)
                .on_press(Message::PlaylistJumpOpen(i))
                .on_enter(Message::PlaylistJumpHover(pos))
                .interaction(iced::mouse::Interaction::Pointer),
        )
        .style(move |theme: &AppTheme| container::Style {
            background: if pos == jump.selected {
                Some(theme.palette.bg_current.into())
            } else {
                None
            },
            border: iced::border::rounded(theme::RADIUS_SM),
            ..Default::default()
        });
        highlighted.into()
    });

    let list: Element<'_, Message, AppTheme> = if filtered.is_empty() {
        Container::new(
            text(player.strings.no_results_found)
                .style(fg_secondary())
                .width(Length::Fill),
        )
        .padding([theme::SPACING_XS, theme::SPACING_MD])
        .into()
    } else {
        scrollable(Column::with_children(items).spacing(theme::SPACING_XS))
            .height(Length::Fill)
            .into()
    };

    let input = text_input(player.strings.find_in_list, &jump.query)
        .on_input(Message::PlaylistJumpInput)
        .padding(theme::SPACING_SM)
        .id(playlist_jump_input_id())
        .width(Length::Fill);

    let cancel_btn = Button::new(
        Container::new(text(player.strings.cancel).size(theme::TEXT_SIZE_SM))
            .center_x(Length::Fill),
    )
    .padding(theme::SPACING_SM)
    .on_press(Message::CloseDialog);

    view_dialog(
        Column::with_children([
            text(title).size(theme::TEXT_SIZE_LG).into(),
            input.into(),
            list,
            cancel_btn.into(),
        ])
        .align_x(alignment::Horizontal::Center)
        .spacing(theme::SPACING_SM)
        .width(theme::DIALOG_WIDTH_LG)
        .height(player.window_size.height * 0.6)
        .padding(theme::SPACING_MD)
        .into(),
        Message::CloseDialog,
    )
}

pub(super) fn view_delete_confirm(
    strings: &crate::i18n::Strings,
) -> Element<'_, Message, AppTheme> {
    let cancel_btn = Button::new(Container::new(text(strings.cancel)).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .on_press(Message::CloseDialog);

    let delete_btn = Button::new(Container::new(text(strings.delete)).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
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
            Row::with_children([cancel_btn.into(), delete_btn.into()])
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
