use iced::{
    alignment,
    widget::{scrollable, text, Button, Column, Container, Id, MouseArea, Row, Space},
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
    shared_components::{
        empty_state, inner_row_layout, play_pause_button, subtitle_artist, thumbnail, track_row,
    },
    styles::{button_style_album, button_style_primary, fg_secondary, row_bg},
    theme, Message, MusicPlayer,
};

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
        && !matches!(player.view_data().kind, crate::app::ViewKind::Album(_));
    let show_plays = list == TrackListKind::Active
        && matches!(
            player.view_data().kind,
            crate::app::ViewKind::Search(_)
                | crate::app::ViewKind::SongRadio(_)
                | crate::app::ViewKind::ArtistRadio(_)
                | crate::app::ViewKind::Artist(_)
                | crate::app::ViewKind::Album(_)
                | crate::app::ViewKind::PlaylistView(_)
        );

    virtual_scrollable(tracks.len(), list, player, |i| {
        view_track_row_inner(
            &tracks[i],
            TrackPos::new(i + index_offset, list),
            player,
            show_album,
            show_plays,
        )
    })
}

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
    Space::new().height(height).into()
}

pub(super) fn leading_control<'a>(
    pos: TrackPos,
    track: &'a Track,
    player: &'a MusicPlayer,
) -> Element<'a, Message, AppTheme> {
    let is_current = player
        .queue
        .current()
        .is_some_and(|t| t.cache_key() == track.cache_key());
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
    view_track_row_inner(track, pos, player, show_album, false)
}

fn view_track_row_inner<'a>(
    track: &'a Track,
    pos: TrackPos,
    player: &'a MusicPlayer,
    show_album: bool,
    show_plays: bool,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let is_selected = player.selection(pos.list).contains(&pos.index);
    let is_hovered = player.drag.hovered_track() == Some(pos);
    let is_current = player
        .queue
        .current()
        .is_some_and(|t| t.cache_key() == track.cache_key());
    let is_match = player.is_track_list_match(pos);

    let row_bg = row_bg(
        p,
        if is_selected {
            Some((1.0, 1.0))
        } else if is_current {
            Some((0.6, 0.8))
        } else {
            None
        },
        is_hovered,
        p.bg,
    );

    let leading = leading_control(pos, track, player);

    let inner = track_row_layout_inner(leading, track, player, show_album, show_plays);

    let track_area = MouseArea::new(inner)
        .interaction(player.drag.clickable_cursor_interaction())
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

/// The inner row layout for a track: leading | thumbnail | title/artist |
/// status icon | duration. Delegates to [`inner_row_layout`].
pub(super) fn track_row_layout<'a>(
    leading: Element<'a, Message, AppTheme>,
    track: &'a Track,
    player: &'a MusicPlayer,
    show_album: bool,
) -> Row<'a, Message, AppTheme> {
    track_row_layout_inner(leading, track, player, show_album, false)
}

fn track_row_layout_inner<'a>(
    leading: Element<'a, Message, AppTheme>,
    track: &'a Track,
    player: &'a MusicPlayer,
    show_album: bool,
    show_plays: bool,
) -> Row<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let thumb = player.thumbnail_index.get(track.primary_id());
    let is_downloaded = player.download_registry.contains(&track.cache_key());
    let is_cached = player
        .stream_cache
        .index_contains(track.source, track.primary_id());

    let mut trailing_children = Vec::with_capacity(2);

    if show_album {
        if let Some(album) = track.album() {
            let album_button: Element<'a, Message, AppTheme> = Container::new(
                Button::new(text(album.name.clone()).size(theme::TEXT_SIZE_SM))
                    .style(button_style_album())
                    .on_press(Message::Browse(
                        crate::app::ViewKind::Album(crate::app::view_data::BrowseRef {
                            id: album.id.clone(),
                            name: album.name.clone(),
                        }),
                        track.source,
                    )),
            )
            .width(Length::FillPortion(2))
            .into();
            trailing_children.push(album_button);
        }
    }

    if show_plays {
        let plays = track.play_count();
        if plays > 0 {
            trailing_children.push(
                text(format!("{} plays", crate::util::format_count(plays)))
                    .size(theme::TEXT_SIZE_SM)
                    .width(Length::Fill)
                    .style(fg_secondary())
                    .into(),
            );
        }
    }

    trailing_children.push(if is_downloaded {
        icons::icon(icons::DOWNLOAD_ICON, p.accent, theme::ICON_SIZE_MD).into()
    } else if is_cached {
        icons::icon(icons::CACHE_ICON, p.accent, theme::ICON_SIZE_MD).into()
    } else {
        Space::new().width(theme::ICON_SIZE_MD).into()
    });

    // Unknown durations (0) render blank rather than "--:--".
    let duration = track.duration();
    trailing_children.push(
        Container::new(
            text(if duration > 0 {
                crate::util::format_duration(duration).into_owned()
            } else {
                String::new()
            })
            .size(theme::TEXT_SIZE_SM)
            .style(fg_secondary()),
        )
        .padding([0.0, theme::SPACING_2XL])
        .into(),
    );

    let artist_id = track.provider_artist_id(track.source);
    let artist_subtitle = subtitle_artist(
        &track.artist,
        theme::TEXT_SIZE_SM,
        artist_id.map(|id| (id.to_string(), track.source)),
    );

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
