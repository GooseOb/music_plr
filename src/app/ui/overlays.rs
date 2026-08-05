use iced::{
    alignment,
    widget::{text, Button, Column, Container, MouseArea, Row},
    Element, Length,
};

use crate::{
    icons,
    theme::{AppTheme, Palette},
};

use super::{
    button,
    styles::{bg_overlay, bg_popup, bg_secondary, bg_transparent, button_style_danger},
    theme, ContextMenuState, Message, MusicPlayer,
};

pub(super) fn view_context_menu<'a>(
    menu: &'a ContextMenuState,
    p: &'a Palette,
) -> Element<'a, Message, AppTheme> {
    let pos_x = menu.position.0;
    let pos_y = menu.position.1;

    let items: Vec<Element<'_, Message, AppTheme>> = {
        let mut v: Vec<Element<'_, Message, AppTheme>> = vec![];

        v.push(
            menu_item(
                "Play",
                icons::PLAY_ICON,
                p,
                Message::ContextMenuPlayTrack(menu.track_index),
            )
            .width(Length::Fill)
            .into(),
        );

        if menu.is_youtube {
            v.push(
                menu_item(
                    "Song Radio",
                    icons::RADIO_ICON,
                    p,
                    Message::ContextMenuStartSongRadio(menu.track_index),
                )
                .width(Length::Fill)
                .into(),
            );
            v.push(
                menu_item(
                    "Artist Radio",
                    icons::RADIO_ICON,
                    p,
                    Message::ContextMenuStartArtistRadio(menu.track_index),
                )
                .width(Length::Fill)
                .into(),
            );
        }

        v.push(
            menu_item(
                "Add to Playlist",
                icons::FOLDER_ICON,
                p,
                Message::TogglePicker(menu.track_index),
            )
            .width(Length::Fill)
            .into(),
        );

        if menu.is_youtube {
            let label = if menu.is_downloaded {
                "Delete Download"
            } else {
                "Download"
            };
            v.push(
                menu_item(
                    label,
                    icons::DOWNLOAD_ICON,
                    p,
                    Message::ContextMenuDownloadOrDelete(menu.track_index),
                )
                .width(Length::Fill)
                .into(),
            );
        }

        if menu.is_queue {
            v.push(
                menu_item(
                    "Remove from Queue",
                    icons::DELETE_ICON,
                    p,
                    Message::ContextMenuRemoveFromQueue(menu.track_index),
                )
                .width(Length::Fill)
                .into(),
            );
        } else if menu.in_playlist {
            v.push(
                menu_item(
                    "Remove from Playlist",
                    icons::DELETE_ICON,
                    p,
                    Message::ContextMenuRemoveFromPlaylist(menu.track_index),
                )
                .width(Length::Fill)
                .into(),
            );
        }

        v
    };

    let menu_content = Container::new(
        Column::with_children(items)
            .spacing(2)
            .padding(theme::SPACING_SM),
    )
    .width(theme::CONTEXT_MENU_WIDTH)
    .style(bg_popup());

    let row = Row::with_children(vec![
        Container::new(Row::new()).width(pos_x).into(),
        menu_content.into(),
    ])
    .spacing(0);

    let col = Column::with_children(vec![
        Container::new(Column::new()).height(pos_y).into(),
        row.into(),
    ])
    .spacing(0);

    Container::new(MouseArea::new(col).on_press(Message::CloseContextMenu))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(bg_transparent())
        .into()
}

fn menu_item<'a>(
    label: &'a str,
    icon: &'static [u8],
    p: &'a Palette,
    on_press: Message,
) -> Container<'a, Message, AppTheme> {
    Container::new(
        Button::new(
            Row::with_children(vec![
                icons::icon(icon, p.fg_muted, theme::ICON_SIZE_SM).into(),
                text(label).into(),
            ])
            .spacing(theme::SPACING_SM)
            .padding([theme::SPACING_XS, theme::SPACING_SM])
            .align_y(alignment::Vertical::Center)
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .padding(0)
        .style(move |_, status| {
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => p.bg_hover,
                _ => p.bg_secondary,
            };
            button::Style {
                background: Some(bg.into()),
                text_color: p.fg,
                border: iced::border::rounded(theme::RADIUS_SM),
                ..Default::default()
            }
        })
        .on_press(on_press),
    )
    .width(Length::Fill)
    .style(bg_secondary())
}

