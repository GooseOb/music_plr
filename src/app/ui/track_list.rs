use iced::{
    alignment,
    widget::{scrollable, text, Button, Column, Container, Id, MouseArea, Row, Space},
    Element, Length,
};

pub fn track_list_id(pane: PaneId) -> Id {
    match pane {
        crate::app::pane::QUEUE_PANE_ID => crate::app::ui::QUEUE_LIST_ID,
        crate::app::pane::RECENT_PANE_ID => crate::app::ui::QUEUE_RECENT_LIST_ID,
        crate::app::pane::MANUAL_SEARCH_PANE_ID => crate::app::ui::MANUAL_SEARCH_LIST_ID,
        _ => Id::from(format!("track_list:{pane}")),
    }
}

use super::{
    right_click::RightClickArea,
    shared_components::{
        empty_state, inner_row_layout, play_pause_button, subtitle_artist, thumbnail, track_row,
    },
    styles::{
        button_style_album, button_style_primary, fg_secondary, icon_style_accent,
        icon_style_primary,
    },
    theme, Message, MusicPlayer,
};
use crate::{
    app::{
        interaction::{row_id, HoverTarget, Pressed, TrackPos},
        pane::{is_main_pane, PaneId},
        ui::shared_components::like_button,
        update::operation::{ListGeometry, ScrollTarget},
        ViewData,
    },
    icons,
    theme::AppTheme,
    types::Track,
};

pub(super) fn view_track_list<'a>(
    tracks: &'a [Track],
    player: &'a MusicPlayer,
    pane: PaneId,
    index_offset: usize,
) -> Element<'a, Message, AppTheme> {
    if tracks.is_empty() {
        return empty_state(player.strings.no_tracks_found);
    }

    let view_data = is_main_pane(pane).then(|| player.view_data_in(pane));
    let show_album = view_data.is_some_and(ViewData::show_album);
    let show_plays = view_data.is_some_and(ViewData::show_plays);

    virtual_scrollable(tracks.len(), pane, player, |i| {
        view_track_row_inner(
            &tracks[i],
            TrackPos::new(i + index_offset, pane),
            player,
            show_album,
            show_plays,
        )
    })
}

pub(super) fn virtual_scrollable<'a, F>(
    count: usize,
    pane: PaneId,
    player: &'a MusicPlayer,
    render_row: F,
) -> Element<'a, Message, AppTheme>
where
    F: Fn(usize) -> Element<'a, Message, AppTheme>,
{
    let row_h = theme::ROW_HEIGHT;
    let children: Vec<Element<'a, Message, AppTheme>> = match player.bounds.list_geo(pane) {
        Some(ListGeometry {
            translation_y,
            bounds,
            ..
        }) if count > 0 && count as f32 * row_h > bounds.height + 1.0 => {
            // The scrollable clamps its own offset to the content's height, so
            // a capture taken before the list shrank can point past the last
            // row. Clamp it too, or the window would start after the content.
            let translation_y = translation_y.min((count as f32 * row_h - bounds.height).max(0.0));
            let viewport_height = bounds.height;
            let overscan = 6i32;

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
        // Bounds known and the list fits within the viewport, or no capture has
        // seen this list yet: render every row. The captured geometry only ever
        // narrows the window; a missing capture must never read as an empty
        // list (the update loop refreshes it once the list's shape changes).
        _ => (0..count).map(render_row).collect(),
    };

    let id = track_list_id(pane);
    scrollable(Column::with_children(children))
        .id(id)
        .height(Length::Fill)
        .on_scroll(move |vp| super::scrolled(ScrollTarget::Tracks(pane), vp.absolute_offset()))
        .into()
}

pub(super) fn row_spacer<'a>(height: f32) -> Element<'a, Message, AppTheme> {
    Space::new().height(height).into()
}

