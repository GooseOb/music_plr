use iced::{
    alignment,
    widget::{
        container, image, rule, scrollable, text, Button, Column, Container, MouseArea, Row, Rule,
    },
    Color, Element, Length,
};

use crate::{
    app::ui::shared_components::play_pause_button,
    icons,
    theme::{AppTheme, Palette},
    types::Track,
};

use super::{styles::button_style_primary, theme, DragTargetList, Message, MusicPlayer};

pub fn thumbnail<'a>(
    track: &'a Track,
    p: &'a Palette,
    size: f32,
) -> Element<'a, Message, AppTheme> {
    let thumb_path = crate::thumbnails::thumbnail_path(&track.id);
    if thumb_path.exists() {
        image(image::Handle::from_path(thumb_path))
            .width(size)
            .height(size)
            .border_radius(size / 4.0)
            .content_fit(iced::ContentFit::Cover)
            .into()
    } else {
        icons::icon(icons::MUSIC_ICON, p.fg_muted, size).into()
    }
}

fn drop_indicator() -> Rule<'static, AppTheme> {
    rule::horizontal(theme::DROP_LINE_HEIGHT).style(|theme: &AppTheme| rule::Style {
        color: theme.palette.accent,
        radius: iced::border::Radius::new(0),
        fill_mode: rule::FillMode::Full,
        snap: true,
    })
}

pub(super) fn view_track_list<'a>(
    tracks: &'a [crate::types::Track],
    player: &'a MusicPlayer,
    is_queue: bool,
    index_offset: usize,
) -> Element<'a, Message, AppTheme> {
    if tracks.is_empty() {
        return empty_state("No tracks found", player.app_theme.palette.fg_secondary);
    }

    let target_matches = matches!(
        player.drag.drag_target_list,
        Some(DragTargetList::Queue) if is_queue,
    ) || matches!(
        player.drag.drag_target_list,
        Some(DragTargetList::TrackList) if !is_queue,
    );

    let mut items: Vec<Element<'a, Message, AppTheme>> = Vec::with_capacity(tracks.len());
    for (i, track) in tracks.iter().enumerate() {
        let adjusted = i + index_offset;
        if player.drag.drag_active && target_matches {
            if let Some(drop_idx) = player.drag.drag_drop_target {
                if adjusted == drop_idx {
                    items.push(drop_indicator().into());
                }
            }
        }
        items.push(view_track_row(track, adjusted, player, is_queue));
    }

    if player.drag.drag_active && target_matches {
        if let Some(drop_idx) = player.drag.drag_drop_target {
            if drop_idx == tracks.len() + index_offset {
                items.push(drop_indicator().into());
            }
        }
    }

    Container::new(
        scrollable(Column::with_children(items).spacing(0).width(Length::Fill))
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
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let is_selected = player.selection(is_queue).contains(&index);
    let is_hovered = player.drag.hovered_track == Some((index, is_queue));
    let is_focused = !is_queue && player.focused_list_index == index;
    let is_current = player.queue.current().is_some_and(|t| t.url == track.url);

    let row_bg = if is_current {
        if is_selected {
            p.bg_current
        } else if is_hovered {
            p.bg_current.scale_alpha(0.8)
        } else {
            p.bg_current.scale_alpha(0.6)
        }
    } else {
        if is_selected {
            p.bg_selected
        } else if is_hovered {
            p.bg_hover
        } else {
            p.bg
        }
    };

    let leading = if is_current {
        play_pause_button(player.is_playing)
            .padding(theme::SPACING_XS2)
            .on_press(Message::TogglePlayPause)
            .into()
    } else if is_hovered {
        Button::new(icons::icon(
            icons::PLAY_ICON,
            Color::BLACK,
            theme::ICON_SIZE_LG,
        ))
        .padding(theme::SPACING_XS2)
        .style(button_style_primary())
        .on_press(Message::PlayTrackAtIndex { index, is_queue })
        .into()
    } else {
        text((index + 1).to_string())
            .size(theme::TEXT_SIZE_SM)
            .color(p.fg_secondary)
            .width(theme::TRACK_LEADING_WIDTH)
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
    p: &'a Palette,
) -> Column<'a, Message, AppTheme> {
    Column::with_children(vec![
        text(track.title.clone()).width(Length::Fill).into(),
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
    leading: Element<'a, Message, AppTheme>,
    thumb: Element<'a, Message, AppTheme>,
    title_artist: Column<'a, Message, AppTheme>,
    p: &'a Palette,
    duration_text: String,
) -> Row<'a, Message, AppTheme> {
    Row::with_children(vec![
        Container::new(leading)
            .width(theme::TRACK_LEADING_WIDTH)
            .into(),
        Container::new(thumb)
            .width(theme::THUMBNAIL_SIZE)
            .height(theme::THUMBNAIL_SIZE)
            .into(),
        Container::new(title_artist).width(Length::Fill).into(),
        text(duration_text)
            .size(theme::TEXT_SIZE_SM)
            .color(p.fg_secondary)
            .width(theme::DURATION_WIDTH)
            .into(),
    ])
    .spacing(theme::SPACING_SM)
    .align_y(alignment::Vertical::Center)
    .padding([theme::SPACING_XS, theme::SPACING_MD])
}

/// Wraps row content in a fixed-height container with a background color and
/// optional accent border (used for keyboard focus highlight).
pub(super) fn track_row<'a>(
    content: impl Into<Element<'a, Message, AppTheme>>,
    bg: Color,
    focus_border: Option<Color>,
) -> Container<'a, Message, AppTheme> {
    let accent = focus_border;
    Container::new(content)
        .width(Length::Fill)
        .height(theme::ROW_HEIGHT)
        .style(move |_: &AppTheme| {
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
pub(super) fn empty_state(msg: &str, color: Color) -> Element<'_, Message, AppTheme> {
    Container::new(text(msg).color(color).center())
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(theme::SPACING_XL)
        .into()
}

/// A scrollable column of pre-built elements with a stable id.
pub(super) fn scrollable_list<'a>(
    id: &'static str,
    items: Vec<Element<'a, Message, AppTheme>>,
) -> Element<'a, Message, AppTheme> {
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
pub(super) fn section_header<'a>(
    label: &'a str,
    p: &'a Palette,
) -> Container<'a, Message, AppTheme> {
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