pub(super) fn view_playlist_picker<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;

    let playlists: Vec<&crate::playlists::Playlist> = player.playlists.playlists.iter().collect();

    let items: Vec<Element<'a, Message, AppTheme>> = playlists
        .iter()
        .enumerate()
        .map(|(i, pl)| {
            let is_focused = player.picker_focused_index == i;
            let bg_color = if is_focused {
                p.bg_hover
            } else {
                p.bg_secondary
            };
            let bg_hover = p.bg_hover;

            Button::new(
                Row::with_children(vec![text(&pl.name).into()])
                    .spacing(theme::SPACING_SM)
                    .padding([theme::SPACING_SM, theme::SPACING_MD])
                    .align_y(alignment::Vertical::Center)
                    .width(Length::Fill),
            )
            .width(Length::Fill)
            .padding(0)
            .style(move |_, status| {
                let bg = if is_focused {
                    bg_color
                } else {
                    match status {
                        button::Status::Hovered | button::Status::Pressed => bg_hover,
                        _ => bg_color,
                    }
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color: p.fg,
                    border: iced::border::rounded(theme::RADIUS_SM),
                    ..Default::default()
                }
            })
            .on_press(Message::AddToPlaylist(i))
            .into()
        })
        .collect();

    let cancel_btn = Button::new(
        Container::new(text("Cancel").size(theme::TEXT_SIZE_SM)).center_x(Length::Fill),
    )
    .padding(theme::SPACING_SM)
    .on_press(Message::ClosePicker);

    view_dialog(
        Container::new(
            Column::with_children(vec![
                text("Add to Playlist").size(theme::TEXT_SIZE_LG).into(),
                Column::with_children(items)
                    .spacing(theme::SPACING_XS)
                    .width(Length::Fill)
                    .into(),
                cancel_btn.into(),
            ])
            .align_x(alignment::Horizontal::Center)
            .spacing(theme::SPACING_SM)
            .width(theme::DIALOG_WIDTH)
            .padding(theme::SPACING_MD),
        ),
        Message::ClosePicker,
    )
}

fn view_dialog(
    dialog: Container<'_, Message, AppTheme>,
    close_msg: Message,
) -> Element<'_, Message, AppTheme> {
    let dialog = Container::new(dialog).style(bg_popup());

    Container::new(
        MouseArea::new(
            Container::new(MouseArea::new(dialog).on_press(Message::Noop))
                .width(Length::Fill)
                .height(Length::Fill)
                .center(Length::Fill),
        )
        .on_press(close_msg),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .style(bg_overlay())
    .into()
}

pub(super) fn view_delete_confirm(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let p = &player.app_theme.palette;

    let cancel_btn = Button::new(Container::new(text("Cancel")).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .on_press(Message::HideDeleteConfirm);

    let delete_btn = Button::new(Container::new(text("Delete")).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .style(button_style_danger())
        .on_press(Message::ConfirmDeletePlaylist);

    view_dialog(
        Container::new(
            Column::with_children(vec![
                text("Delete playlist?").size(theme::TEXT_SIZE_LG).into(),
                text("Tracks will not be deleted.")
                    .color(p.fg_secondary)
                    .into(),
                Row::with_children(vec![cancel_btn.into(), delete_btn.into()])
                    .spacing(theme::SPACING_XL)
                    .align_y(alignment::Vertical::Center)
                    .into(),
            ])
            .width(theme::DIALOG_WIDTH)
            .align_x(alignment::Horizontal::Center)
            .spacing(theme::SPACING_LG)
            .padding(theme::SPACING_XL),
        ),
        Message::HideDeleteConfirm,
    )
}
