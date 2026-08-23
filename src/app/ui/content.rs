use iced::widget::{Column, Space};

use crate::{app::ViewKind, theme::AppTheme};

use super::{
    artist, lyrics, playlist, search, settings, track_list_search, Element, Message, MusicPlayer,
};

pub(super) fn view_main_content<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let search_bar = search::view_search_bar(player);

    let inner: Element<'a, Message, AppTheme> = if player.lyrics.is_some() {
        lyrics::view_lyrics(player)
    } else {
        match &player.view_data().kind {
            ViewKind::Search { tab, .. } => search::view_search(player, tab),
            ViewKind::SongRadio(_) | ViewKind::ArtistRadio(_) => search::view_search_radio(player),
            ViewKind::Artist { .. } => artist::view_artist(player),
            ViewKind::Album { name, .. } | ViewKind::PlaylistView { name, .. } => {
                search::view_browse(player, name)
            }
            ViewKind::Playlist { .. } | ViewKind::Downloads => playlist::view_playlist(player),
            ViewKind::Settings => settings::view_settings(player),
        }
    };

    let float_slot: Element<'a, Message, AppTheme> = if matches!(
        player.track_list_search.as_ref().map(|fs| fs.list),
        Some(crate::app::TrackListKind::Active)
    ) {
        track_list_search::view_track_list_search(player)
    } else {
        Space::new().into()
    };

    let children: Vec<Element<'a, Message, AppTheme>> = vec![search_bar, float_slot, inner];
    Column::with_children(children).into()
}
