use iced::{
    alignment,
    widget::{text, text_input, Button, Column, Container, Row},
    Color, Element, Length,
};

use super::{
    button_style_danger, button_style_secondary, icons, text_input_style, theme, view_track_list,
    Message, MusicPlayer,
};

pub(super) fn view_playlist<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;

    let header: Element<'a, Message> = if let Some(idx) = player.selected_playlist {
        if let Some(pl) = player.playlists.playlists.get(idx) {
            let track_count = pl.tracks.len();
            Row::with_children(vec![
                text_input(&pl.name, &player.selected_playlist_name)
                    .on_input(Message::RenamePlaylist)
                    .size(theme::TEXT_SIZE_MD)
                    .padding([theme::SPACING_SM, theme::SPACING_MD])
                    // .padding([theme::SPACING_XS, theme::SPACING_SM])
                    .width(Length::Fill)
                    .style(text_input_style(p))
                    .into(),
                text(format!("({track_count} tracks)"))
                    .size(theme::TEXT_SIZE_MD)
                    .color(p.fg_secondary)
                    .into(),
                Button::new(
                    Row::with_children(vec![
                        icons::icon("folder.svg", Color::WHITE, theme::ICON_SIZE_SM).into(),
                        text("Add local")
                            .size(theme::TEXT_SIZE_DEFAULT)
                            .align_y(alignment::Vertical::Center)
                            .color(Color::WHITE)
                            .into(),
                    ])
                    .spacing(theme::SPACING_SM)
                    .align_y(alignment::Vertical::Center),
                )
                .padding(theme::SPACING_SM)
                .height(Length::Fixed(theme::BUTTON_HEIGHT))
                .style(button_style_secondary(p))
                .on_press(Message::AddLocalMusic)
                .into(),
                Button::new(icons::icon("delete.svg", p.fg, theme::ICON_SIZE_SM))
                    .padding(theme::SPACING_SM)
                    .height(theme::BUTTON_HEIGHT)
                    .width(theme::BUTTON_HEIGHT)
                    .style(button_style_danger(p))
                    .on_press(Message::ShowDeleteConfirm(idx))
                    .into(),
            ])
            .spacing(theme::SPACING_SM)
            .align_y(alignment::Vertical::Center)
            .padding([theme::SPACING_MD, theme::SPACING_XL])
            .into()
        } else {
            Row::new().into()
        }
    } else {
        Container::new(
            text("Select a playlist from the sidebar")
                .size(theme::TEXT_SIZE_MD)
                .color(p.fg_secondary),
        )
        .padding(theme::SPACING_XL)
        .into()
    };

    let track_list = if let Some(idx) = player.selected_playlist {
        if let Some(pl) = player.playlists.playlists.get(idx) {
            view_track_list(&pl.tracks, player, false, 0)
        } else {
            Container::new(Row::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    } else {
        Container::new(Row::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };

    Column::with_children(vec![
        Container::new(header).width(Length::Fill).into(),
        track_list,
    ])
    .spacing(0)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}