pub(super) fn leading_control<'a>(
    pos: TrackPos,
    track: &'a Track,
    player: &'a MusicPlayer,
    trashed: bool,
) -> Element<'a, Message, AppTheme> {
    let is_current = player
        .queue
        .current()
        .is_some_and(|t| t.same_identity(track));
    let is_hovered = player.drag.hovered_track() == Some(pos);

    if is_current {
        play_pause_button(player.is_playing)
            .padding(theme::SPACING_2XS)
            .on_press(Message::TogglePlayPause)
            .into()
    } else if is_hovered && !trashed {
        Button::new(icons::icon(icons::PLAY_ICON, theme::ICON_SIZE_LG).style(icon_style_primary()))
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
    let is_selected = player.is_selected_in(pos.pane, pos.index);
    let is_hovered = player.drag.hovered_track() == Some(pos);
    let is_dragging = player.is_dragging_track(pos);
    let is_current = player
        .queue
        .current()
        .is_some_and(|t| t.same_identity(track));
    let is_match = player.is_track_list_match(pos);
    let trashed = player.is_trashed(track);
    let is_interacting = is_hovered || is_dragging;

    let row_bg = {
        let current = if is_selected {
            Some((1.0, 1.0))
        } else if is_current {
            Some((0.6, 0.8))
        } else {
            None
        };
        if let Some((idle, hot)) = current {
            p.bg_current
                .scale_alpha(if is_interacting { hot } else { idle })
        } else if is_interacting {
            p.bg_hover
        } else {
            p.bg
        }
    };

    let leading = leading_control(pos, track, player, trashed);

    let inner =
        track_row_layout_inner(leading, track, player, pos, show_album, show_plays, trashed);

    let track_area = RightClickArea::new(
        MouseArea::new(inner)
            .interaction(player.drag.clickable_cursor_interaction())
            .on_press(Message::DragPress(Pressed::Track(pos)))
            .on_enter(Message::HoverStart(HoverTarget::Track(pos)))
            .on_exit(Message::HoverEnd(HoverTarget::Track(pos))),
        pos,
        Message::TrackRightClicked,
    );

    let border = if is_match || is_dragging {
        Some(if is_interacting { p.accent } else { p.fg_muted })
    } else {
        None
    };

    track_row(
        track_area,
        row_bg,
        Some(row_id(pos.pane, pos.index)),
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
    pos: TrackPos,
    show_album: bool,
) -> Row<'a, Message, AppTheme> {
    let trashed = player.is_trashed(track);
    track_row_layout_inner(leading, track, player, pos, show_album, false, trashed)
}

#[allow(clippy::too_many_lines)]
fn track_row_layout_inner<'a>(
    leading: Element<'a, Message, AppTheme>,
    track: &'a Track,
    player: &'a MusicPlayer,
    pos: TrackPos,
    show_album: bool,
    show_plays: bool,
    trashed: bool,
) -> Row<'a, Message, AppTheme> {
    let pane = if is_main_pane(pos.pane) {
        pos.pane
    } else {
        player.focused_pane_id
    };
    let thumb = player.thumbnail_index.get(track.source, track.primary_id());
    let is_downloaded = player.download_registry.contains(&track.cache_key());
    let is_cached = player
        .stream_cache
        .index_contains(track.source, track.primary_id());

    let mut trailing_children = Vec::with_capacity(3);

    if show_album {
        if let Some(album) = track.album() {
            let album_button: Element<'a, Message, AppTheme> = Container::new(
                Button::new(text(&album.name).size(theme::TEXT_SIZE_SM))
                    .style(button_style_album())
                    .on_press(Message::Browse(
                        pane,
                        crate::app::ViewKind::Album(crate::app::view_data::AlbumRef {
                            url: track.album_page_url().unwrap_or_default(),
                            id: album.id.clone(),
                            name: album.name.clone(),
                            provider: track.source,
                            ..Default::default()
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
                text((player.strings.n_plays)(plays))
                    .size(theme::TEXT_SIZE_SM)
                    .width(Length::Fill)
                    .style(fg_secondary())
                    .into(),
            );
        }
    }

    if trashed {
        trailing_children.push(
            icons::icon(icons::DELETE_ICON, theme::ICON_SIZE_MD)
                .style(icon_style_accent())
                .into(),
        );
    }

    trailing_children.push(if is_downloaded {
        icons::icon(icons::DOWNLOAD_ICON, theme::ICON_SIZE_MD)
            .style(icon_style_accent())
            .into()
    } else if is_cached {
        icons::icon(icons::CACHE_ICON, theme::ICON_SIZE_MD)
            .style(icon_style_accent())
            .into()
    } else {
        Space::new().width(theme::ICON_SIZE_MD).into()
    });

    let liked = player.is_track_liked(track);
    trailing_children.push(
        like_button(liked)
            .on_press(Message::ToggleTrackLike(pos))
            .into(),
    );

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
        .padding(iced::Padding {
            top: 0.0,
            right: theme::SPACING_2XL,
            bottom: 0.0,
            left: theme::SPACING_SM,
        })
        .into(),
    );

    let artist_id = track.provider_artist_id(track.source);
    let artist_subtitle = subtitle_artist(
        pane,
        &track.artist,
        theme::TEXT_SIZE_SM,
        artist_id.map(|id| (id.to_string(), track.source)),
    );

    inner_row_layout(
        leading,
        thumbnail(theme::THUMBNAIL_SIZE, thumb),
        &track.title,
        artist_subtitle,
        Row::with_children(trailing_children)
            .spacing(theme::SPACING_SM)
            .align_y(alignment::Vertical::Center)
            .into(),
    )
}
