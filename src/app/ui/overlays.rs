use iced::{
    alignment,
    widget::{column, container, row, text, Button, Column, Container, MouseArea, Row, Space},
    Element, Length, Rectangle,
};

use crate::{
    app::interaction::TrackListKind,
    icons,
    theme::{AppTheme, Palette},
};

use super::{
    styles::{
        bg_overlay, bg_popup, bg_secondary, button_style_danger, button_style_popup_item,
        fg_secondary,
    },
    theme, ContextMenuState, Message, MusicPlayer,
};

/// A drop-indicator line drawn on top of the UI at the given screen-space
/// rectangle (computed from captured row geometry), so it never perturbs the
/// list layout it marks.
pub(super) fn view_drop_indicator(rect: Rectangle, p: &Palette) -> Element<'_, Message, AppTheme> {
    column![
        Space::new().height(rect.y),
        row![
            Space::new().width(rect.x),
            Container::new(
                Space::new()
                    .width(rect.width)
                    .height(crate::theme::DROP_LINE_HEIGHT),
            )
            .style(move |_: &AppTheme| container::Style {
                background: Some(p.accent.into()),
                ..Default::default()
            })
        ]
    ]
    .into()
}

#[allow(clippy::too_many_lines)]
pub(super) fn view_context_menu<'a>(
    menu: &'a ContextMenuState,
    p: &'a Palette,
) -> Element<'a, Message, AppTheme> {
    let (pos_x, pos_y) = menu.position;

    let items: Vec<Element<'_, Message, AppTheme>> = {
        let mut v: Vec<Element<'_, Message, AppTheme>> = vec![menu_item(
            "Play",
            icons::PLAY_ICON,
            p,
            Message::ContextMenuPlayTrack(menu.pos),
        )
        .into()];

        if menu.is_youtube {
            v.push(
                menu_item(
                    "Song Radio",
                    icons::RADIO_ICON,
                    p,
                    Message::ContextMenuStartSongRadio,
                )
                .into(),
            );
            v.push(
                menu_item(
                    "Artist Radio",
                    icons::RADIO_ICON,
                    p,
                    Message::ContextMenuStartArtistRadio,
                )
                .into(),
            );
        }

        let target_indices = &menu.target_indices;

        v.push(
            menu_item(
                "Add to Playlist",
                icons::FOLDER_ICON,
                p,
                Message::TogglePicker(target_indices.clone()),
            )
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
                    Message::ContextMenuDownloadOrDelete(target_indices.clone()),
                )
                .into(),
            );
        }

        if menu.pos.list == TrackListKind::Queue {
            v.push(
                menu_item(
                    "Remove from Queue",
                    icons::DELETE_ICON,
                    p,
                    Message::ContextMenuRemoveFromQueue(target_indices.clone()),
                )
                .into(),
            );
        } else if menu.in_playlist && menu.pos.list != TrackListKind::Recent {
            v.push(
                menu_item(
                    "Remove from Playlist",
                    icons::DELETE_ICON,
                    p,
                    Message::ContextMenuRemoveFromPlaylist(target_indices.clone()),
                )
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

    let overlay = Container::new(column![
        Space::new().height(pos_y),
        row![Space::new().width(pos_x), menu_content]
    ])
    .width(Length::Fill)
    .height(Length::Fill);

    MouseArea::new(overlay)
        .on_press(Message::CloseContextMenu)
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
            Row::with_children([
                icons::icon(icon, p.fg_muted, theme::ICON_SIZE_SM).into(),
                text(label).into(),
            ])
            .spacing(theme::SPACING_SM)
            .padding([theme::SPACING_XS, theme::SPACING_SM])
            .align_y(alignment::Vertical::Center)
            .width(Length::Fill),
        )
        .padding(0)
        .style(button_style_popup_item())
        .on_press(on_press),
    )
    .style(bg_secondary())
}

pub(super) fn view_playlist_picker(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let items = player
        .playlists
        .playlists
        .iter()
        .enumerate()
        .map(|(i, pl)| {
            Button::new(
                Row::with_children([text(&pl.name).into()])
                    .spacing(theme::SPACING_SM)
                    .align_y(alignment::Vertical::Center)
                    .width(Length::Fill),
            )
            .padding([theme::SPACING_SM, theme::SPACING_MD])
            .style(button_style_popup_item())
            .on_press(Message::AddToPlaylist(i))
            .into()
        });

    let cancel_btn = Button::new(
        Container::new(text("Cancel").size(theme::TEXT_SIZE_SM)).center_x(Length::Fill),
    )
    .padding(theme::SPACING_SM)
    .on_press(Message::ClosePicker);

    view_dialog(
        Column::with_children([
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
        .padding(theme::SPACING_MD)
        .into(),
        Message::ClosePicker,
    )
}

pub fn no_click_propagation(
    content: Element<'_, Message, AppTheme>,
) -> Element<'_, Message, AppTheme> {
    MouseArea::new(content).on_press(Message::Noop).into()
}

fn view_dialog(
    dialog: Element<'_, Message, AppTheme>,
    close_msg: Message,
) -> Element<'_, Message, AppTheme> {
    let dialog = no_click_propagation(Container::new(dialog).style(bg_popup()).into());

    Container::new(MouseArea::new(Container::new(dialog).center(Length::Fill)).on_press(close_msg))
        .style(bg_overlay())
        .into()
}

pub(super) fn view_delete_confirm() -> Element<'static, Message, AppTheme> {
    let cancel_btn = Button::new(Container::new(text("Cancel")).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .on_press(Message::HideDeleteConfirm);

    let delete_btn = Button::new(Container::new(text("Delete")).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .style(button_style_danger())
        .on_press(Message::ConfirmDeletePlaylist);

    view_dialog(
        Column::with_children([
            text("Delete playlist?").size(theme::TEXT_SIZE_LG).into(),
            text("Tracks will not be deleted.")
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
        Message::HideDeleteConfirm,
    )
}
