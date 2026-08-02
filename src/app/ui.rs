use crate::icons;
use crate::theme;
use crate::types::View;
use iced::{
    alignment,
    widget::{
        self, button, container, image, scrollable, slider, text, text_input, Button, Column,
        Container, MouseArea, Row, Stack,
    },
    Color, Element, Length,
};

use super::{ContextMenuState, Message, MusicPlayer};

fn scrollable_id(is_queue: bool) -> iced::widget::Id {
    if is_queue {
        iced::widget::Id::new("queue_list")
    } else {
        iced::widget::Id::new("track_list")
    }
}

fn bg(color: Color) -> impl Fn(&iced::Theme) -> container::Style + 'static {
    move |_| container::Style {
        background: Some(color.into()),
        ..Default::default()
    }
}

fn button_style(
    bg: Color,
    bg_hover: Color,
    text_color: Color,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    move |_, status| {
        let bg_color = match status {
            button::Status::Hovered | button::Status::Pressed => bg_hover,
            _ => bg,
        };
        button::Style {
            background: Some(bg_color.into()),
            text_color,
            border: iced::border::rounded(theme::RADIUS_SM),
            ..Default::default()
        }
    }
}

fn button_style_accent() -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    button_style(
        Color::from_rgb8(0x2a, 0x2a, 0x34),
        Color::from_rgb8(0x3a, 0x3a, 0x44),
        Color::WHITE,
    )
}

fn button_style_green() -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    button_style(
        Color::from_rgb8(0x14, 0xc8, 0x84),
        Color::from_rgba8(0x14, 0xc8, 0x84, 0.8),
        Color::BLACK,
    )
}

fn slider_style(
    accent: Color,
    bg_secondary: Color,
) -> impl Fn(&iced::Theme, slider::Status) -> slider::Style + 'static {
    move |_, status| {
        let color = match status {
            slider::Status::Active => accent,
            slider::Status::Hovered => accent,
            slider::Status::Dragged => accent,
        };
        slider::Style {
            rail: slider::Rail {
                backgrounds: (color.into(), bg_secondary.into()),
                width: 4.0,
                border: iced::border::rounded(2.0),
            },
            handle: slider::Handle {
                shape: slider::HandleShape::Circle { radius: 7.0 },
                background: color.into(),
                border_color: Color::TRANSPARENT,
                border_width: 0.0,
            },
        }
    }
}

fn thumbnail<'a>(
    track: &'a crate::types::Track,
    p: &theme::Palette,
    size: f32,
) -> Element<'a, Message> {
    let thumb_path = crate::thumbnails::thumbnail_path(&track.id);
    let fallback_color = p.fg_muted;
    if thumb_path.exists() {
        image(iced::widget::image::Handle::from_path(thumb_path))
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .content_fit(iced::ContentFit::Cover)
            .into()
    } else {
        icons::icon("music.svg", fallback_color, size).into()
    }
}

fn drop_indicator(color: Color) -> Container<'static, Message> {
    Container::new(Row::new())
        .width(Length::Fill)
        .height(Length::Fixed(crate::theme::DROP_LINE_HEIGHT))
        .style(move |_| container::Style {
            background: Some(color.into()),
            ..Default::default()
        })
}

pub fn view(player: &MusicPlayer) -> Element<'_, Message> {
    let p = &player.palette;

    let main_content = view_main_content(player);
    let sidebar = view_sidebar(player);
    let queue = if player.show_queue {
        view_queue_panel(player)
    } else {
        Container::new(Row::new()).width(Length::Fixed(0.0)).into()
    };

    let body = Row::with_children(vec![sidebar, main_content, queue])
        .height(Length::Fill)
        .align_y(alignment::Vertical::Top);

    let layout = Column::with_children(vec![
        view_notification(player),
        body.into(),
        view_playbar(player),
    ])
    .spacing(0);

    let main = Container::new(layout)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(bg(p.bg));

    let mut stack = Stack::new()
        .width(Length::Fill)
        .height(Length::Fill)
        .push(main);

    if player.show_playlist_picker.is_some() {
        stack = stack.push(view_playlist_picker(player));
    } else if player.show_delete_confirm {
        stack = stack.push(view_delete_confirm(player));
    } else if let Some(menu) = &player.context_menu {
        if menu.visible {
            stack = stack.push(view_context_menu(menu, &player.palette));
        }
    }

    stack.into()
}

