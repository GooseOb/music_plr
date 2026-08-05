use iced::{
    widget::{Column, Container, Row, Stack},
    Length,
};

use crate::theme::AppTheme;

use super::{playlist, search, theme, Element, Message, MusicPlayer, View};

pub(super) fn view_main_content<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let search_bar = search::view_search_bar(player);

    let content: Element<'a, Message, AppTheme> = match &player.current_view {
        View::Search => search::view_search(player),
        View::SongRadio | View::ArtistRadio => search::view_search_radio(player),
        View::Playlist | View::Downloads => playlist::view_playlist(player),
    };

    let inner = Container::new(content)
        .width(Length::Fill)
        .height(Length::Fill);

    let base = Column::with_children(vec![search_bar, inner.into()])
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

        let positioned = Column::with_children(vec![
            Container::new(Row::new())
                .height(theme::SEARCH_BAR_HEIGHT)
                .into(),
            Row::with_children(vec![
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
