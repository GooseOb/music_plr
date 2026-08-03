use iced::{
    alignment,
    widget::{button, scrollable, text, Button, Column, Container, MouseArea, Row},
    Color, Length,
};

use super::*;
use crate::types::QueueTab;

pub(super) fn view_track_list<'a>(
    tracks: &'a [crate::types::Track],
    player: &'a MusicPlayer,
    is_queue: bool,
    index_offset: usize,
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
        let adjusted = i + index_offset;
        if player.drag_active && player.pressed_track_is_queue == is_queue {
            if let Some(drop_idx) = player.drag_drop_target {
                if adjusted == drop_idx {
                    items.push(drop_indicator(player.palette.accent).into());
                }
            }
        }
        items.push(view_track_row(track, adjusted, player, is_queue));
    }

    if player.drag_active && player.pressed_track_is_queue == is_queue {
        if let Some(drop_idx) = player.drag_drop_target {
            if drop_idx == tracks.len() + index_offset {
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
    let is_focused = !is_queue && player.focused_list_index == index;
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

    let accent = p.accent;
    Container::new(track_area)
        .width(Length::Fill)
        .height(Length::Fixed(theme::ROW_HEIGHT))
        .style(move |_: &iced::Theme| {
            let mut s = container::Style {
                background: Some(row_bg.into()),
                ..Default::default()
            };
            if is_focused && !is_selected {
                s.border = iced::border::rounded(0).color(accent).width(2.0);
            }
            s
        })
        .into()
}

// ── queue panel ────────────────────────────────────────────────

pub(super) fn view_queue_panel<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;
    let queue_width = (player.window_width * theme::QUEUE_WIDTH_RATIO).max(theme::QUEUE_MIN_WIDTH);

    // ── tab bar ──
    let tab_bar = Container::new(view_queue_tabs(player))
        .width(Length::Fill)
        .style(bg(p.bg_secondary));

    let body: Element<'_, Message> = match player.queue.queue_tab {
        QueueTab::Queue => view_queue_tab(player),
        QueueTab::RecentlyPlayed => view_recently_played_tab(player),
    };

    Container::new(
        Column::with_children(vec![tab_bar.into(), body])
            .spacing(0)
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fixed(queue_width))
    .height(Length::Fill)
    .style(bg(p.bg_secondary))
    .into()
}

fn view_queue_tabs<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;
    let is_queue_active = player.queue.queue_tab == QueueTab::Queue;
    let is_recent_active = player.queue.queue_tab == QueueTab::RecentlyPlayed;

    let tab =
        |label: &'a str, icon_color: Color, text_color: Color, bg_color: Color, on: Message| {
            Button::new(
                Row::with_children(vec![
                    icons::icon("music.svg", icon_color, theme::ICON_SIZE_SM).into(),
                    text(label)
                        .size(theme::TEXT_SIZE_MD)
                        .color(text_color)
                        .into(),
                ])
                .spacing(theme::SPACING_SM)
                .padding([theme::SPACING_SM, theme::SPACING_MD])
                .align_y(alignment::Vertical::Center)
                .width(Length::Fill),
            )
            .width(Length::Fill)
            .padding(0)
            .style(move |_, _| button::Style {
                background: Some(bg_color.into()),
                text_color,
                border: iced::border::rounded(0),
                ..Default::default()
            })
            .on_press(on)
        };

    let queue_item: Element<'a, Message> = tab(
        "Queue",
        if is_queue_active {
            p.accent
        } else {
            p.fg_muted
        },
        if is_queue_active {
            p.fg
        } else {
            p.fg_secondary
        },
        if is_queue_active {
            p.bg_current
        } else {
            p.bg_secondary
        },
        Message::SwitchQueueTab(QueueTab::Queue),
    )
    .into();

    let recent_item: Element<'a, Message> = tab(
        "Recently played",
        if is_recent_active {
            p.accent
        } else {
            p.fg_muted
        },
        if is_recent_active {
            p.fg
        } else {
            p.fg_secondary
        },
        if is_recent_active {
            p.bg_current
        } else {
            p.bg_secondary
        },
        Message::SwitchQueueTab(QueueTab::RecentlyPlayed),
    )
    .into();

    Row::with_children(vec![queue_item, recent_item])
        .spacing(0)
        .width(Length::Fill)
        .into()
}

