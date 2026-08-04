use iced::{
    alignment,
    widget::{text, Button, Column, Container, MouseArea, Row},
    Color, Length,
};

use super::{
    bg, button, button_style_danger, button_style_secondary, container, icons, theme,
    ContextMenuState, Element, Message, MusicPlayer,
};

fn transparent_bg() -> impl Fn(&iced::Theme) -> container::Style + 'static {
    |_| container::Style {
        background: None,
        ..Default::default()
    }
}

fn menu_bg(bg_color: Color) -> impl Fn(&iced::Theme) -> container::Style + 'static {
    move |_| container::Style {
        background: Some(bg_color.into()),
        border: iced::border::rounded(theme::RADIUS_MD),
        ..Default::default()
    }
}

pub(super) fn view_context_menu<'a>(
    menu: &'a ContextMenuState,
    p: &'a theme::Palette,
) -> Element<'a, Message> {
    let pos_x = menu.position.0;
    let pos_y = menu.position.1;

    let items: Vec<Element<'_, Message>> = {
        let mut v: Vec<Element<'_, Message>> = vec![];

        v.push(
            menu_item(
                "Play",
                "play.svg",
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
                    "radio.svg",
                    p,
                    Message::ContextMenuStartSongRadio(menu.track_index),
                )
                .width(Length::Fill)
                .into(),
            );
            v.push(
                menu_item(
                    "Artist Radio",
                    "radio.svg",
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
                "folder.svg",
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
                    "download.svg",
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
                    "delete.svg",
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
                    "delete.svg",
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
    .width(Length::Fixed(theme::CONTEXT_MENU_WIDTH))
    .style(menu_bg(p.bg_secondary));

    let row = Row::with_children(vec![
        Container::new(Row::new())
            .width(Length::Fixed(pos_x))
            .into(),
        menu_content.into(),
    ])
    .spacing(0);

    let col = Column::with_children(vec![
        Container::new(Column::new())
            .height(Length::Fixed(pos_y))
            .into(),
        row.into(),
    ])
    .spacing(0);

    Container::new(MouseArea::new(col).on_press(Message::CloseContextMenu))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(transparent_bg())
        .into()
}

fn menu_item<'a>(
    label: &'a str,
    icon: &'a str,
    p: &'a theme::Palette,
    on_press: Message,
) -> Container<'a, Message> {
    Container::new(
        Button::new(
            Row::with_children(vec![
                icons::icon(icon, p.fg_muted, theme::ICON_SIZE_SM).into(),
                text(label)
                    .size(theme::TEXT_SIZE_DEFAULT)
                    .color(p.fg)
                    .into(),
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
    .style(bg(p.bg_secondary))
}

pub(super) fn view_playlist_picker<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;

    let playlists: Vec<&crate::playlists::Playlist> = player.playlists.playlists.iter().collect();

    let items: Vec<Element<'a, Message>> = playlists
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
            let is_focused_copy = is_focused;

            Button::new(
                Row::with_children(vec![text(&pl.name)
                    .size(theme::TEXT_SIZE_DEFAULT)
                    .color(p.fg)
                    .into()])
                .spacing(theme::SPACING_SM)
                .padding([theme::SPACING_SM, theme::SPACING_MD])
                .align_y(alignment::Vertical::Center)
                .width(Length::Fill),
            )
            .width(Length::Fill)
            .padding(0)
            .style(move |_, status| {
                let bg = if is_focused_copy {
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
        Container::new(text("Cancel").size(theme::TEXT_SIZE_SM).color(Color::WHITE)).padding(4),
    )
    .padding(theme::SPACING_SM)
    .width(Length::Fixed(theme::BUTTON_WIDTH))
    .style(button_style_secondary(p))
    .on_press(Message::ClosePicker);

    let dialog = Container::new(
        Column::with_children(vec![
            text("Add to Playlist")
                .size(theme::TEXT_SIZE_LG)
                .color(p.fg)
                .into(),
            Column::with_children(items)
                .spacing(0)
                .width(Length::Fill)
                .into(),
            cancel_btn.into(),
        ])
        .spacing(theme::SPACING_SM)
        .padding(0),
    )
    .width(Length::Fixed(theme::DIALOG_WIDTH))
    .height(Length::Fill)
    .style(bg(p.bg_secondary));

    Container::new(MouseArea::new(dialog).on_press(Message::ClosePicker))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(bg(p.overlay))
        .into()
}

pub(super) fn view_delete_confirm(player: &MusicPlayer) -> Element<'_, Message> {
    let p = &player.palette;

    let cancel_btn = Button::new(
        Container::new(text("Cancel").size(theme::TEXT_SIZE_SM).color(Color::WHITE)).padding(4),
    )
    .padding(theme::SPACING_SM)
    .width(Length::Fixed(theme::BUTTON_WIDTH))
    .style(button_style_secondary(p))
    .on_press(Message::HideDeleteConfirm);

    let delete_btn = Button::new(
        Container::new(text("Delete").size(theme::TEXT_SIZE_SM).color(Color::WHITE)).padding(4),
    )
    .padding(theme::SPACING_SM)
    .width(Length::Fixed(theme::BUTTON_WIDTH))
    .style(button_style_danger(p))
    .on_press(Message::ConfirmDeletePlaylist);

    let dialog = Container::new(
        Column::with_children(vec![
            text("Delete playlist?")
                .size(theme::TEXT_SIZE_LG)
                .color(p.fg)
                .into(),
            text("Tracks will not be deleted.")
                .size(theme::TEXT_SIZE_DEFAULT)
                .color(p.fg_secondary)
                .into(),
            Row::with_children(vec![cancel_btn.into(), delete_btn.into()])
                .spacing(theme::SPACING_SM)
                .align_y(alignment::Vertical::Center)
                .into(),
        ])
        .spacing(theme::SPACING_MD)
        .padding(theme::SPACING_XL),
    )
    .width(Length::Fixed(theme::DIALOG_WIDTH))
    .height(Length::Fixed(theme::DIALOG_HEIGHT))
    .style(bg(p.bg_secondary));

    Container::new(MouseArea::new(dialog))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(bg(p.overlay))
        .into()
}