fn view_notification(player: &MusicPlayer) -> Element<'_, Message> {
    if let Some(msg) = &player.notification {
        return Container::new(
            text(msg)
                .size(theme::TEXT_SIZE_DEFAULT)
                .color(Color::WHITE)
                .center(),
        )
        .width(Length::Fill)
        .padding([theme::SPACING_XS, theme::SPACING_XL])
        .style(bg(player.palette.warning))
        .into();
    }
    Container::new(Row::new())
        .width(Length::Fill)
        .height(Length::Fixed(0.0))
        .into()
}

fn view_sidebar<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;

    let nav_buttons = Row::with_children(vec![
        Button::new(
            Container::new(icons::icon("back.svg", p.fg, theme::ICON_SIZE_MD)).center(Length::Fill),
        )
        .padding(6)
        .style(button_style_accent())
        .width(Length::Fill)
        .height(Length::Fixed(theme::BUTTON_HEIGHT))
        .on_press_maybe(if player.can_navigate_back() {
            Some(Message::NavigateBack)
        } else {
            None
        })
        .into(),
        Button::new(
            Container::new(icons::icon("forward.svg", p.fg, theme::ICON_SIZE_MD))
                .center(Length::Fill),
        )
        .padding(6)
        .style(button_style_accent())
        .width(Length::Fill)
        .height(Length::Fixed(theme::BUTTON_HEIGHT))
        .on_press_maybe(if player.can_navigate_forward() {
            Some(Message::NavigateForward)
        } else {
            None
        })
        .into(),
    ])
    .spacing(theme::SPACING_XS)
    .align_y(alignment::Vertical::Center)
    .padding([theme::SPACING_MD, theme::SPACING_XL])
    .width(Length::Fill);

    let nav_items: Vec<Element<'a, Message>> = vec![
        sidebar_nav_item("Search", View::Search(String::new()), player, p),
        sidebar_nav_item("Downloads", View::Downloads, player, p),
    ];

    let playlist_items: Vec<Element<'a, Message>> = player
        .playlists
        .playlists
        .iter()
        .enumerate()
        .map(|(i, pl)| {
            let is_selected = matches!(player.current_view, View::Playlist(idx) if idx == i);
            let bg_color = if is_selected {
                p.bg_current
            } else {
                p.bg_secondary
            };
            let icon_color = if is_selected { p.accent } else { p.fg_muted };
            let text_color = if is_selected { p.fg } else { p.fg_secondary };
            let bg_hover = p.bg_hover;
            let is_hover = player.sidebar_hover_playlist == Some(i);

            Button::new(
                Row::with_children(vec![
                    icons::icon("music.svg", icon_color, theme::ICON_SIZE_SM).into(),
                    text(&pl.name)
                        .size(theme::TEXT_SIZE_DEFAULT)
                        .color(text_color)
                        .into(),
                ])
                .spacing(10)
                .padding([theme::SPACING_SM, theme::SPACING_MD])
                .align_y(alignment::Vertical::Center)
                .width(Length::Fill),
            )
            .width(Length::Fill)
            .padding(0)
            .style(move |_, status| {
                let bg = if is_selected {
                    bg_color
                } else if is_hover {
                    p.accent
                } else {
                    match status {
                        button::Status::Hovered | button::Status::Pressed => bg_hover,
                        _ => bg_color,
                    }
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color,
                    border: iced::border::rounded(theme::RADIUS_SM),
                    ..Default::default()
                }
            })
            .on_press(Message::SelectPlaylist(i))
            .into()
        })
        .collect();

    let create_row = Row::with_children(vec![
        Container::new(
            text_input("New playlist name", &player.playlist_create_name)
                .on_input(Message::NewPlaylistNameChanged)
                .padding([theme::SPACING_XS, theme::SPACING_SM])
                .size(theme::TEXT_SIZE_DEFAULT),
        )
        .width(Length::Fill)
        .into(),
        Button::new(icons::icon("folder.svg", Color::WHITE, theme::ICON_SIZE_SM))
            .padding(6)
            .style(button_style_accent())
            .on_press(Message::CreatePlaylist)
            .into(),
    ])
    .align_y(alignment::Vertical::Center)
    .spacing(6)
    .padding([theme::SPACING_SM, theme::SPACING_MD]);

    let import_btn = Button::new(
        Row::with_children(vec![
            icons::icon("folder.svg", Color::WHITE, theme::ICON_SIZE_SM).into(),
            text("Local Music")
                .size(theme::TEXT_SIZE_DEFAULT)
                .color(Color::WHITE)
                .into(),
        ])
        .spacing(6)
        .align_y(alignment::Vertical::Center),
    )
    .padding(theme::SPACING_SM)
    .width(Length::Fill)
    .style(button_style_accent())
    .on_press(Message::AddLocalMusic);

    let sidebar_content = Column::with_children(vec![
        nav_buttons.into(),
        Container::new(widget::rule::horizontal(1))
            .width(Length::Fill)
            .padding([theme::SPACING_SM, 0.0])
            .into(),
        Column::with_children(nav_items).spacing(2).into(),
        Container::new(widget::rule::horizontal(1))
            .width(Length::Fill)
            .padding([theme::SPACING_SM, 0.0])
            .into(),
        scrollable(
            Column::with_children(playlist_items)
                .spacing(0)
                .width(Length::Fill),
        )
        .id(iced::widget::Id::new("sidebar_playlist_list"))
        .on_scroll(|vp| Message::SidebarListScrolled {
            offset_y: vp.absolute_offset().y,
            bounds: vp.bounds(),
        })
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
        Container::new(widget::rule::horizontal(1))
            .width(Length::Fill)
            .padding([theme::SPACING_SM, 0.0])
            .into(),
        create_row.into(),
        import_btn.into(),
    ]);

    Container::new(sidebar_content)
        .width(Length::Fixed(theme::SIDEBAR_WIDTH))
        .height(Length::Fill)
        .style(bg(p.bg_secondary))
        .into()
}

