use iced::{
    widget::{Column, Container, Row, Stack},
    Length,
};

use crate::{app::ViewKind, theme::AppTheme};

use super::{lyrics, playlist, search, theme, Element, Message, MusicPlayer};

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
        }
    };

    let base = Column::with_children([search_bar, inner])
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill);

    let mut stack = Stack::new()
        .width(Length::Fill)
        .height(Length::Fill)
        .push(base);

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
            .spacing(0)
            .width(Length::Fill)
            .height(Length::Fill)
            .into(),
        ])
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill);

        stack = stack.push(positioned);
    }

    stack.into()
}
