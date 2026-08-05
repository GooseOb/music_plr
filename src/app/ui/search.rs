use iced::{
    alignment,
    widget::{button, scrollable, text, text_input, Button, Column, Container, Row},
    Color, Element, Length,
};

use crate::{icons, theme::AppTheme};

use super::{
    styles::{bg_secondary, button_style_primary},
    theme, view_track_list, Message, MusicPlayer, View,
};

pub(super) fn view_search_bar(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let is_search_view = matches!(
        player.current_view,
        View::Search | View::SongRadio | View::ArtistRadio
    );
    let submit_msg = || {
        if is_search_view {
            Message::SearchExecute
        } else {
            Message::GlobalSearchSubmit
        }
    };

    let input = Container::new(
        text_input("Search YouTube Music...", &player.search_query)
            .on_input(Message::SearchInputChanged)
            .on_submit(submit_msg())
            .padding([theme::SPACING_SM, theme::SPACING_MD])
            .size(theme::TEXT_SIZE_MD),
    )
    .width(Length::Fill)
    .into();

    Container::new(
        Row::with_children(vec![
            input,
            Button::new(icons::icon("search.svg", Color::BLACK, theme::ICON_SIZE_MD))
                .padding(theme::SPACING_SM)
                .style(button_style_primary())
                .width(Length::Fixed(theme::SEARCH_BTN_SIZE))
                .height(Length::Fixed(theme::SEARCH_BTN_SIZE))
                .on_press(submit_msg())
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
    let track_list = view_search_results(
        player,
        &player.search_results,
        player.search_loading,
        "Searching...",
    );

    let load_more = if !player.search_loading
        && !player.search_exhausted
        && !player.search_results.is_empty()
    {
        let btn = Button::new(
            text("Load More")
                .size(theme::TEXT_SIZE_DEFAULT)
                .color(Color::WHITE),
        )
        .padding(theme::SPACING_SM)
        .width(Length::Fill)
        .on_press(Message::SearchLoadMore);
        Container::new(btn).padding(theme::SPACING_SM).into()
    } else {
        Container::new(Row::new()).height(Length::Fixed(0.0)).into()
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
                .size(theme::TEXT_SIZE_MD)
                .color(player.palette.fg_secondary)
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
    let header = Container::new(
        text(player.radio_label.clone())
            .size(theme::TEXT_SIZE_DEFAULT)
            .width(Length::Fill)
            .center(),
    )
    .padding([theme::SPACING_SM, theme::SPACING_XL]);

    let track_list = view_search_results(
        player,
        &player.radio_tracks,
        player.search_loading,
        "Generating radio...",
    );

    Column::with_children(vec![header.into(), track_list])
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

pub(super) fn view_search_history<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let p = &player.palette;

    if player.last_filtered_history.is_empty() {
        return Container::new(
            text("No recent searches")
                .size(theme::TEXT_SIZE_DEFAULT)
                .color(p.fg_secondary),
        )
        .width(Length::Fill)
        .height(Length::Fixed(theme::SEARCH_HISTORY_ITEM_HEIGHT))
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
                            icons::icon("search.svg", p.fg_muted, theme::ICON_SIZE_SM).into(),
                            text(q).size(theme::TEXT_SIZE_SM).into(),
                        ])
                        .spacing(theme::SPACING_SM)
                        .padding([theme::SPACING_XS, theme::SPACING_MD])
                        .align_y(alignment::Vertical::Center)
                        .width(Length::Fill),
                    )
                    .width(Length::Fill)
                    .padding(0)
                    .style(move |t, status| {
                        let p = &t.palette;
                        let bg = match status {
                            button::Status::Hovered | button::Status::Pressed => p.bg_hover,
                            _ => p.bg_secondary,
                        };
                        let text_color = match status {
                            button::Status::Hovered | button::Status::Pressed => p.fg,
                            _ => p.fg_secondary,
                        };
                        button::Style {
                            background: Some(bg.into()),
                            text_color,
                            border: iced::border::rounded(theme::RADIUS_SM),
                            ..Default::default()
                        }
                    })
                    .on_press(Message::SearchHistorySelected(i))
                    .into(),
                    Button::new(icons::icon("delete.svg", p.fg_muted, theme::ICON_SIZE_SM))
                        .padding(theme::SPACING_XS)
                        .style(move |t, status| {
                            let p = &t.palette;
                            let bg = match status {
                                button::Status::Hovered | button::Status::Pressed => p.bg_hover,
                                _ => p.bg_secondary,
                            };
                            button::Style {
                                background: Some(bg.into()),
                                border: iced::border::rounded(theme::RADIUS_SM),
                                ..Default::default()
                            }
                        })
                        .on_press(Message::DeleteSearchHistory(i))
                        .width(Length::Fixed(theme::DELETE_BTN_SIZE))
                        .height(Length::Fixed(theme::DELETE_BTN_SIZE))
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
        .height(Length::Fixed(dropdown_height));

    Container::new(content)
        .width(Length::Fill)
        .height(Length::Fixed(dropdown_height))
        .style(bg_secondary())
        .into()
}