fn sidebar_nav_item<'a>(
    name: &'a str,
    view: View,
    player: &'a MusicPlayer,
    p: &theme::Palette,
) -> Element<'a, Message> {
    let is_active = player.current_view == view;
    let bg_color = if is_active {
        p.bg_current
    } else {
        p.bg_secondary
    };
    let icon_color = if is_active { p.accent } else { p.fg_muted };
    let text_color = if is_active { p.fg } else { p.fg_secondary };
    let bg_hover = p.bg_hover;
    let icon_name: &'a str = match &view {
        View::Search(_) => "search.svg",
        View::SongRadio(_) => "radio.svg",
        View::ArtistRadio(_) => "radio.svg",
        View::Playlist(_) => "music.svg",
        View::Downloads => "download.svg",
    };

    Button::new(
        Row::with_children(vec![
            icons::icon(icon_name, icon_color, theme::ICON_SIZE_MD).into(),
            text(name)
                .size(theme::TEXT_SIZE_DEFAULT)
                .color(text_color)
                .into(),
        ])
        .spacing(10)
        .padding([theme::SPACING_LG, theme::SPACING_XL])
        .align_y(alignment::Vertical::Center)
        .width(Length::Fill),
    )
    .width(Length::Fill)
    .padding(0)
    .style(move |_, status| {
        let bg = if is_active {
            bg_color
        } else {
            match status {
                button::Status::Hovered | button::Status::Pressed => bg_hover,
                _ => bg_color,
            }
        };
        button::Style {
            background: Some(bg.into()),
            text_color,
            border: iced::border::rounded(theme::RADIUS_LG),
            ..Default::default()
        }
    })
    .on_press(Message::NavigateTo(view))
    .into()
}

