use iced::{
    alignment,
    widget::{text, Button, Column, Container, MouseArea, Row},
    Color, Length,
};

use super::*;

pub(super) fn view_track_list<'a>(
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

pub(super) fn view_queue_panel<'a>(player: &'a MusicPlayer) -> Element<'a, Message> {
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
