use iced::{
    alignment,
    widget::{scrollable, text, Button, Column, Container, MouseArea, Row},
    Color, Element, Length,
};

use super::{
    button_style_green, container, drop_indicator, icons, scrollable_id, theme, thumbnail,
    DragTargetList, Message, MusicPlayer,
};

pub(super) fn view_track_list<'a>(
    tracks: &'a [crate::types::Track],
    player: &'a MusicPlayer,
    is_queue: bool,
    index_offset: usize,
) -> Element<'a, Message> {
    if tracks.is_empty() {
        return empty_state("No tracks found", player.palette.fg_secondary);
    }

    let target_matches = matches!(
        player.drag.drag_target_list,
        Some(DragTargetList::Queue) if is_queue,
    ) || matches!(
        player.drag.drag_target_list,
        Some(DragTargetList::TrackList) if !is_queue,
    );

    let mut items: Vec<Element<'a, Message>> = Vec::with_capacity(tracks.len());
    for (i, track) in tracks.iter().enumerate() {
        let adjusted = i + index_offset;
        if player.drag.drag_active && target_matches {
            if let Some(drop_idx) = player.drag.drag_drop_target {
                if adjusted == drop_idx {
                    items.push(drop_indicator(player.palette.accent).into());
                }
            }
        }
        items.push(view_track_row(track, adjusted, player, is_queue));
    }

    if player.drag.drag_active && target_matches {
        if let Some(drop_idx) = player.drag.drag_drop_target {
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
    let is_hovered = player.drag.hovered_track == Some((index, is_queue));
    let is_focused = !is_queue && player.focused_list_index == index;
    let row_bg = if is_selected {
        p.bg_selected
    } else if is_hovered {
        p.bg_hover
    } else {
        p.bg
    };

    let leading = if is_hovered {
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

    let duration_text = crate::util::format_duration(track.duration);

    let inner = row_layout(
        leading,
        thumbnail(track, p, theme::THUMBNAIL_SIZE),
        title_artist_column(track, p),
        p,
        duration_text,
    );

    let track_area = MouseArea::new(inner)
        .on_press(Message::TrackPressed { index, is_queue })
        .on_right_press(Message::TrackRightClicked { index, is_queue })
        .on_move(move |_| Message::TrackHoverStart { index, is_queue });

    let focus_border = if is_focused && !is_selected {
        Some(p.accent)
    } else {
        None
    };
    track_row(track_area, row_bg, focus_border).into()
}

// ── shared helpers ─────────────────────────────────────────────

/// A title + artist column, used by all track rows.
pub(super) fn title_artist_column<'a>(
    track: &'a crate::types::Track,
    p: &'a theme::Palette,
) -> Column<'a, Message> {
    Column::with_children(vec![
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
    .spacing(2)
}

/// The inner row layout: leading | thumbnail | `title_artist` | duration.
pub(super) fn row_layout<'a>(
    leading: Element<'a, Message>,
    thumb: Element<'a, Message>,
    title_artist: Column<'a, Message>,
    p: &'a theme::Palette,
    duration_text: String,
) -> Row<'a, Message> {
    Row::with_children(vec![
        Container::new(leading)
            .width(Length::Fixed(theme::TRACK_LEADING_WIDTH))
            .into(),
        Container::new(thumb)
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
    .padding([theme::SPACING_XS, theme::SPACING_MD])
}

/// Wraps row content in a fixed-height container with a background color and
/// optional accent border (used for keyboard focus highlight).
pub(super) fn track_row<'a>(
    content: impl Into<Element<'a, Message>>,
    bg: Color,
    focus_border: Option<Color>,
) -> Container<'a, Message> {
    let accent = focus_border;
    Container::new(content)
        .width(Length::Fill)
        .height(Length::Fixed(theme::ROW_HEIGHT))
        .style(move |_: &iced::Theme| {
            let mut s = container::Style {
                background: Some(bg.into()),
                ..Default::default()
            };
            if let Some(color) = accent {
                s.border = iced::border::rounded(0).color(color).width(2.0);
            }
            s
        })
}

/// An empty-state message centered in the available space.
pub(super) fn empty_state(msg: &str, color: Color) -> Element<'_, Message> {
    Container::new(text(msg).size(theme::TEXT_SIZE_MD).color(color).center())
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(theme::SPACING_XL)
        .into()
}

/// A scrollable column of pre-built elements with a stable id.
pub(super) fn scrollable_list<'a>(
    id: &'static str,
    items: Vec<Element<'a, Message>>,
) -> Element<'a, Message> {
    Container::new(
        scrollable(Column::with_children(items).spacing(0).width(Length::Fill))
            .id(iced::widget::Id::new(id))
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// A centered section header (e.g. "NOW PLAYING", "UP NEXT").
pub(super) fn section_header<'a>(label: &'a str, p: &'a theme::Palette) -> Container<'a, Message> {
    Container::new(
        text(label)
            .size(theme::TEXT_SIZE_XS)
            .color(p.accent)
            .width(Length::Fill)
            .center(),
    )
    .width(Length::Fill)
    .padding([theme::SPACING_SM, theme::SPACING_MD])
}
