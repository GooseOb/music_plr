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
        empty_state, inner_row_layout, play_pause_button, subtitle_artists, thumbnail, track_row,
    },
    styles::{button_style_album, fg_secondary, icon_style_accent},
    theme, Message, MusicPlayer,
};
use crate::{
    app::{
        interaction::{row_id, HoverTarget, Pressed, TrackPos},
        pane::{is_main_pane, PaneId},
        ui::shared_components::{leading_number, playbar_toggle_button_unsized},
        update::operation::ScrollTarget,
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
    let overscan = 6usize;
    let geo = player.bounds.list_geo(pane);
    // A list's first frame has no capture yet.
    let viewport_height = geo.map_or(player.window_size.height, |g| g.bounds.height);
    let translation_y = geo.map_or(0.0, |g| g.translation_y);
    let (first, end) = visible_range(count, row_h, translation_y, viewport_height, overscan);

    let children = (first > 0)
        .then(|| row_spacer(first as f32 * row_h))
        .into_iter()
        .chain((first..end).map(render_row))
        .chain((end < count).then(|| row_spacer((count - end) as f32 * row_h)));

    let id = track_list_id(pane);
    scrollable(Column::with_children(children))
        .id(id)
        .height(Length::Fill)
        .on_scroll(move |vp| super::scrolled(ScrollTarget::Tracks(pane), vp.absolute_offset()))
        .into()
}

/// Half-open range of rows a virtualized list draws for a viewport. The
/// scrollable clamps its own offset to the content's height, so a capture taken
/// before the list shrank can point past the last row; clamp it here too, or
/// the window would start after the content.
fn visible_range(
    count: usize,
    row_h: f32,
    translation_y: f32,
    viewport_height: f32,
    overscan: usize,
) -> (usize, usize) {
    let translation_y = translation_y.min((count as f32 * row_h - viewport_height).max(0.0));
    let first = ((((translation_y / row_h) - overscan as f32).max(0.0)) as usize).min(count);
    let visible = ((viewport_height / row_h).ceil() as usize) + overscan * 2;
    (first, (first + visible).min(count))
}

pub(super) fn row_spacer<'a>(height: f32) -> Element<'a, Message, AppTheme> {
    Space::new().height(height).into()
}

pub(super) fn leading_control(
    pos: TrackPos,
    player: &MusicPlayer,
    trashed: bool,
    is_hovered: bool,
    is_current: bool,
) -> Element<'_, Message, AppTheme> {
    if is_current {
        play_pause_button(player.is_playing)
            .padding(theme::SPACING_2XS)
            .on_press(Message::TogglePlayPause)
            .into()
    } else if is_hovered && !trashed {
        play_pause_button(false)
            .padding(theme::SPACING_2XS)
            .on_press(Message::PlayTrackAt(pos))
            .into()
    } else {
        leading_number(pos.index)
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
        .session
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

    let leading = leading_control(pos, player, trashed, is_hovered, is_current);

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

    let border =
        (is_match || is_dragging).then_some(if is_interacting { p.accent } else { p.fg_muted });

    track_row(
        track_area,
        row_bg,
        Some(row_id(pos.pane, pos.index)),
        border,
    )
    .into()
}

// ── shared helpers ─────────────────────────────────────────────
fn trailing_icon(icon: &'static [u8]) -> Element<'static, Message, AppTheme> {
    icons::icon(icon, theme::ICON_SIZE_MD)
        .style(icon_style_accent())
        .into()
}

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
    let dir = player.strings.direction;
    let pane = if is_main_pane(pos.pane) {
        pos.pane
    } else {
        player.session.focused
    };
    let thumb = player.thumbnail_index.get(track.source, track.primary_id());
    let is_downloaded = player.download_registry.contains(&track.cache_key());
    let is_cached = player
        .stream_cache
        .index_contains(track.source, track.primary_id());

    let album_button = show_album.then(|| track.album()).flatten().map(
        |album| -> Element<'a, Message, AppTheme> {
            Container::new(
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
            .into()
        },
    );

    let plays = show_plays
        .then(|| track.play_count())
        .filter(|&plays| plays > 0)
        .map(|plays| {
            text((player.strings.n_plays)(plays))
                .size(theme::TEXT_SIZE_SM)
                .width(Length::Fill)
                .style(fg_secondary())
                .into()
        });

    let trashed_icon = trashed.then(|| trailing_icon(icons::DELETE_ICON));

    let icon = is_downloaded
        .then_some(icons::DOWNLOAD_ICON)
        .or_else(|| is_cached.then_some(icons::CACHE_ICON))
        .map(trailing_icon);

    let liked = player.is_track_liked(track);

    // Unknown durations (0) render blank rather than "--:--".
    let duration = track.duration();

    let trailing_children = album_button
        .into_iter()
        .chain(plays)
        .chain(trashed_icon)
        .chain(icon)
        .chain([
            playbar_toggle_button_unsized(icons::HEART_ICON, liked)
                .padding(theme::SPACING_XS)
                .on_press(Message::ToggleTrackLike(pos))
                .into(),
            Container::new(
                text(if duration > 0 {
                    crate::util::format_duration(duration).into_owned()
                } else {
                    String::new()
                })
                .size(theme::TEXT_SIZE_SM)
                .style(fg_secondary()),
            )
            .padding(theme::SPACING_SM)
            .into(),
        ]);

    let artist_subtitle = subtitle_artists(pane, track, player.strings.direction);

    inner_row_layout(
        player.strings.direction,
        leading,
        thumbnail(theme::THUMBNAIL_SIZE, thumb),
        &track.title,
        artist_subtitle,
        dir.row(trailing_children)
            .spacing(theme::SPACING_SM)
            .align_y(alignment::Vertical::Center)
            .into(),
    )
    .padding(iced::Padding {
        top: theme::SPACING_XS,
        right: theme::SPACING_XL,
        bottom: theme::SPACING_XS,
        left: theme::SPACING_SM,
    })
}

#[cfg(test)]
mod tests {
    use super::visible_range;

    const ROW: f32 = 48.0;

    #[test]
    fn an_uncaptured_list_renders_only_a_screenful() {
        let (first, end) = visible_range(10_000, ROW, 0.0, 768.0, 6);
        assert_eq!(first, 0);
        assert_eq!(end, (768.0f32 / ROW).ceil() as usize + 12);
        assert!(end < 10_000);
    }

    #[test]
    fn a_captured_window_follows_the_scroll_offset() {
        let (first, end) = visible_range(1_000, ROW, 100.0 * ROW, 480.0, 2);
        assert_eq!(first, 98);
        assert_eq!(end, 98 + 10 + 4);
    }

    #[test]
    fn a_short_list_renders_entirely() {
        assert_eq!(visible_range(5, ROW, 0.0, 480.0, 6), (0, 5));
    }

    #[test]
    fn a_stale_offset_clamps_to_the_last_screen() {
        let (first, end) = visible_range(100, ROW, f32::MAX, 480.0, 2);
        assert_eq!(first, 88);
        assert_eq!(end, 100);
    }
}
