use iced::{
    alignment,
    widget::{
        container, image, rule, scrollable, text, Button, Column, Container, Id, MouseArea, Row,
        Rule,
    },
    Color, Element, Length,
};

pub const TRACK_LIST_ID: Id = Id::new("track_list");

use crate::{
    icons,
    theme::{AppTheme, Palette},
    types::Track,
};

use super::{
    queue::QUEUE_LIST_ID,
    shared_components::play_pause_button,
    styles::{button_style_primary, fg_secondary},
    theme, Message, MusicPlayer,
};

/// Render a thumbnail image if it exists on disk, otherwise a music-note
/// placeholder. `thumb` is the resolved path from the thumbnail index
/// (`Some`) or `None` when not yet downloaded.
pub fn thumbnail<'a>(
    p: &'a Palette,
    size: f32,
    thumb: Option<&'a std::path::PathBuf>,
) -> Element<'a, Message, AppTheme> {
    if let Some(path) = thumb {
        image(image::Handle::from_path(path))
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
    tracks: &'a [Track],
    player: &'a MusicPlayer,
    is_queue: bool,
    index_offset: usize,
) -> Element<'a, Message, AppTheme> {
    if tracks.is_empty() {
        return empty_state("No tracks found", player.app_theme.palette.fg_secondary);
    }

    let target_matches = MusicPlayer::same_list_kind_as(player.drag.drag_target_list, is_queue);

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
            .id(if is_queue {
                QUEUE_LIST_ID
            } else {
                TRACK_LIST_ID
            })
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub(super) fn view_track_row<'a>(
    track: &'a Track,
    index: usize,
    player: &'a MusicPlayer,
    is_queue: bool,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let is_selected = player.selection(is_queue).contains(&index);
    let is_hovered = player.drag.hovered_track == Some((index, is_queue));
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
            .padding(theme::SPACING_2XS)
            .on_press(Message::TogglePlayPause)
            .into()
    } else if is_hovered {
        Button::new(icons::icon(
            icons::PLAY_ICON,
            Color::BLACK,
            theme::ICON_SIZE_LG,
        ))
        .padding(theme::SPACING_2XS)
        .style(button_style_primary())
        .on_press(Message::PlayTrackAtIndex { index, is_queue })
        .into()
    } else {
        text((index + 1).to_string())
            .size(theme::TEXT_SIZE_SM)
            .style(fg_secondary())
            .width(theme::TRACK_LEADING_WIDTH)
            .center()
            .into()
    };

    let inner = track_row_layout(leading, track, player);

    let track_area = MouseArea::new(inner)
        .on_press(Message::TrackPressed { index, is_queue })
        .on_right_press(Message::TrackRightClicked { index, is_queue })
        .on_move(move |_| Message::TrackHoverStart { index, is_queue });

    track_row(track_area, row_bg).into()
}

// ── shared helpers ─────────────────────────────────────────────

/// The shared inner row layout used by both track rows and the non-track
/// card rows (artists/albums/playlists): leading | optional thumbnail |
/// title(+subtitle) | optional trailing. `subtitle`/`trailing` are `None`
/// when not needed.
pub(super) fn inner_row_layout<'a>(
    leading: Element<'a, Message, AppTheme>,
    thumbnail: Option<Element<'a, Message, AppTheme>>,
    title: &'a str,
    subtitle: Option<&'a str>,
    trailing: Option<Element<'a, Message, AppTheme>>,
) -> Row<'a, Message, AppTheme> {
    let mut children: Vec<Element<'a, Message, AppTheme>> = Vec::with_capacity(5);
    children.push(leading);
    if let Some(thumbnail) = thumbnail {
        children.push(thumbnail);
    }
    let title_el = text(title).size(theme::TEXT_SIZE_MD).width(Length::Fill);
    children.push(match subtitle {
        Some(sub) => Column::with_children(vec![
            title_el.into(),
            text(sub)
                .size(theme::TEXT_SIZE_SM)
                .style(fg_secondary())
                .into(),
        ])
        .spacing(theme::SPACING_2XS)
        .into(),
        None => title_el.into(),
    });
    if let Some(trailing) = trailing {
        children.push(trailing);
    }
    Row::with_children(children)
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .padding([theme::SPACING_XS, theme::SPACING_SM])
}

/// The inner row layout for a track: leading | thumbnail | title/artist |
/// status icon | duration. Delegates to [`inner_row_layout`].
pub(super) fn track_row_layout<'a>(
    leading: Element<'a, Message, AppTheme>,
    track: &'a Track,
    player: &'a MusicPlayer,
) -> Row<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let thumb = player.thumbnail_index.get(&track.id);
    let is_downloaded = player.download_registry.contains(&track.url);
    let is_cached = player.stream_cache.index_contains(&track.id);

    let status_icon: Element<'a, Message, AppTheme> = if is_downloaded {
        icons::icon(icons::DOWNLOAD_ICON, p.accent, theme::ICON_SIZE_MD).into()
    } else if is_cached {
        icons::icon(icons::CACHE_ICON, p.accent, theme::ICON_SIZE_MD).into()
    } else {
        Container::new(text("")).into()
    };

    inner_row_layout(
        leading,
        Some(thumbnail(p, theme::THUMBNAIL_SIZE, thumb)),
        &track.title,
        Some(&track.artist),
        Some(
            Row::with_children(vec![
                status_icon,
                Container::new(
                    text(crate::util::format_duration(track.duration))
                        .size(theme::TEXT_SIZE_SM)
                        .style(fg_secondary()),
                )
                .padding([0.0, theme::SPACING_2XL])
                .into(),
            ])
            .align_y(alignment::Vertical::Center)
            .into(),
        ),
    )
}

/// Wraps row content in a fixed-height container with a background color.
pub(super) fn track_row<'a>(
    content: impl Into<Element<'a, Message, AppTheme>>,
    bg: Color,
) -> Container<'a, Message, AppTheme> {
    Container::new(content)
        .width(Length::Fill)
        .height(theme::ROW_HEIGHT)
        .style(move |_: &AppTheme| container::Style {
            background: Some(bg.into()),
            ..Default::default()
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
pub(super) fn scrollable_list(
    id: Id,
    items: Vec<Element<Message, AppTheme>>,
) -> Element<Message, AppTheme> {
    Container::new(
        scrollable(Column::with_children(items).spacing(0).width(Length::Fill))
            .id(id)
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
