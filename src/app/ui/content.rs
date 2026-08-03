use iced::{
    alignment,
    widget::{scrollable, text_input, Button, Column, Container, Row, Stack, text},
    Color, Length,
};

use super::*;

pub(super) fn view_main_content<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let search_bar = view_search_bar(player);

    let content: Element<'a, Message> = match &player.current_view {
        View::Search => view_search(player),
        View::SongRadio | View::ArtistRadio => view_search_radio(player),
        View::Playlist => view_playlist(player),
        View::Downloads => view_playlist(player),
    };

    let inner = Container::new(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(bg(player.palette.bg));

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
        let dropdown =
            Container::new(view_search_history(player)).width(Length::Fixed(input_width));

        let positioned = Column::with_children(vec![
            Container::new(Row::new())
                .height(Length::Fixed(theme::SEARCH_BAR_HEIGHT))
                .into(),
            Row::with_children(vec![
                Container::new(Row::new())
                    .width(Length::Fixed(input_x))
                    .into(),
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

fn view_search_bar<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;
    let is_search_view = matches!(
        player.current_view,
        View::Search | View::SongRadio | View::ArtistRadio
    );

    let input = if is_search_view {
        Container::new(
            text_input("Search YouTube Music...", &player.search_query)
                .on_input(Message::SearchInputChanged)
                .on_submit(Message::SearchExecute)
                .padding([theme::SPACING_SM, theme::SPACING_MD])
                .size(theme::TEXT_SIZE_MD),
        )
        .width(Length::Fill)
        .into()
    } else {
        Container::new(
            text_input("Search YouTube Music...", &player.search_query)
                .on_input(Message::SearchInputChanged)
                .on_submit(Message::GlobalSearchSubmit)
                .padding([theme::SPACING_SM, theme::SPACING_MD])
                .size(theme::TEXT_SIZE_MD),
        )
        .width(Length::Fill)
        .into()
    };

    Container::new(
        Row::with_children(vec![
            input,
            Button::new(icons::icon("search.svg", Color::WHITE, theme::ICON_SIZE_MD))
                .padding(theme::SPACING_SM)
                .style(button_style_accent())
                .width(Length::Fixed(theme::SEARCH_BTN_SIZE))
                .height(Length::Fixed(theme::SEARCH_BTN_SIZE))
                .on_press(if is_search_view {
                    Message::SearchExecute
                } else {
                    Message::GlobalSearchSubmit
                })
                .into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .padding([theme::SPACING_LG, theme::SPACING_XL]),
    )
    .width(Length::Fill)
    .style(bg(p.bg_secondary))
    .into()
}

fn view_search<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let track_list = if player.search_loading && player.search_results.is_empty() {
        Container::new(
            text("Searching...")
                .size(theme::TEXT_SIZE_MD)
                .color(player.palette.fg_secondary)
                .center(),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(theme::SPACING_XL)
        .into()
    } else {
        view_track_list(&player.search_results, player, false)
    };

    let load_more = if !player.search_loading
        && !player.search_exhausted
        && !player.search_results.is_empty()
    {
        let btn = Button::new(
            text("Load More")
                .size(theme::TEXT_SIZE_SM)
                .color(Color::WHITE),
        )
        .padding(theme::SPACING_SM)
        .style(button_style_accent())
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

fn view_search_radio<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;

    let header = Container::new(
        text(player.radio_label.clone())
            .size(theme::TEXT_SIZE_DEFAULT)
            .color(p.fg)
            .width(Length::Fill)
            .center(),
    )
    .padding([theme::SPACING_SM, theme::SPACING_XL]);

    let track_list = if player.search_loading && player.radio_tracks.is_empty() {
        Container::new(
            text("Generating radio...")
                .size(theme::TEXT_SIZE_MD)
                .color(p.fg_secondary)
                .center(),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(theme::SPACING_XL)
        .into()
    } else {
        view_track_list(&player.radio_tracks, player, false)
    };

    Column::with_children(vec![header.into(), track_list])
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn view_search_history<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
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
        .style(bg(p.bg_secondary))
        .into();
    }

    let items: Vec<Element<'a, Message>> = player
        .last_filtered_history
        .iter()
        .enumerate()
        .map(|(i, q)| {
            let is_focused = player.search_history_focused_index == i;
            let bg_color = if is_focused {
                p.bg_hover
            } else {
                p.bg_secondary
            };
            let bg_hover = p.bg_hover;
            let is_focused_copy = is_focused;
            let sel_fg = p.fg;
            let sel_fg_secondary = if is_focused { p.fg } else { p.fg_secondary };
            let sel_bg_hover = bg_hover;
            let del_fg = p.fg;
            let del_fg_muted = p.fg_muted;
            let del_bg_hover = bg_hover;
            let del_bg = bg_color;

            Container::new(
                Row::with_children(vec![
                    Button::new(
                        Row::with_children(vec![
                            icons::icon("search.svg", del_fg_muted, theme::ICON_SIZE_SM).into(),
                            text(q)
                                .size(theme::TEXT_SIZE_SM)
                                .color(sel_fg_secondary)
                                .into(),
                        ])
                        .spacing(theme::SPACING_SM)
                        .padding([theme::SPACING_XS, theme::SPACING_MD])
                        .align_y(alignment::Vertical::Center)
                        .width(Length::Fill),
                    )
                    .width(Length::Fill)
                    .padding(0)
                    .style(move |_, status| {
                        let bg = if is_focused_copy {
                            bg_color
                        } else {
                            match status {
                                button::Status::Hovered | button::Status::Pressed => sel_bg_hover,
                                _ => bg_color,
                            }
                        };
                        button::Style {
                            background: Some(bg.into()),
                            text_color: sel_fg,
                            border: iced::border::rounded(theme::RADIUS_SM),
                            ..Default::default()
                        }
                    })
                    .on_press(Message::SearchHistorySelected(i))
                    .into(),
                    Button::new(icons::icon("delete.svg", del_fg_muted, theme::ICON_SIZE_SM))
                        .padding(2)
                        .style(move |_, status| {
                            let bg = match status {
                                button::Status::Hovered | button::Status::Pressed => del_bg_hover,
                                _ => del_bg,
                            };
                            button::Style {
                                background: Some(bg.into()),
                                text_color: del_fg,
                                border: iced::border::rounded(theme::RADIUS_SM),
                                ..Default::default()
                            }
                        })
                        .on_press(Message::DeleteSearchHistory(i))
                        .width(Length::Fixed(theme::DELETE_BTN_SIZE))
                        .height(Length::Fixed(theme::DELETE_BTN_SIZE))
                        .into(),
                ])
                .spacing(theme::SPACING_SM)
                .align_y(alignment::Vertical::Center),
            )
            .width(Length::Fill)
            .style(bg(bg_color))
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
        .style(bg(p.bg_secondary))
        .into()
}

fn view_playlist<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;

    let header: Element<'a, Message> = if let Some(idx) = player.selected_playlist {
        if let Some(pl) = player.playlists.playlists.get(idx) {
            let track_count = pl.tracks.len();
            Row::with_children(vec![
                text_input(&pl.name, &player.selected_playlist_name)
                    .on_input(Message::RenamePlaylist)
                    .size(theme::TEXT_SIZE_LG)
                    .padding([theme::SPACING_XS, theme::SPACING_SM])
                    .into(),
                text(format!("({} tracks)", track_count))
                    .size(theme::TEXT_SIZE_MD)
                    .color(p.fg_secondary)
                    .into(),
                icons::icon("edit.svg", p.fg_muted, theme::ICON_SIZE_SM).into(),
                Button::new(icons::icon("delete.svg", p.fg_muted, theme::ICON_SIZE_SM))
                    .padding(4)
                    .style(button_style_accent())
                    .on_press(Message::ShowDeleteConfirm(idx))
                    .into(),
            ])
            .spacing(theme::SPACING_SM)
            .align_y(alignment::Vertical::Center)
            .padding([theme::SPACING_MD, theme::SPACING_XL])
            .into()
        } else {
            Row::new().into()
        }
    } else {
        Container::new(
            text("Select a playlist from the sidebar")
                .size(theme::TEXT_SIZE_MD)
                .color(p.fg_secondary),
        )
        .padding(theme::SPACING_XL)
        .into()
    };

    let track_list = if let Some(idx) = player.selected_playlist {
        if let Some(pl) = player.playlists.playlists.get(idx) {
            view_track_list(&pl.tracks, player, false)
        } else {
            Container::new(Row::new())
                .width(Length::Fill)
                .height(Length::Fill)
                .into()
        }
    } else {
        Container::new(Row::new())
            .width(Length::Fill)
            .height(Length::Fill)
            .into()
    };

    Column::with_children(vec![
        Container::new(header).width(Length::Fill).into(),
        track_list,
    ])
    .spacing(0)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}