/// Renders the Queue tab: a "Now Playing" section header followed by the
/// current track (non-interactive), a separator, then a draggable "Up Next"
/// list.
fn view_queue_tab<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;

    let now_playing_header = Container::new(
        text("NOW PLAYING")
            .size(theme::TEXT_SIZE_XS)
            .color(p.accent)
            .width(Length::Fill)
            .center(),
    )
    .width(Length::Fill)
    .padding([theme::SPACING_SM, theme::SPACING_MD]);

    let now_playing_row: Element<'a, Message> = if let Some(track) = player.queue.current() {
        view_now_playing_row(track, p)
    } else {
        Container::new(
            text("No track playing")
                .size(theme::TEXT_SIZE_SM)
                .color(p.fg_secondary),
        )
        .padding(theme::SPACING_MD)
        .into()
    };

    let separator = iced::widget::rule::horizontal(1);

    let up_next_header = Container::new(
        text("UP NEXT")
            .size(theme::TEXT_SIZE_XS)
            .color(p.accent)
            .width(Length::Fill)
            .center(),
    )
    .width(Length::Fill)
    .padding([theme::SPACING_SM, theme::SPACING_MD]);

    let offset = player.queue.current_index + 1;
    let upcoming = if offset <= player.queue.tracks.len() {
        &player.queue.tracks[offset..]
    } else {
        &[]
    };

    let up_next: Element<'a, Message> = if upcoming.is_empty() {
        Container::new(
            text("No more tracks in queue")
                .size(theme::TEXT_SIZE_SM)
                .color(p.fg_secondary),
        )
        .padding(theme::SPACING_MD)
        .into()
    } else {
        view_track_list(upcoming, player, true, offset)
    };

    Column::with_children(vec![
        now_playing_header.into(),
        now_playing_row,
        Container::new(separator).width(Length::Fill).into(),
        up_next_header.into(),
        up_next,
    ])
    .spacing(0)
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn view_now_playing_row<'a>(
    track: &'a crate::types::Track,
    p: &'a theme::Palette,
) -> Element<'a, Message> {
    let title_artist = Column::with_children(vec![
        text(track.title.clone())
            .size(theme::TEXT_SIZE_DEFAULT)
            .color(p.fg)
            .into(),
        text(track.artist.clone())
            .size(theme::TEXT_SIZE_SM)
            .color(p.fg_secondary)
            .into(),
    ])
    .spacing(2);

    Container::new(
        Row::with_children(vec![
            Container::new(thumbnail(track, p, theme::THUMBNAIL_SIZE))
                .width(Length::Fixed(theme::THUMBNAIL_SIZE))
                .height(Length::Fixed(theme::THUMBNAIL_SIZE))
                .into(),
            Container::new(title_artist).width(Length::Fill).into(),
            Container::new(
                text(crate::util::format_duration(track.duration))
                    .size(theme::TEXT_SIZE_SM)
                    .color(p.fg_secondary),
            )
            .width(Length::Fixed(theme::DURATION_WIDTH))
            .into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .padding([theme::SPACING_XS, theme::SPACING_MD]),
    )
    .width(Length::Fill)
    .height(Length::Fixed(theme::ROW_HEIGHT))
    .style(bg(p.bg_hover))
    .into()
}

/// Renders the Recently Played tab: a simple scrollable list with no
/// drag / selection / context-menu support.
fn view_recently_played_tab<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
    let p = &player.palette;
    let tracks = &player.queue.recently_played;

    if tracks.is_empty() {
        return Container::new(
            text("No recently played tracks")
                .size(theme::TEXT_SIZE_MD)
                .color(p.fg_secondary)
                .center(),
        )
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(theme::SPACING_XL)
        .into();
    }

    let mut items: Vec<Element<'a, Message>> = Vec::with_capacity(tracks.len());
    for (i, track) in tracks.iter().enumerate() {
        items.push(view_recently_played_row(track, i, player, p));
    }

    Container::new(
        scrollable(Column::with_children(items).spacing(0).width(Length::Fill))
            .id(iced::widget::Id::new("recently_played_list"))
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn view_recently_played_row<'a>(
    track: &'a crate::types::Track,
    index: usize,
    player: &'a MusicPlayer,
    p: &'a theme::Palette,
) -> Element<'a, Message> {
    let is_hovered = player.hovered_track == Some((index, true));
    let row_bg = if is_hovered { p.bg_hover } else { p.bg };

    let duration_text = crate::util::format_duration(track.duration);

    let leading: Element<'a, Message> = if is_hovered {
        Button::new(icons::icon("play.svg", Color::BLACK, theme::ICON_SIZE_LG))
            .padding(6)
            .style(button_style_green())
            .on_press(Message::PlayRecentTrack(index))
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
        .on_press(Message::PlayRecentTrack(index))
        .on_move(move |_| Message::TrackHoverStart {
            index,
            is_queue: true,
        });

    Container::new(track_area)
        .width(Length::Fill)
        .height(Length::Fixed(theme::ROW_HEIGHT))
        .style(move |_: &iced::Theme| container::Style {
            background: Some(row_bg.into()),
            ..Default::default()
        })
        .into()
}
