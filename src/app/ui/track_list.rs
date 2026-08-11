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
    app::interaction::{TrackListKind, TrackPos},
    icons,
    theme::{AppTheme, Palette},
    types::Track,
};

use super::{
    shared_components::play_pause_button,
    styles::{button_style_album, button_style_primary, fg_secondary},
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
    list: TrackListKind,
    index_offset: usize,
) -> Element<'a, Message, AppTheme> {
    if tracks.is_empty() {
        return empty_state("No tracks found");
    }

    let show_album = list == TrackListKind::Active
        && !matches!(player.view_data().kind, crate::app::ViewKind::Album { .. });

    let target_matches = player.drag.drag_target_list == Some(list);

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
        items.push(view_track_row(
            track,
            TrackPos::new(adjusted, list),
            player,
            show_album,
        ));
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
            .id(list.scrollable_id())
            .width(Length::Fill)
            .height(Length::Fill),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

pub(super) fn leading_control<'a>(
    pos: TrackPos,
    track: &'a Track,
    player: &'a MusicPlayer,
) -> Element<'a, Message, AppTheme> {
    let is_current = player.queue.current().is_some_and(|t| t.url == track.url);
    let is_hovered = player.drag.hovered_track == Some(pos);

    if is_current {
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
        .on_press(Message::PlayTrackAt(pos))
        .into()
    } else {
        text((pos.index + 1).to_string())
            .size(theme::TEXT_SIZE_SM)
            .style(fg_secondary())
            .width(theme::TRACK_LEADING_WIDTH)
            .center()
            .into()
    }
}

pub(super) fn view_track_row<'a>(
    track: &'a Track,
    pos: TrackPos,
    player: &'a MusicPlayer,
    show_album: bool,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let is_selected = player.selection(pos.list).contains(&pos.index);
    let is_hovered = player.drag.hovered_track == Some(pos);
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
            p.bg_current
        } else if is_hovered {
            p.bg_hover
        } else {
            p.bg
        }
    };

    let leading = leading_control(pos, track, player);

    let inner = track_row_layout(leading, track, player, show_album);

    let track_area = MouseArea::new(inner)
        .on_press(Message::TrackPressed(pos))
        .on_right_press(Message::TrackRightClicked(pos))
        .on_move(move |_| Message::TrackHoverStart(pos));

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
        Some(sub) => Column::with_children([
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
    show_album: bool,
) -> Row<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let thumb = player.thumbnail_index.get(&track.id);
    let is_downloaded = player.download_registry.contains(&track.url);
    let is_cached = player.stream_cache.index_contains(&track.id);

    let mut trailing_children = Vec::with_capacity(2);

    if show_album {
        if let Some(album) = &track.album {
            let album_button: Element<'a, Message, AppTheme> = Container::new(
                Button::new(text(album.name.clone()).size(theme::TEXT_SIZE_SM))
                    .style(button_style_album())
                    .on_press(Message::OpenAlbum(album.id.clone(), album.name.clone())),
            )
            .width(Length::FillPortion(2))
            .into();
            trailing_children.push(album_button);
        }
    }

    if is_downloaded {
        trailing_children
            .push(icons::icon(icons::DOWNLOAD_ICON, p.accent, theme::ICON_SIZE_MD).into());
    } else if is_cached {
        trailing_children
            .push(icons::icon(icons::CACHE_ICON, p.accent, theme::ICON_SIZE_MD).into());
    }

    trailing_children.push(
        Container::new(
            text(crate::util::format_duration(track.duration))
                .size(theme::TEXT_SIZE_SM)
                .style(fg_secondary()),
        )
        .padding([0.0, theme::SPACING_2XL])
        .into(),
    );

    inner_row_layout(
        leading,
        Some(thumbnail(p, theme::THUMBNAIL_SIZE, thumb)),
        &track.title,
        Some(&track.artist),
        Some(
            Row::with_children(trailing_children)
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
pub(super) fn empty_state(msg: &str) -> Element<'_, Message, AppTheme> {
    Container::new(text(msg).style(fg_secondary()).center())
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
