use iced::{
    alignment,
    widget::{text, Button, Column},
    Element,
};

use super::{
    search::{track_page_header, PageTitle, TrackPage},
    shared_components::{empty_state, icon_button_delete},
    styles::{fg_secondary, icon_style_fg_muted, icon_style_fg_secondary},
    theme, view_track_list, Message, MusicPlayer,
};
use crate::{
    app::{pane::PaneId, view_data::PlaylistEntry},
    icons,
    load_state::LoadState,
    providers::ProviderId,
    theme::AppTheme,
};

pub(super) fn view_playlist<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    entry: &'a PlaylistEntry,
) -> Element<'a, Message, AppTheme> {
    let Some(pl) = player.playlists.playlists.get(entry.index) else {
        return empty_state(player.strings.playlist_not_found);
    };

    // The local page mirrors the server playlist page: the same header with
    // the inline name input and the delete action, only without the refresh
    // button — a refetch would ask the server for what this view already
    // holds in the store.
    let dir = player.strings.direction;
    let add_local = Button::new(
        dir.row([
            icons::icon(icons::FOLDER_ICON, theme::ICON_SIZE_MD)
                .style(icon_style_fg_secondary())
                .into(),
            text(player.strings.add_local)
                .align_y(alignment::Vertical::Center)
                .into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center),
    )
    .padding([theme::SPACING_XS, theme::SPACING_SM])
    .on_press(Message::AddLocalMusic);

    let header = track_page_header(
        player,
        TrackPage {
            thumbnail: None,
            title: PageTitle::Rename {
                // The live store name is both mirror and truth: what the box
                // shows is what the store holds (the typed name saves at
                // once). The snapshot on the history entry stays for nav
                // reconstructs.
                placeholder: &pl.name,
                value: &pl.name,
                on_input: Box::new(move |name| Message::RenamePlaylist(entry.index, name)),
            },
            meta: player.strings.provider(ProviderId::Local),
        },
        None,
        [
            add_local.into(),
            icon_button_delete()
                .on_press(Message::ShowDeleteConfirm(entry.index))
                .into(),
        ],
    );

    let track_list = view_track_list(&pl.tracks, player, pane, 0);

    Column::with_children([dir.leading(header), track_list]).into()
}

pub(super) fn view_trashbin(player: &MusicPlayer, pane: PaneId) -> Element<'_, Message, AppTheme> {
    if player.trashbin.tracks.is_empty() {
        empty_state(player.strings.no_tracks_found)
    } else {
        view_track_list(&player.trashbin.tracks, player, pane, 0)
    }
}

pub(super) fn view_downloads(player: &MusicPlayer, pane: PaneId) -> Element<'_, Message, AppTheme> {
    let dir = player.strings.direction;
    let header = dir
        .row([
            icons::icon(icons::DOWNLOAD_ICON, theme::ICON_SIZE_MD)
                .style(icon_style_fg_muted())
                .into(),
            text(player.strings.downloaded_tracks)
                .style(fg_secondary())
                .into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .padding([theme::SPACING_SM, theme::SPACING_XL]);

    let track_list = match &player.view_data_in(pane).content {
        LoadState::Ready(tracks) if !tracks.is_empty() => {
            view_track_list(tracks.as_slice(), player, pane, 0)
        }
        _ => empty_state(player.strings.no_downloaded_tracks),
    };

    Column::with_children([player.strings.direction.leading(header), track_list]).into()
}
