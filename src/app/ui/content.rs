use iced::widget::{Column, Container, Row, Stack};

use crate::{app::ViewKind, theme::AppTheme};

use super::{
    floating_search, lyrics, playlist, search, settings, theme, Element, Message, MusicPlayer,
};
pub(super) fn view_main_content<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let search_bar = search::view_search_bar(player);

    let inner: Element<'a, Message, AppTheme> = if player.lyrics.is_some() {
        lyrics::view_lyrics(player)
    } else {
        match &player.view_data().kind {
            ViewKind::Search { .. } => search::view_search(player),
            ViewKind::SongRadio(_) | ViewKind::ArtistRadio(_) => search::view_search_radio(player),
            ViewKind::Artist { .. } | ViewKind::Album { .. } | ViewKind::PlaylistView { .. } => {
                search::view_browse(player)
            }
            ViewKind::Playlist { .. } | ViewKind::Downloads => playlist::view_playlist(player),
            ViewKind::Settings => settings::view_settings(player),
        }
    };

    let mut children: Vec<Element<'a, Message, AppTheme>> = vec![search_bar];
    if matches!(
        player.floating_search.as_ref().map(|fs| fs.list),
        Some(crate::app::TrackListKind::Active)
    ) {
        children.push(floating_search::view_floating_search(player));
    }
    children.push(inner);
    let base = Column::with_children(children);

    let mut stack = Stack::new().push(base);

    if player.show_search_history {
        let (input_x, input_width) = player.search_input_geometry();
        let dropdown = Container::new(search::view_search_history(player)).width(input_width);

        let positioned = Column::with_children([
            Container::new(Row::new())
                .height(theme::SEARCH_BAR_HEIGHT)
                .into(),
            Row::with_children([
                Container::new(Row::new()).width(input_x).into(),
                dropdown.into(),
            ])
            .into(),
        ]);

        stack = stack.push(positioned);
    }

    stack.into()
}
