use iced::widget::{Column, Space};

use crate::{app::ViewKind, theme::AppTheme};

use super::{
    artist, lyrics, playlist, search, settings, track_list_search, Element, Message, MusicPlayer,
};

pub(super) fn view_main_content<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let search_bar = search::view_search_bar(player);

    let inner: Element<'a, Message, AppTheme> = if let Some(lyrics_state) = player.lyrics.as_ref() {
        lyrics::view_lyrics(player, lyrics_state)
    } else {
        match &player.view_data().kind {
            ViewKind::Search(s) => search::view_search(player, s),
            ViewKind::SongRadio(label) | ViewKind::ArtistRadio(label) => {
                search::view_search_radio(player, label)
            }
            ViewKind::Artist(_) => artist::view_artist(player),
            ViewKind::Album(r) | ViewKind::PlaylistView(r) => search::view_browse(player, &r.name),
            ViewKind::Playlist(entry) => playlist::view_playlist(player, entry),
            ViewKind::Downloads => playlist::view_downloads(player),
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
