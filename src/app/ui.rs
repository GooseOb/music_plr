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

fn bg(color: Color) -> impl Fn(&iced::Theme) -> container::Style + 'static {
    move |_| container::Style {
        background: Some(color.into()),
        ..Default::default()
    }
}

fn button_style(
    bg: Color,
    text_color: Color,
) -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    move |_, _| button::Style {
        background: Some(bg.into()),
        text_color,
        border: iced::border::rounded(4.0),
        ..Default::default()
    }
}

fn button_style_accent() -> impl Fn(&iced::Theme, button::Status) -> button::Style + 'static {
    button_style(Color::from_rgb8(0x2a, 0x2a, 0x34), Color::WHITE)
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

fn thumbnail<'a>(track: &'a crate::types::Track, p: &theme::Palette) -> Element<'a, Message> {
    let thumb_path = crate::thumbnails::thumbnail_path(&track.id);
    let fallback_color = p.fg_muted;
    if thumb_path.exists() {
        image(iced::widget::image::Handle::from_path(thumb_path))
            .width(Length::Fixed(theme::THUMBNAIL_SIZE))
            .height(Length::Fixed(theme::THUMBNAIL_SIZE))
            .content_fit(iced::ContentFit::Contain)
            .into()
    } else {
        icons::icon("music.svg", fallback_color, theme::THUMBNAIL_SIZE).into()
    }
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
        return Container::new(text(msg).size(13).color(Color::WHITE).center())
            .width(Length::Fill)
            .padding([6, 16])
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
        Button::new(Container::new(icons::icon("back.svg", p.fg, 16.0)).padding(4))
            .padding(6)
            .style(button_style_accent())
            .width(Length::Fixed(36.0))
            .height(Length::Fixed(28.0))
            .on_press_maybe(if player.can_navigate_back() {
                Some(Message::NavigateBack)
            } else {
                None
            })
            .into(),
        Button::new(Container::new(icons::icon("forward.svg", p.fg, 16.0)).padding(4))
            .padding(6)
            .style(button_style_accent())
            .width(Length::Fixed(36.0))
            .height(Length::Fixed(28.0))
            .on_press_maybe(if player.can_navigate_forward() {
                Some(Message::NavigateForward)
            } else {
                None
            })
            .into(),
    ])
    .spacing(4)
    .align_y(alignment::Vertical::Center)
    .padding([4, 12]);

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
            Container::new(
                MouseArea::new(
                    Row::with_children(vec![
                        icons::icon("music.svg", p.fg_muted, 14.0).into(),
                        text(&pl.name)
                            .size(13)
                            .color(if is_selected { p.fg } else { p.fg_secondary })
                            .into(),
                    ])
                    .spacing(10)
                    .padding([8, 12])
                    .align_y(alignment::Vertical::Center),
                )
                .on_press(Message::SelectPlaylist(i)),
            )
            .width(Length::Fill)
            .height(Length::Fixed(theme::SIDEBAR_ITEM_HEIGHT))
            .style(bg(bg_color))
            .into()
        })
        .collect();

    let create_row = Row::with_children(vec![
        Container::new(
            text_input("New playlist name", &player.playlist_create_name)
                .on_input(Message::NewPlaylistNameChanged)
                .padding([6, 8])
                .size(13),
        )
        .width(Length::Fill)
        .into(),
        Button::new(icons::icon("folder.svg", Color::WHITE, 14.0))
            .padding(6)
            .style(button_style_accent())
            .on_press(Message::CreatePlaylist)
            .into(),
    ])
    .align_y(alignment::Vertical::Center)
    .spacing(6)
    .padding([8, 12]);

    let import_btn = Button::new(
        Row::with_children(vec![
            icons::icon("folder.svg", Color::WHITE, 14.0).into(),
            text("Local Music").size(13).color(Color::WHITE).into(),
        ])
        .spacing(6)
        .align_y(alignment::Vertical::Center),
    )
    .padding(8)
    .width(Length::Fill)
    .style(button_style_accent())
    .on_press(Message::AddLocalMusic);

    let sidebar_content = Column::with_children(vec![
        nav_buttons.into(),
        Column::with_children(nav_items).spacing(2).into(),
        Container::new(widget::rule::horizontal(1))
            .width(Length::Fill)
            .padding([8, 0])
            .into(),
        scrollable(
            Column::with_children(playlist_items)
                .spacing(0)
                .width(Length::Fill),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .into(),
        Container::new(widget::rule::horizontal(1))
            .width(Length::Fill)
            .padding([8, 0])
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
    let icon_name = match &view {
        View::Search(_) => "search.svg",
        View::SongRadio(_) => "radio.svg",
        View::ArtistRadio(_) => "radio.svg",
        View::Playlist(_) => "music.svg",
        View::Downloads => "download.svg",
    };
    let icon_color = if is_active { p.accent } else { p.fg_muted };
    let text_color = if is_active { p.fg } else { p.fg_secondary };

    Container::new(
        MouseArea::new(
            Row::with_children(vec![
                icons::icon(icon_name, icon_color, 16.0).into(),
                text(name).size(13).color(text_color).into(),
            ])
            .spacing(10)
            .padding([10, 16])
            .align_y(alignment::Vertical::Center),
        )
        .on_press(Message::NavigateTo(view)),
    )
    .width(Length::Fill)
    .style(bg(bg_color))
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
                .padding([8, 12])
                .size(14),
        )
        .width(Length::Fill)
        .into()
    } else {
        Container::new(
            text_input("Search YouTube Music...", &player.search_query)
                .on_input(Message::SearchInputChanged)
                .on_submit(Message::GlobalSearchSubmit)
                .padding([8, 12])
                .size(14),
        )
        .width(Length::Fill)
        .into()
    };

    Container::new(
        Row::with_children(vec![
            input,
            Button::new(icons::icon("search.svg", Color::WHITE, 16.0))
                .padding(8)
                .style(button_style_accent())
                .on_press(if is_search_view {
                    Message::SearchExecute
                } else {
                    Message::GlobalSearchSubmit
                })
                .into(),
        ])
        .spacing(8)
        .align_y(alignment::Vertical::Center)
        .padding([10, 16]),
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
                .size(14)
                .color(player.palette.fg_secondary)
                .center(),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .into()
    } else {
        view_track_list(&player.search_results, player)
    };

    let load_more = if !player.search_loading
        && player.search_results.len() >= player.search_offset
        && !player.search_results.is_empty()
    {
        let btn = Button::new(text("Load More").size(12).color(Color::WHITE))
            .padding(8)
            .style(button_style_accent())
            .on_press(Message::SearchLoadMore);
        Container::new(btn).padding(8).into()
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
            .size(15)
            .color(p.fg)
            .width(Length::Fill)
            .center(),
    )
    .padding([8, 16]);

    let track_list = if player.search_loading && player.radio_tracks.is_empty() {
        Container::new(
            text("Generating radio...")
                .size(14)
                .color(p.fg_secondary)
                .center(),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .into()
    } else {
        view_track_list(&player.radio_tracks, player)
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
        return Container::new(text("No recent searches").size(13).color(p.fg_secondary))
            .padding([8, 32])
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
            Container::new(
                Row::with_children(vec![
                    Container::new(
                        MouseArea::new(
                            Row::with_children(vec![
                                icons::icon("search.svg", p.fg_muted, 12.0).into(),
                                text(q)
                                    .size(12)
                                    .color(if is_focused { p.fg } else { p.fg_secondary })
                                    .into(),
                            ])
                            .spacing(8),
                        )
                        .on_press(Message::SearchHistorySelected(i)),
                    )
                    .width(Length::Fill)
                    .into(),
                    MouseArea::new(icons::icon("delete.svg", p.fg_muted, 12.0))
                        .on_press(Message::DeleteSearchHistory(i))
                        .into(),
                ])
                .spacing(8)
                .padding([6, 12])
                .align_y(alignment::Vertical::Center),
            )
            .width(Length::Fill)
            .style(bg(bg_color))
            .into()
        })
        .collect();

    Container::new(Column::with_children(items).spacing(0).width(Length::Fill))
        .width(Length::Fill)
        .max_width(400.0)
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
                    .size(16)
                    .padding([4, 8])
                    .into(),
                text(format!("({} tracks)", track_count))
                    .size(14)
                    .color(p.fg_secondary)
                    .into(),
                icons::icon("edit.svg", p.fg_muted, 14.0).into(),
                Button::new(icons::icon("delete.svg", p.fg_muted, 14.0))
                    .padding(4)
                    .style(button_style_accent())
                    .on_press(Message::ShowDeleteConfirm(idx))
                    .into(),
            ])
            .spacing(8)
            .align_y(alignment::Vertical::Center)
            .padding([12, 16])
            .into()
        } else {
            Row::new().into()
        }
    } else {
        Container::new(
            text("Select a playlist from the sidebar")
                .size(14)
                .color(p.fg_secondary),
        )
        .padding(24)
        .into()
    };

    let track_list = if let Some(idx) = player.selected_playlist {
        if let Some(pl) = player.playlists.playlists.get(idx) {
            view_track_list(&pl.tracks, player)
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
) -> Element<'a, Message> {
    if tracks.is_empty() {
        return Container::new(
            text("No tracks found")
                .size(14)
                .color(player.palette.fg_secondary)
                .center(),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(24)
        .into();
    }

    let items: Vec<Element<'a, Message>> = tracks
        .iter()
        .enumerate()
        .map(|(i, track)| view_track_row(track, i, player))
        .collect();

    Container::new(
        scrollable(Column::with_children(items).spacing(0).width(Length::Fill))
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
) -> Element<'a, Message> {
    let p = &player.palette;
    let is_selected = player.selected_indices.contains(&index);
    let row_bg = if is_selected { p.bg_selected } else { p.bg };
    let is_hovered = player.hovered_track == Some(index);

    let duration_text = crate::util::format_duration(track.duration);

    let leading: Element<'a, Message> = if is_hovered {
        Button::new(icons::icon("play.svg", p.fg, 14.0))
            .padding(2)
            .style(button_style_accent())
            .on_press(Message::PlayTrackAtIndex(index))
            .into()
    } else {
        text((index + 1).to_string())
            .size(12)
            .color(p.fg_secondary)
            .width(Length::Fixed(24.0))
            .center()
            .into()
    };

    let content = Row::with_children(vec![
        Container::new(leading).width(Length::Fixed(24.0)).into(),
        Container::new(thumbnail(track, p))
            .width(Length::Fixed(theme::THUMBNAIL_SIZE))
            .height(Length::Fixed(theme::THUMBNAIL_SIZE))
            .into(),
        text(track.title.clone())
            .size(13)
            .color(p.fg)
            .width(Length::Fill)
            .into(),
        text(track.artist.clone())
            .size(12)
            .color(p.fg_secondary)
            .width(Length::Fill)
            .into(),
        text(duration_text)
            .size(12)
            .color(p.fg_secondary)
            .width(Length::Fixed(48.0))
            .into(),
    ])
    .spacing(10)
    .align_y(alignment::Vertical::Center)
    .padding([6, 10]);

    let track_area = MouseArea::new(content)
        .on_press(Message::TrackPressed(index))
        .on_right_press(Message::TrackRightClicked(index))
        .on_move(move |_| Message::TrackHoverStart(index));

    Container::new(track_area)
        .width(Length::Fill)
        .height(Length::Fixed(theme::ROW_HEIGHT))
        .style(bg(row_bg))
        .into()
}

