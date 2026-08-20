use iced::{
    alignment,
    widget::{
        container, image, scrollable, text, Button, Column, Container, Id, MouseArea, Row, Space,
    },
    Color, Element, Length,
};

pub const TRACK_LIST_ID: Id = Id::new("track_list");

use crate::{
    app::{
        interaction::{row_id, HoverTarget, Pressed, TrackListKind, TrackPos},
        update::operation::ListGeometry,
    },
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

    virtual_scrollable(tracks.len(), list, player, |i| {
        view_track_row(
            &tracks[i],
            TrackPos::new(i + index_offset, list),
            player,
            show_album,
        )
    })
}

/// Render a scrollable of `count` rows, building only the rows currently
/// visible in the viewport (plus a small overscan) and padding the rest with
/// fixed-height spacers so the scrollable's total height — and therefore its
/// scroll range — stays `count * ROW_HEIGHT`.
///
/// Without this, `view()` rebuilds and lays out every row on every message
/// (the 250ms `Tick`, every `CursorMoved`), so a long playlist makes the UI
/// stutter on startup and scroll. The visible window is derived from the scroll
/// offset: the viewport height and content height come from the one-time
/// `CaptureBounds` pass (see `player.bounds`), and the live offset is updated
/// per scroll by [`Message::ListScrolled`]. Until the bounds are captured the
/// list falls back to rendering all rows, which is correct, just not windowed.
pub(super) fn virtual_scrollable<'a, F>(
    count: usize,
    list: TrackListKind,
    player: &'a MusicPlayer,
    render_row: F,
) -> Element<'a, Message, AppTheme>
where
    F: Fn(usize) -> Element<'a, Message, AppTheme>,
{
    let geo = match list {
        TrackListKind::Queue => player.bounds.queue.as_ref(),
        TrackListKind::Active => player.bounds.track.as_ref(),
        TrackListKind::Recent => player.bounds.recent.as_ref(),
    };
    let children: Vec<Element<'a, Message, AppTheme>> = match geo {
        Some(ListGeometry {
            translation_y,
            bounds,
            ..
        }) if count > 0 && count as f32 * crate::theme::ROW_HEIGHT > bounds.height + 1.0 => {
            let viewport_height = bounds.height;
            let overscan = 6i32;
            let row_h = crate::theme::ROW_HEIGHT;

            let first =
                ((((translation_y / row_h) - overscan as f32).max(0.0)) as usize).min(count);
            let visible = ((viewport_height / row_h).ceil() as usize) + (overscan as usize) * 2;
            let end = (first + visible).min(count);

            let mut items = Vec::with_capacity((end - first) + 2);
            if first > 0 {
                items.push(row_spacer(first as f32 * row_h));
            }
            for i in first..end {
                items.push(render_row(i));
            }
            if end < count {
                items.push(row_spacer((count - end) as f32 * row_h));
            }
            items
        }
        _ => (0..count).map(render_row).collect(),
    };

    scrollable(Column::with_children(children))
        .id(list)
        .on_scroll(move |vp| Message::ListScrolled {
            list,
            translation_y: vp.absolute_offset().y,
        })
        .into()
}

pub(super) fn row_spacer<'a>(height: f32) -> Element<'a, Message, AppTheme> {
    Space::new().height(Length::Fixed(height)).into()
}

pub(super) fn leading_control<'a>(
    pos: TrackPos,
    track: &'a Track,
    player: &'a MusicPlayer,
) -> Element<'a, Message, AppTheme> {
    let is_current = player.queue.current().is_some_and(|t| t.url == track.url);
    let is_hovered = player.drag.hovered_track() == Some(pos);

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
    let is_hovered = player.drag.hovered_track() == Some(pos);
    let is_current = player.queue.current().is_some_and(|t| t.url == track.url);
    let is_match = player.is_floating_match(pos);

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
        .on_press(Message::DragPress(Pressed::Track(pos)))
        .on_right_press(Message::TrackRightClicked(pos))
        .on_move(move |_| Message::HoverStart(HoverTarget::Track(pos)));

    let border = if is_match {
        Some(if is_hovered { p.accent } else { p.fg_muted })
    } else {
        None
    };

    track_row(
        track_area,
        row_bg,
        Some(row_id(pos.list, pos.index)),
        border,
    )
    .into()
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
    subtitle: Option<Element<'a, Message, AppTheme>>,
    trailing: Option<Element<'a, Message, AppTheme>>,
) -> Row<'a, Message, AppTheme> {
    let mut children: Vec<Element<'a, Message, AppTheme>> = Vec::with_capacity(5);
    children.push(leading);
    if let Some(thumbnail) = thumbnail {
        children.push(thumbnail);
    }
    let title_el = text(title).size(theme::TEXT_SIZE_MD).width(Length::Fill);
    children.push(match subtitle {
        Some(sub) => Column::with_children([title_el.into(), sub])
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
                    .on_press(Message::Browse(crate::app::ViewKind::Album {
                        id: album.id.clone(),
                        name: album.name.clone(),
                    })),
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

    let artist_name = track.artist.name.clone();
    let artist_subtitle: Element<'a, Message, AppTheme> = match &track.artist.id {
        Some(artist_id) => Button::new(
            text(artist_name.clone())
                .size(theme::TEXT_SIZE_SM)
                .style(fg_secondary()),
        )
        .padding(0)
        .style(button_style_album())
        .on_press(Message::Browse(crate::app::ViewKind::Artist {
            id: artist_id.clone(),
            name: artist_name,
        }))
        .into(),
        None => text(artist_name)
            .size(theme::TEXT_SIZE_SM)
            .style(fg_secondary())
            .into(),
    };

    inner_row_layout(
        leading,
        Some(thumbnail(p, theme::THUMBNAIL_SIZE, thumb)),
        &track.title,
        Some(artist_subtitle),
        Some(
            Row::with_children(trailing_children)
                .align_y(alignment::Vertical::Center)
                .into(),
        ),
    )
}

/// Wraps row content in a fixed-height container with a background color.
/// `id` (when `Some`) tags the container so the bounds `Operation` can capture
/// its measured geometry for drop-target hit-testing. `border` (when `Some`)
/// draws a 1px outline in the given color (used by the floating in-list
/// search to mark matched / current tracks).
pub(super) fn track_row<'a>(
    content: impl Into<Element<'a, Message, AppTheme>>,
    bg: Color,
    id: Option<Id>,
    border: Option<Color>,
) -> Container<'a, Message, AppTheme> {
    let container = Container::new(content)
        .height(theme::ROW_HEIGHT)
        .style(move |_: &AppTheme| container::Style {
            background: Some(bg.into()),
            border: border.map_or(iced::border::Border::default(), |c| iced::border::Border {
                width: 1.0,
                color: c,
                radius: 0.0.into(),
            }),
            ..Default::default()
        });
    match id {
        Some(id) => container.id(id),
        None => container,
    }
}

pub(super) fn empty_state<'a>(msg: impl text::IntoFragment<'a>) -> Element<'a, Message, AppTheme> {
    Container::new(text(msg).style(fg_secondary()).center())
        .padding(theme::SPACING_XL)
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
    .padding([theme::SPACING_SM, theme::SPACING_MD])
}
