use iced::widget::Column;

use super::{
    artist, lyrics, playlist, search, settings, track_list_search, Element, Message, MusicPlayer,
};
use crate::{
    app::{ui::search::browse_meta, ViewKind},
    theme::AppTheme,
};

pub(super) fn view_main_content<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let mut children: Vec<Element<'a, Message, AppTheme>> = vec![search::view_search_bar(player)];

    if let Some(fs) = player.track_list_search.as_ref() {
        if fs.list == crate::app::TrackListKind::Active {
            children.push(track_list_search::view_track_list_search(player, fs));
        }
    }

    children.push(if let Some(lyrics_state) = player.lyrics.as_ref() {
        lyrics::view_lyrics(player, lyrics_state)
    } else {
        match &player.view_data().kind {
            ViewKind::Search(s) => search::view_search(player, s),
            ViewKind::SongRadio(label) | ViewKind::ArtistRadio(label) => {
                search::view_search_radio(player, label)
            }
            ViewKind::Artist(_) => artist::view_artist(player),
            ViewKind::Album(r) => {
                search::view_browse(player, &r.name, &r.id, browse_meta(&r.badge, &r.date))
            }
            ViewKind::PlaylistView(r) => search::view_browse(player, &r.name, &r.id, None),
            ViewKind::Playlist(entry) => playlist::view_playlist(player, entry),
            ViewKind::Downloads => playlist::view_downloads(player),
            ViewKind::Settings => settings::view_settings(player),
        }
    });

    Column::with_children(children).into()
}
