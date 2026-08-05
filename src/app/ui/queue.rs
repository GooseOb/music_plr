use iced::{
    alignment,
    widget::{self, button, text, Button, Column, Container, MouseArea, Row},
    Color, Element, Length,
};

use crate::{
    icons,
    theme::{self, AppTheme, Palette},
    types::QueueTab,
};

use super::{
    styles::{bg_secondary, button_style_primary},
    Message, MusicPlayer,
};

use super::track_list::{
    empty_state, row_layout, scrollable_list, section_header, thumbnail, title_artist_column,
    track_row,
};

pub(super) fn view_queue_panel(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let queue_width = (player.window_width * theme::QUEUE_WIDTH_RATIO).max(theme::QUEUE_MIN_WIDTH);

    let tab_bar = Container::new(view_queue_tabs(player))
        .width(Length::Fill)
        .style(bg_secondary());

    let body: Element<'_, Message, AppTheme> = match player.queue.queue_tab {
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
    .style(bg_secondary())
    .into()
}

fn view_queue_tabs(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let queue_item = queue_tab(
        player,
        "Queue",
        player.queue.queue_tab == QueueTab::Queue,
        Message::SwitchQueueTab(QueueTab::Queue),
    );
    let recent_item = queue_tab(
        player,
        "Recently played",
        player.queue.queue_tab == QueueTab::RecentlyPlayed,
        Message::SwitchQueueTab(QueueTab::RecentlyPlayed),
    );

    Row::with_children(vec![queue_item, recent_item])
        .spacing(0)
        .width(Length::Fill)
        .into()
}

fn queue_tab<'a>(
    player: &'a MusicPlayer,
    label: &'a str,
    active: bool,
    on: Message,
) -> Element<'a, Message, AppTheme> {
    let p = &player.palette;

    let icon_color = if active { p.accent } else { p.fg_muted };
    let text_color = if active { p.fg } else { p.fg_secondary };
    let bg_color = if active { p.bg_current } else { p.bg_secondary };

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
    .into()
}

/// Renders the Queue tab: a "Now Playing" section header followed by the
/// current track (non-interactive), a separator, then a draggable "Up Next"
/// list.
fn view_queue_tab(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let p = &player.palette;

    let now_playing_header = section_header("NOW PLAYING", p);

    let now_playing_row: Element<'_, Message, AppTheme> =
        if let Some(track) = player.queue.current() {
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

    let up_next_header = section_header("UP NEXT", p);

    let offset = player.queue.current_index + 1;
    let upcoming = if offset <= player.queue.tracks.len() {
        &player.queue.tracks[offset..]
    } else {
        &[]
    };

    let up_next: Element<'_, Message, AppTheme> = if upcoming.is_empty() {
        Container::new(
            text("No more tracks in queue")
                .size(theme::TEXT_SIZE_SM)
                .color(p.fg_secondary),
        )
        .padding(theme::SPACING_MD)
        .into()
    } else {
        super::track_list::view_track_list(upcoming, player, true, offset)
    };

    Column::with_children(vec![
        now_playing_header.into(),
        now_playing_row,
        widget::rule::horizontal(1)
            .style(move |_| widget::rule::Style {
                color: p.fg_muted,
                radius: iced::border::Radius::new(0),
                fill_mode: widget::rule::FillMode::Padded(theme::SPACING_MD as u16),
                snap: true,
            })
            .into(),
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
    p: &'a Palette,
) -> Element<'a, Message, AppTheme> {
    let duration_text = crate::util::format_duration(track.duration);

    let inner = row_layout(
        Container::new(Row::new())
            .width(Length::Fixed(theme::TRACK_LEADING_WIDTH))
            .into(),
        thumbnail(track, p, theme::THUMBNAIL_SIZE),
        title_artist_column(track, p),
        p,
        duration_text,
    );

    Container::new(inner)
        .width(Length::Fill)
        .height(Length::Fixed(theme::ROW_HEIGHT))
        .into()
}

/// Renders the Recently Played tab: a simple scrollable list with no
/// drag / selection / context-menu support.
fn view_recently_played_tab(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let tracks = &player.queue.recently_played;

    if tracks.is_empty() {
        return empty_state("No recently played tracks", player.palette.fg_secondary);
    }

    let items: Vec<Element<'_, Message, AppTheme>> = tracks
        .iter()
        .enumerate()
        .map(|(i, track)| view_recently_played_row(track, i, player))
        .collect();

    scrollable_list("recently_played_list", items)
}

fn view_recently_played_row<'a>(
    track: &'a crate::types::Track,
    index: usize,
    player: &'a MusicPlayer,
) -> Element<'a, Message, AppTheme> {
    let p = &player.palette;
    let is_hovered = player.drag.hovered_track == Some((index, true));
    let row_bg = if is_hovered { p.bg_hover } else { p.bg };

    let leading = if is_hovered {
        Button::new(icons::icon("play.svg", Color::BLACK, theme::ICON_SIZE_LG))
            .padding(theme::SPACING_XS2)
            .style(button_style_primary())
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

    let duration_text = crate::util::format_duration(track.duration);

    let inner = row_layout(
        leading,
        thumbnail(track, p, theme::THUMBNAIL_SIZE),
        title_artist_column(track, p),
        p,
        duration_text,
    );

    let track_area = MouseArea::new(inner)
        .on_press(Message::PlayRecentTrack(index))
        .on_move(move |_| Message::TrackHoverStart {
            index,
            is_queue: true,
        });

    track_row(track_area, row_bg, None).into()
}