fn view_main_content<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let search_bar = view_search_bar(player);

    let content: Element<'a, Message> = match &player.current_view {
        View::Search(_) => view_search(player),
        View::SongRadio(_) | View::ArtistRadio(_) => view_search_radio(player),
        View::Playlist(_) => view_playlist(player),
        View::Downloads => view_playlist(player),
    };

    let inner = Container::new(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .style(bg(player.palette.bg));

    Column::with_children(vec![search_bar, inner.into()])
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn view_search_bar<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;
    let is_search_view = matches!(
        player.current_view,
        View::Search(_) | View::SongRadio(_) | View::ArtistRadio(_)
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
    let history_dropdown = if player.show_search_history {
        view_search_history(player)
    } else {
        Container::new(Row::new())
            .width(Length::Fill)
            .height(Length::Fixed(0.0))
            .into()
    };

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
        && player.search_results.len() >= player.search_offset
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

    Column::with_children(vec![history_dropdown, track_list, load_more])
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
        .padding([theme::SPACING_SM, theme::SPACING_XL])
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

    Container::new(Column::with_children(items).spacing(0).width(Length::Fill))
        .width(Length::Fill)
        .max_width(theme::MIN_TRACK_WIDTH)
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

fn view_track_list<'a>(
    tracks: &'a [crate::types::Track],
    player: &'a MusicPlayer,
    is_queue: bool,
) -> Element<'a, Message> {
    if tracks.is_empty() {
        return Container::new(
            text("No tracks found")
                .size(theme::TEXT_SIZE_MD)
                .color(player.palette.fg_secondary)
                .center(),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(theme::SPACING_XL)
        .into();
    }

    let mut items: Vec<Element<'a, Message>> = Vec::with_capacity(tracks.len());
    for (i, track) in tracks.iter().enumerate() {
        if player.drag_active && player.pressed_track_is_queue == is_queue {
            if let Some(drop_idx) = player.drag_drop_target {
                if i == drop_idx {
                    items.push(drop_indicator(player.palette.accent).into());
                }
            }
        }
        items.push(view_track_row(track, i, player, is_queue));
    }

    if player.drag_active && player.pressed_track_is_queue == is_queue {
        if let Some(drop_idx) = player.drag_drop_target {
            if drop_idx == tracks.len() {
                items.push(drop_indicator(player.palette.accent).into());
            }
        }
    }

    let list_id = scrollable_id(is_queue);

    Container::new(
        scrollable(Column::with_children(items).spacing(0).width(Length::Fill))
            .id(list_id)
            .on_scroll(move |vp| Message::ListScrolled {
                offset_y: vp.absolute_offset().y,
                bounds: vp.bounds(),
                is_queue,
            })
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn view_track_row<'a>(
    track: &'a crate::types::Track,
    index: usize,
    player: &'a MusicPlayer,
    is_queue: bool,
) -> Element<'a, Message> {
    let p = &player.palette;
    let is_selected = player.selection(is_queue).contains(&index);
    let is_hovered = player.hovered_track == Some((index, is_queue));
    let row_bg = if is_selected {
        p.bg_selected
    } else if is_hovered {
        p.bg_hover
    } else {
        p.bg
    };

    let duration_text = crate::util::format_duration(track.duration);

    let leading: Element<'a, Message> = if is_hovered {
        Button::new(icons::icon("play.svg", Color::BLACK, theme::ICON_SIZE_LG))
            .padding(6)
            .style(button_style_green())
            .on_press(Message::PlayTrackAtIndex { index, is_queue })
            .into()
    } else {
        text((index + 1).to_string())
            .size(theme::TEXT_SIZE_SM)
            .color(p.fg_secondary)
            .width(Length::Fixed(theme::TRACK_LEADING_WIDTH))
            .center()
            .into()
    };

    let title_artist = Column::with_children(vec![
        text(track.title.clone())
            .size(theme::TEXT_SIZE_DEFAULT)
            .color(p.fg)
            .width(Length::Fill)
            .into(),
        text(track.artist.clone())
            .size(theme::TEXT_SIZE_SM)
            .color(p.fg_secondary)
            .width(Length::Fill)
            .into(),
    ])
    .spacing(2);

    let content = Row::with_children(vec![
        Container::new(leading)
            .width(Length::Fixed(theme::TRACK_LEADING_WIDTH))
            .into(),
        Container::new(thumbnail(track, p, theme::THUMBNAIL_SIZE))
            .width(Length::Fixed(theme::THUMBNAIL_SIZE))
            .height(Length::Fixed(theme::THUMBNAIL_SIZE))
            .into(),
        Container::new(title_artist).width(Length::Fill).into(),
        text(duration_text)
            .size(theme::TEXT_SIZE_SM)
            .color(p.fg_secondary)
            .width(Length::Fixed(theme::DURATION_WIDTH))
            .into(),
    ])
    .spacing(theme::SPACING_SM)
    .align_y(alignment::Vertical::Center)
    .padding([theme::SPACING_XS, theme::SPACING_MD]);

    let track_area = MouseArea::new(content)
        .on_press(Message::TrackPressed { index, is_queue })
        .on_right_press(Message::TrackRightClicked { index, is_queue })
        .on_move(move |_| Message::TrackHoverStart { index, is_queue });

    Container::new(track_area)
        .width(Length::Fill)
        .height(Length::Fixed(theme::ROW_HEIGHT))
        .style(bg(row_bg))
        .into()
}

