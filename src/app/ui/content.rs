use iced::widget::{Column, Space};

use super::{
    artist, lyrics, playlist, search, settings, track_list_search, Element, Message, MusicPlayer,
};
use crate::{
    app::{pane::PaneId, ui::search::browse_meta, ViewKind},
    theme::AppTheme,
};

pub(super) fn view_main_content(
    player: &MusicPlayer,
    pane: PaneId,
) -> Element<'_, Message, AppTheme> {
    let track_list_search = match &player.track_list_search {
        Some(fs) if fs.list == crate::app::TrackListKind::Active && fs.pane == pane => {
            track_list_search::view_track_list_search(player, fs)
        }
        _ => Space::new().into(),
    };

    let view = if let Some(lyrics_state) = player.pane(pane).lyrics.as_ref() {
        lyrics::view_lyrics(player, pane, lyrics_state)
    } else {
        match &player.view_data_in(pane).kind {
            ViewKind::Search(s) => search::view_search(player, pane, s),
            ViewKind::SongRadio(label) | ViewKind::ArtistRadio(label) => {
                search::view_search_radio(player, pane, label)
            }
            ViewKind::Artist(_) => artist::view_artist(player, pane),
            ViewKind::Album(r) => search::view_browse(
                player,
                pane,
                r.provider,
                &r.name,
                &r.id,
                browse_meta(&r.badge, &r.date),
            ),
            ViewKind::PlaylistView(r) => {
                search::view_browse(player, pane, r.provider, &r.name, &r.id, None)
            }
            ViewKind::Playlist(entry) => playlist::view_playlist(player, pane, entry),
            ViewKind::Downloads => playlist::view_downloads(player, pane),
            ViewKind::Settings => settings::view_settings(player),
        }
    };

    Column::with_children([
        search::view_search_bar(player, pane),
        track_list_search,
        view,
    ])
    .into()
}