fn view_queue_panel<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;

    let header = Container::new(
        Row::with_children(vec![
            text("Queue").size(14).color(p.fg_secondary).into(),
            icons::icon("sync.svg", p.fg_muted, 14.0).into(),
        ])
        .spacing(8)
        .align_y(alignment::Vertical::Center)
        .padding([10, 14]),
    )
    .width(Length::Fill);

    if player.queue.tracks.is_empty() {
        let empty =
            Container::new(text("Queue is empty").size(12).color(p.fg_secondary)).padding(16);
        return Container::new(
            Column::with_children(vec![header.into(), empty.into()])
                .spacing(0)
                .width(Length::Fill),
        )
        .width(Length::Fixed(theme::QUEUE_MIN_WIDTH))
        .height(Length::Fill)
        .style(bg(p.bg_secondary))
        .into();
    }

    let items: Vec<Element<'a, Message>> = player
        .queue
        .tracks
        .iter()
        .enumerate()
        .map(|(i, track)| {
            let is_current = player.queue.current_index == i;
            let row_bg = if is_current { p.bg_current } else { p.bg };

            let duration_text = crate::util::format_duration(track.duration);
            let title_color = if is_current { p.accent } else { p.fg };

            let content = Row::with_children(vec![
                Container::new(
                    MouseArea::new(icons::icon("delete.svg", p.fg_muted, 12.0))
                        .on_press(Message::ContextMenuRemoveFromQueue(i)),
                )
                .width(Length::Fixed(24.0))
                .into(),
                Container::new(thumbnail(track, p))
                    .width(Length::Fixed(theme::THUMBNAIL_SIZE))
                    .height(Length::Fixed(theme::THUMBNAIL_SIZE))
                    .into(),
                text(track.title.clone())
                    .size(12)
                    .color(title_color)
                    .width(Length::Fill)
                    .into(),
                text(track.artist.clone())
                    .size(11)
                    .color(p.fg_secondary)
                    .width(Length::Fill)
                    .into(),
                text(duration_text)
                    .size(11)
                    .color(p.fg_secondary)
                    .width(Length::Fixed(48.0))
                    .into(),
            ])
            .spacing(8)
            .align_y(alignment::Vertical::Center)
            .padding([6, 8]);

            Container::new(content)
                .width(Length::Fill)
                .height(Length::Fixed(theme::ROW_HEIGHT))
                .style(bg(row_bg))
                .into()
        })
        .collect();

    let list = scrollable(Column::with_children(items).spacing(0).width(Length::Fill))
        .width(Length::Fill)
        .height(Length::Fill);

    Container::new(
        Column::with_children(vec![header.into(), list.into()])
            .spacing(0)
            .width(Length::Fill),
    )
    .width(Length::Fixed(theme::QUEUE_MIN_WIDTH))
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

    let controls = Row::with_children(vec![
        Button::new(icons::icon("skip-back.svg", p.fg, 16.0))
            .padding(6)
            .style(button_style_accent())
            .on_press(Message::PreviousTrack)
            .into(),
        Button::new(icons::icon(play_pause_icon, p.fg, 18.0))
            .padding(8)
            .style(button_style_accent())
            .on_press(Message::TogglePlayPause)
            .into(),
        Button::new(icons::icon("skip-forward.svg", p.fg, 16.0))
            .padding(6)
            .style(button_style_accent())
            .on_press(Message::NextTrack)
            .into(),
        Button::new(icons::icon("queue.svg", p.fg_muted, 16.0))
            .padding(6)
            .style(button_style_accent())
            .on_press(Message::ToggleQueue)
            .into(),
    ])
    .spacing(8)
    .align_y(alignment::Vertical::Center);

    let track_info = Column::with_children(vec![
        text(title)
            .size(13)
            .color(p.fg)
            .width(Length::Fixed(180.0))
            .into(),
        text(artist).size(11).color(p.fg_secondary).into(),
    ])
    .spacing(2);

    let progress = slider(0.0..=1.0, player.progress, Message::Seek)
        .width(Length::Fill)
        .step(0.01f32)
        .style(slider_style(p.accent, p.bg_secondary));

    let time_and_volume = Row::with_children(vec![
        text(player.elapsed_text.clone())
            .size(11)
            .color(p.fg_secondary)
            .into(),
        Container::new(widget::rule::horizontal(1))
            .width(Length::Fixed(1.0))
            .height(Length::Fixed(12.0))
            .into(),
        text(player.total_text.clone())
            .size(11)
            .color(p.fg_secondary)
            .into(),
        Container::new(Row::new()).width(Length::Fill).into(),
        icons::icon("volume.svg", p.fg_secondary, 14.0).into(),
        slider(0.0..=1.0, player.volume, Message::SetVolume)
            .width(Length::Fixed(80.0))
            .step(0.01f32)
            .style(slider_style(p.accent, p.bg_secondary))
            .into(),
    ])
    .align_y(alignment::Vertical::Center)
    .spacing(10);

    Column::with_children(vec![
        Container::new(
            Row::with_children(vec![controls.into(), track_info.into(), progress.into()])
                .spacing(12)
                .align_y(alignment::Vertical::Center)
                .padding([8, 12]),
        )
        .width(Length::Fill)
        .into(),
        Container::new(time_and_volume)
            .padding([4, 12])
            .width(Length::Fill)
            .into(),
    ])
    .spacing(0)
    .into()
}