fn view_queue_panel<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;
    let queue_width = (player.window_width * theme::QUEUE_WIDTH_RATIO).max(theme::QUEUE_MIN_WIDTH);

    let header = Container::new(
        Row::with_children(vec![
            text("Queue")
                .size(theme::TEXT_SIZE_MD)
                .color(p.fg_secondary)
                .into(),
            icons::icon("sync.svg", p.fg_muted, theme::ICON_SIZE_SM).into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .padding([theme::SPACING_SM, theme::SPACING_MD]),
    )
    .width(Length::Fill);

    let track_list = if player.queue.tracks.is_empty() {
        Container::new(
            text("Queue is empty")
                .size(theme::TEXT_SIZE_SM)
                .color(p.fg_secondary),
        )
        .padding(theme::SPACING_LG)
        .into()
    } else {
        view_track_list(&player.queue.tracks, player, true)
    };

    Container::new(
        Column::with_children(vec![header.into(), track_list])
            .spacing(0)
            .width(Length::Fill),
    )
    .width(Length::Fixed(queue_width))
    .height(Length::Fill)
    .style(bg(p.bg_secondary))
    .into()
}

fn view_playbar<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;

    let track = player.queue.current();
    let title = track.map(|t| t.title.as_str()).unwrap_or("Not playing");
    let artist = track.map(|t| t.artist.as_str()).unwrap_or("");

    let play_pause_icon = if player.is_playing {
        "pause.svg"
    } else {
        "play.svg"
    };

    let track_thumb: Element<'a, Message> = if let Some(t) = track {
        thumbnail(t, p, theme::PLAYBAR_THUMBNAIL_SIZE)
    } else {
        icons::icon("music.svg", p.fg_muted, theme::PLAYBAR_THUMBNAIL_SIZE).into()
    };

    let track_info = Column::with_children(vec![
        text(title)
            .size(theme::TEXT_SIZE_DEFAULT)
            .color(p.fg)
            .into(),
        text(artist)
            .size(theme::TEXT_SIZE_SM)
            .color(p.fg_secondary)
            .into(),
    ])
    .spacing(2);

    let elapsed_text = text(player.elapsed_text.clone())
        .size(theme::TEXT_SIZE_XS)
        .color(p.fg_secondary)
        .width(Length::Fixed(theme::TIME_TEXT_WIDTH))
        .center();

    let total_text = text(player.total_text.clone())
        .size(theme::TEXT_SIZE_XS)
        .color(p.fg_secondary)
        .width(Length::Fixed(theme::TIME_TEXT_WIDTH))
        .center();

    let controls = Container::new(
        Row::with_children(vec![
            Button::new(icons::icon("skip-back.svg", p.fg, theme::ICON_SIZE_MD))
                .padding(6)
                .style(button_style_accent())
                .on_press(Message::PreviousTrack)
                .into(),
            Button::new(icons::icon(
                play_pause_icon,
                Color::BLACK,
                theme::ICON_SIZE_LG,
            ))
            .padding(theme::SPACING_SM)
            .style(button_style_green())
            .on_press(Message::TogglePlayPause)
            .into(),
            Button::new(icons::icon("skip-forward.svg", p.fg, theme::ICON_SIZE_MD))
                .padding(6)
                .style(button_style_accent())
                .on_press(Message::NextTrack)
                .into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center),
    )
    .center_x(Length::Fill);

    let progress = slider(0.0..=1.0, player.progress, Message::Seek)
        .width(Length::Fill)
        .step(0.01f32)
        .style(slider_style(p.accent, p.bg_secondary));

    let controls_and_progress =
        Column::with_children(vec![controls.into(), progress.into()]).spacing(theme::SPACING_XS);

    let volume_slider = slider(0.0..=1.0, player.volume, Message::SetVolume)
        .width(Length::Fixed(theme::VOLUME_SLIDER_WIDTH))
        .step(0.01f32)
        .style(slider_style(p.accent, p.bg_secondary));

    let queue_btn = Button::new(icons::icon("queue.svg", p.fg_muted, theme::ICON_SIZE_MD))
        .padding(6)
        .style(button_style_accent())
        .on_press(Message::ToggleQueue)
        .width(Length::Fixed(theme::QUEUE_BTN_WIDTH))
        .height(Length::Fixed(theme::BUTTON_HEIGHT));

    Container::new(
        Row::with_children(vec![
            Container::new(track_thumb)
                .width(Length::Fixed(theme::PLAYBAR_THUMBNAIL_SIZE))
                .height(Length::Fixed(theme::PLAYBAR_THUMBNAIL_SIZE))
                .into(),
            Container::new(track_info)
                .width(Length::Fixed(theme::PLAYBAR_TRACK_INFO_WIDTH))
                .into(),
            Container::new(elapsed_text).into(),
            Container::new(controls_and_progress)
                .width(Length::Fill)
                .into(),
            Container::new(total_text).into(),
            icons::icon("volume.svg", p.fg_secondary, theme::ICON_SIZE_SM).into(),
            volume_slider.into(),
            queue_btn.into(),
        ])
        .spacing(theme::SPACING_MD)
        .align_y(alignment::Vertical::Center)
        .padding([theme::SPACING_SM, theme::SPACING_MD]),
    )
    .width(Length::Fill)
    .style(bg(p.bg_secondary))
    .into()
}

