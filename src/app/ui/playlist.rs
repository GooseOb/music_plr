use iced::{
    alignment,
    widget::{text, text_input, Button, Column, Container, Row},
    Color, Element, Length,
};

use crate::{app::ViewData, icons, theme::AppTheme, types::Track};

use super::{
    styles::{button_style_danger, fg_secondary},
    theme, view_track_list, Message, MusicPlayer,
};

pub(super) fn view_playlist<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;

    let is_downloads = matches!(player.view_data, ViewData::Downloads { .. });

    let header: Element<'a, Message, AppTheme> = if is_downloads {
        Row::with_children(vec![
            icons::icon(icons::DOWNLOAD_ICON, p.fg_muted, theme::ICON_SIZE_MD).into(),
            text("Downloaded tracks").style(fg_secondary()).into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .padding([theme::SPACING_SM, theme::SPACING_XL])
        .into()
    } else if let Some(idx) = player.view_data.selected_playlist_id() {
        if let Some(pl) = player.playlists.playlists.get(idx) {
            let track_count = pl.tracks.len();
            Row::with_children(vec![
                text_input(&pl.name, player.view_data.playlist_name())
                    .on_input(Message::RenamePlaylist)
                    .padding([theme::SPACING_SM, theme::SPACING_MD])
                    .width(Length::Fill)
                    .into(),
                text(format!("({track_count} tracks)"))
                    .style(fg_secondary())
                    .into(),
                Button::new(
                    Row::with_children(vec![
                        icons::icon(icons::FOLDER_ICON, Color::WHITE, theme::ICON_SIZE_SM).into(),
                        text("Add local")
                            .align_y(alignment::Vertical::Center)
                            .color(Color::WHITE)
                            .into(),
                    ])
                    .spacing(theme::SPACING_SM)
                    .align_y(alignment::Vertical::Center),
                )
                .padding(theme::SPACING_SM)
                .height(theme::BUTTON_HEIGHT)
                .on_press(Message::AddLocalMusic)
                .into(),
                Button::new(icons::icon(icons::DELETE_ICON, p.fg, theme::ICON_SIZE_SM))
                    .padding(theme::SPACING_SM)
                    .height(theme::BUTTON_HEIGHT)
                    .width(theme::BUTTON_HEIGHT)
                    .style(button_style_danger())
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
        Container::new(text("Select a playlist from the sidebar").style(fg_secondary()))
            .padding(theme::SPACING_XL)
            .into()
    };

    let track_list = if is_downloads {
        let tracks: &[Track] = match &player.view_data {
            ViewData::Downloads { tracks, .. } => tracks,
            _ => &[],
        };
        if tracks.is_empty() {
            Container::new(text("No downloaded tracks").style(fg_secondary()))
                .padding(theme::SPACING_XL)
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        } else {
            view_track_list(tracks, player, false, 0)
        }
    } else if let Some(idx) = player.view_data.selected_playlist_id() {
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