fn transparent_bg() -> impl Fn(&iced::Theme) -> container::Style + 'static {
    |_| container::Style {
        background: None,
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

        if menu.in_playlist {
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

        if menu.in_queue {
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
        }

        v
    };

    let menu_content = Container::new(Column::with_children(items).spacing(2).padding(8))
        .width(Length::Fixed(190.0))
        .style(bg(p.bg_secondary));

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
    p: &theme::Palette,
    on_press: Message,
) -> Container<'a, Message> {
    Container::new(
        MouseArea::new(
            Row::with_children(vec![
                icons::icon(icon, p.fg_muted, 12.0).into(),
                text(label).size(13).color(p.fg).into(),
            ])
            .spacing(8)
            .padding([6, 8])
            .align_y(alignment::Vertical::Center),
        )
        .on_press(on_press),
    )
    .width(Length::Fill)
    .style(bg(p.bg_hover))
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
            Container::new(
                MouseArea::new(
                    Row::with_children(vec![text(&pl.name).size(13).color(p.fg).into()])
                        .spacing(8)
                        .padding([8, 12])
                        .align_y(alignment::Vertical::Center),
                )
                .on_press(Message::AddToPlaylist(i)),
            )
            .width(Length::Fill)
            .style(bg(bg_color))
            .into()
        })
        .collect();

    let cancel_btn =
        Button::new(Container::new(text("Cancel").size(12).color(Color::WHITE)).padding(4))
            .padding(8)
            .width(Length::Fixed(80.0))
            .style(button_style_accent())
            .on_press(Message::ClosePicker);

    let dialog = Container::new(
        Column::with_children(vec![
            text("Add to Playlist").size(16).color(p.fg).into(),
            Column::with_children(items)
                .spacing(0)
                .width(Length::Fill)
                .into(),
            cancel_btn.into(),
        ])
        .spacing(8)
        .padding(0),
    )
    .width(Length::Fixed(300.0))
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

    let cancel_btn =
        Button::new(Container::new(text("Cancel").size(12).color(Color::WHITE)).padding(4))
            .padding(8)
            .width(Length::Fixed(80.0))
            .style(button_style_accent())
            .on_press(Message::HideDeleteConfirm);

    let delete_btn =
        Button::new(Container::new(text("Delete").size(12).color(Color::WHITE)).padding(4))
            .padding(8)
            .width(Length::Fixed(80.0))
            .style(button::danger)
            .on_press(Message::ConfirmDeletePlaylist);

    let dialog = Container::new(
        Column::with_children(vec![
            text("Delete playlist?").size(16).color(p.fg).into(),
            text("Tracks will not be deleted.")
                .size(13)
                .color(p.fg_secondary)
                .into(),
            Row::with_children(vec![cancel_btn.into(), delete_btn.into()])
                .spacing(8)
                .align_y(alignment::Vertical::Center)
                .into(),
        ])
        .spacing(12)
        .padding(24),
    )
    .width(Length::Fixed(300.0))
    .height(Length::Fixed(140.0))
    .style(bg(p.bg_secondary));

    Container::new(MouseArea::new(dialog))
        .width(Length::Fill)
        .height(Length::Fill)
        .style(bg(p.overlay))
        .into()
}