fn transparent_bg() -> impl Fn(&iced::Theme) -> container::Style + 'static {
    |_| container::Style {
        background: None,
        ..Default::default()
    }
}

fn menu_bg(bg_color: Color) -> impl Fn(&iced::Theme) -> container::Style + 'static {
    move |_| container::Style {
        background: Some(bg_color.into()),
        border: iced::border::rounded(theme::RADIUS_MD),
        ..Default::default()
    }
}

fn view_context_menu<'a>(
    menu: &'a ContextMenuState,
    p: &'a theme::Palette,
) -> Element<'a, Message> {
    let pos_x = menu.position.0;
    let pos_y = menu.position.1;

    let items: Vec<Element<'_, Message>> = {
        let mut v: Vec<Element<'_, Message>> = vec![];

        v.push(
            menu_item(
                "Play",
                "play.svg",
                p,
                Message::ContextMenuPlayTrack(menu.track_index),
            )
            .width(Length::Fill)
            .into(),
        );

        if menu.is_youtube {
            v.push(
                menu_item(
                    "Song Radio",
                    "radio.svg",
                    p,
                    Message::ContextMenuStartSongRadio(menu.track_index),
                )
                .width(Length::Fill)
                .into(),
            );
            v.push(
                menu_item(
                    "Artist Radio",
                    "radio.svg",
                    p,
                    Message::ContextMenuStartArtistRadio(menu.track_index),
                )
                .width(Length::Fill)
                .into(),
            );
        }

        v.push(
            menu_item(
                "Add to Playlist",
                "folder.svg",
                p,
                Message::TogglePicker(menu.track_index),
            )
            .width(Length::Fill)
            .into(),
        );

        if menu.is_youtube {
            let label = if menu.is_downloaded {
                "Delete Download"
            } else {
                "Download"
            };
            v.push(
                menu_item(
                    label,
                    "download.svg",
                    p,
                    Message::ContextMenuDownloadOrDelete(menu.track_index),
                )
                .width(Length::Fill)
                .into(),
            );
        }

        if menu.is_queue {
            v.push(
                menu_item(
                    "Remove from Queue",
                    "delete.svg",
                    p,
                    Message::ContextMenuRemoveFromQueue(menu.track_index),
                )
                .width(Length::Fill)
                .into(),
            );
        } else if menu.in_playlist {
            v.push(
                menu_item(
                    "Remove from Playlist",
                    "delete.svg",
                    p,
                    Message::ContextMenuRemoveFromPlaylist(menu.track_index),
                )
                .width(Length::Fill)
                .into(),
            );
        }

        v
    };

    let menu_content = Container::new(
        Column::with_children(items)
            .spacing(2)
            .padding(theme::SPACING_SM),
    )
    .width(Length::Fixed(theme::CONTEXT_MENU_WIDTH))
    .style(menu_bg(p.bg_secondary));

    let row = Row::with_children(vec![
        Container::new(Row::new())
            .width(Length::Fixed(pos_x))
            .into(),
        menu_content.into(),
    ])
    .spacing(0);

    let col = Column::with_children(vec![
        Container::new(Column::new())
            .height(Length::Fixed(pos_y))
            .into(),
        row.into(),
    ])
    .spacing(0);

    Container::new(MouseArea::new(col).on_press(Message::CloseContextMenu))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(transparent_bg())
        .into()
}

