use iced::{
    alignment,
    widget::{scrollable, text, text_input, Button, Column, Container, Row},
    Color, Element, Length,
};

use crate::{app::ViewKind, icons, theme::AppTheme};

use super::{
    styles::{
        bg_secondary, button_style_delete, button_style_list_item, button_style_primary,
        fg_secondary,
    },
    theme, view_track_list, Message, MusicPlayer,
};

pub(super) fn view_search_bar(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let input = Container::new(
        text_input("Search YouTube Music...", &player.search_query)
            .on_input(Message::SearchInputChanged)
            .on_submit(Message::SearchExecute)
            .padding([theme::SPACING_SM, theme::SPACING_MD]),
    )
    .width(Length::Fill)
    .into();

    Container::new(
        Row::with_children(vec![
            input,
            Button::new(icons::icon(
                icons::SEARCH_ICON,
                Color::BLACK,
                theme::ICON_SIZE_MD,
            ))
            .padding(theme::SPACING_SM)
            .style(button_style_primary())
            .width(theme::SEARCH_BTN_SIZE)
            .height(theme::SEARCH_BTN_SIZE)
            .on_press(Message::SearchExecute)
            .into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .padding([theme::SPACING_LG, theme::SPACING_XL]),
    )
    .width(Length::Fill)
    .style(bg_secondary())
    .into()
}

pub(super) fn view_search(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let (results, loading, exhausted) = if matches!(player.view_data.kind, ViewKind::Search { .. })
    {
        (
            player.view_data.tracks.as_slice(),
            player.view_data.loading,
            player.view_data.exhausted(),
        )
    } else {
        (&[][..], false, false)
    };

    let track_list = view_search_results(player, results, loading, "Searching...");

    let load_more = if !loading && !exhausted && !results.is_empty() {
        let btn = Button::new(text("Load More").color(Color::WHITE))
            .padding(theme::SPACING_SM)
            .width(Length::Fill)
            .on_press(Message::SearchLoadMore);
        Container::new(btn).padding(theme::SPACING_SM).into()
    } else {
        Container::new(Row::new()).height(0.0).into()
    };

    Column::with_children(vec![track_list, load_more])
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn view_search_results<'a>(
    player: &'a MusicPlayer,
    tracks: &'a [crate::types::Track],
    loading: bool,
    loading_msg: &'a str,
) -> Element<'a, Message, AppTheme> {
    if loading && tracks.is_empty() {
        Container::new(
            text(loading_msg)
                .color(player.app_theme.palette.fg_secondary)
                .center(),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(theme::SPACING_XL)
        .into()
    } else {
        view_track_list(tracks, player, false, 0)
    }
}

pub(super) fn view_search_radio(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let (label, tracks, loading) = if matches!(
        player.view_data.kind,
        ViewKind::SongRadio(_) | ViewKind::ArtistRadio(_)
    ) {
        (
            player.view_data.label().to_string(),
            player.view_data.tracks.as_slice(),
            player.view_data.loading,
        )
    } else {
        (String::new(), &[][..], false)
    };

    let header = Container::new(text(label).width(Length::Fill).center())
        .padding([theme::SPACING_SM, theme::SPACING_XL]);

    let track_list = view_search_results(player, tracks, loading, "Generating radio...");

    Column::with_children(vec![header.into(), track_list])
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

pub(super) fn view_search_history<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;

    if player.last_filtered_history.is_empty() {
        return Container::new(text("No recent searches").style(fg_secondary()))
            .width(Length::Fill)
            .height(theme::SEARCH_HISTORY_ITEM_HEIGHT)
            .padding([theme::SPACING_SM, theme::SPACING_XL])
            .style(bg_secondary())
            .into();
    }

    let items: Vec<Element<'a, Message, AppTheme>> = player
        .last_filtered_history
        .iter()
        .enumerate()
        .map(|(i, q)| {
            Container::new(
                Row::with_children(vec![
                    Button::new(
                        Row::with_children(vec![
                            icons::icon(icons::SEARCH_ICON, p.fg_muted, theme::ICON_SIZE_SM).into(),
                            text(q).size(theme::TEXT_SIZE_SM).into(),
                        ])
                        .spacing(theme::SPACING_SM)
                        .padding([theme::SPACING_XS, theme::SPACING_MD])
                        .align_y(alignment::Vertical::Center)
                        .width(Length::Fill),
                    )
                    .width(Length::Fill)
                    .padding(0)
                    .style(button_style_list_item(false))
                    .on_press(Message::SearchHistorySelected(i))
                    .into(),
                    Button::new(icons::icon(
                        icons::DELETE_ICON,
                        p.fg_muted,
                        theme::ICON_SIZE_SM,
                    ))
                    .padding(theme::SPACING_XS)
                    .style(button_style_delete())
                    .on_press(Message::DeleteSearchHistory(i))
                    .width(theme::DELETE_BTN_SIZE)
                    .height(theme::DELETE_BTN_SIZE)
                    .into(),
                ])
                .align_y(alignment::Vertical::Center),
            )
            .width(Length::Fill)
            .style(bg_secondary())
            .into()
        })
        .collect();

    let dropdown_height = (player.last_filtered_history.len() as f32
        * theme::SEARCH_HISTORY_ITEM_HEIGHT)
        .min(theme::SEARCH_DROPDOWN_MAX_HEIGHT);

    let content = scrollable(Column::with_children(items).spacing(0).width(Length::Fill))
        .id(iced::widget::Id::new("search_history_list"))
        .width(Length::Fill)
        .height(dropdown_height);

    Container::new(content)
        .width(Length::Fill)
        .height(dropdown_height)
        .style(bg_secondary())
        .into()
}
