use iced::{
    alignment,
    widget::{text, text_input, Button, Column, Container, Row},
    Color, Element, Length,
};

use crate::{
    app::{
        interaction::TrackListKind, ui::shared_components::empty_state, view_data::PlaylistEntry,
        ViewKind,
    },
    icons,
    load_state::LoadState,
    theme::AppTheme,
};

use super::{
    styles::{button_style_danger, fg_secondary},
    theme, view_track_list, Message, MusicPlayer,
};

pub(super) fn view_playlist<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;

    let is_downloads = matches!(player.view_data().kind, ViewKind::Downloads);

    let header: Element<'a, Message, AppTheme> = if is_downloads {
        Row::with_children([
            icons::icon(icons::DOWNLOAD_ICON, p.fg_muted, theme::ICON_SIZE_MD).into(),
            text("Downloaded tracks").style(fg_secondary()).into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .padding([theme::SPACING_SM, theme::SPACING_XL])
        .into()
    } else if let ViewKind::Playlist(entry) = &player.view_data().kind {
        let idx = entry.index;
        if let Some(pl) = player.playlists.playlists.get(idx) {
            Row::with_children([
                text_input(&pl.name, &entry.name)
                    .on_input(Message::RenamePlaylist)
                    .padding([theme::SPACING_SM, theme::SPACING_MD])
                    .width(Length::Fill)
                    .into(),
                Button::new(
                    Row::with_children([
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
        empty_state("Select a playlist from the sidebar")
    };

    let track_list = if is_downloads {
        match &player.view_data().content {
            LoadState::Ready(tracks) if !tracks.is_empty() => {
                view_track_list(tracks.as_slice(), player, TrackListKind::Active, 0)
            }
            _ => empty_state("No downloaded tracks"),
        }
    } else {
        let playlist = match &player.view_data().kind {
            ViewKind::Playlist(PlaylistEntry { index, .. }) => {
                player.playlists.playlists.get(*index)
            }
            _ => None,
        };
        if let Some(pl) = playlist {
            view_track_list(&pl.tracks, player, TrackListKind::Active, 0)
        } else {
            unreachable!("No playlist selected, but not in downloads view")
        }
    };

    Column::with_children([
        Container::new(header).width(Length::Fill).into(),
        track_list,
    ])
    .spacing(0)
    .into()
}