fn menu_item<'a>(
    label: &'a str,
    icon: &'a str,
    p: &'a theme::Palette,
    on_press: Message,
) -> Container<'a, Message> {
    Container::new(
        Button::new(
            Row::with_children(vec![
                icons::icon(icon, p.fg_muted, theme::ICON_SIZE_SM).into(),
                text(label)
                    .size(theme::TEXT_SIZE_DEFAULT)
                    .color(p.fg)
                    .into(),
            ])
            .spacing(theme::SPACING_SM)
            .padding([theme::SPACING_XS, theme::SPACING_SM])
            .align_y(alignment::Vertical::Center)
            .width(Length::Fill),
        )
        .width(Length::Fill)
        .padding(0)
        .style(move |_, status| {
            let bg = match status {
                button::Status::Hovered | button::Status::Pressed => p.bg_hover,
                _ => p.bg_secondary,
            };
            button::Style {
                background: Some(bg.into()),
                text_color: p.fg,
                border: iced::border::rounded(theme::RADIUS_SM),
                ..Default::default()
            }
        })
        .on_press(on_press),
    )
    .width(Length::Fill)
    .style(bg(p.bg_secondary))
}

fn view_playlist_picker<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;

    let playlists: Vec<&crate::playlists::Playlist> = player.playlists.playlists.iter().collect();

    let items: Vec<Element<'a, Message>> = playlists
        .iter()
        .enumerate()
        .map(|(i, pl)| {
            let is_focused = player.picker_focused_index == i;
            let bg_color = if is_focused {
                p.bg_hover
            } else {
                p.bg_secondary
            };
            let bg_hover = p.bg_hover;
            let is_focused_copy = is_focused;

            Button::new(
                Row::with_children(vec![text(&pl.name)
                    .size(theme::TEXT_SIZE_DEFAULT)
                    .color(p.fg)
                    .into()])
                .spacing(theme::SPACING_SM)
                .padding([theme::SPACING_SM, theme::SPACING_MD])
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
                        button::Status::Hovered | button::Status::Pressed => bg_hover,
                        _ => bg_color,
                    }
                };
                button::Style {
                    background: Some(bg.into()),
                    text_color: p.fg,
                    border: iced::border::rounded(theme::RADIUS_SM),
                    ..Default::default()
                }
            })
            .on_press(Message::AddToPlaylist(i))
            .into()
        })
        .collect();

    let cancel_btn = Button::new(
        Container::new(text("Cancel").size(theme::TEXT_SIZE_SM).color(Color::WHITE)).padding(4),
    )
    .padding(theme::SPACING_SM)
    .width(Length::Fixed(theme::BUTTON_WIDTH))
    .style(button_style_accent())
    .on_press(Message::ClosePicker);

    let dialog = Container::new(
        Column::with_children(vec![
            text("Add to Playlist")
                .size(theme::TEXT_SIZE_LG)
                .color(p.fg)
                .into(),
            Column::with_children(items)
                .spacing(0)
                .width(Length::Fill)
                .into(),
            cancel_btn.into(),
        ])
        .spacing(theme::SPACING_SM)
        .padding(0),
    )
    .width(Length::Fixed(theme::DIALOG_WIDTH))
    .height(Length::Fill)
    .style(bg(p.bg_secondary));

    Container::new(MouseArea::new(dialog).on_press(Message::ClosePicker))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(bg(p.overlay))
        .into()
}

fn view_delete_confirm<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;

    let cancel_btn = Button::new(
        Container::new(text("Cancel").size(theme::TEXT_SIZE_SM).color(Color::WHITE)).padding(4),
    )
    .padding(theme::SPACING_SM)
    .width(Length::Fixed(theme::BUTTON_WIDTH))
    .style(button_style_accent())
    .on_press(Message::HideDeleteConfirm);

    let delete_btn = Button::new(
        Container::new(text("Delete").size(theme::TEXT_SIZE_SM).color(Color::WHITE)).padding(4),
    )
    .padding(theme::SPACING_SM)
    .width(Length::Fixed(theme::BUTTON_WIDTH))
    .style(button::danger)
    .on_press(Message::ConfirmDeletePlaylist);

    let dialog = Container::new(
        Column::with_children(vec![
            text("Delete playlist?")
                .size(theme::TEXT_SIZE_LG)
                .color(p.fg)
                .into(),
            text("Tracks will not be deleted.")
                .size(theme::TEXT_SIZE_DEFAULT)
                .color(p.fg_secondary)
                .into(),
            Row::with_children(vec![cancel_btn.into(), delete_btn.into()])
                .spacing(theme::SPACING_SM)
                .align_y(alignment::Vertical::Center)
                .into(),
        ])
        .spacing(theme::SPACING_MD)
        .padding(theme::SPACING_XL),
    )
    .width(Length::Fixed(theme::DIALOG_WIDTH))
    .height(Length::Fixed(theme::DIALOG_HEIGHT))
    .style(bg(p.bg_secondary));

    Container::new(MouseArea::new(dialog))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(bg(p.overlay))
        .into()
}
