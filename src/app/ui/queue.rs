use iced::{
    alignment,
    widget::{button, rule, text, Button, Column, Container, Id, MouseArea, Row},
    Element, Length,
};

use crate::{
    app::interaction::{TrackListKind, TrackPos},
    icons,
    theme::{self, AppTheme},
    types::QueueTab,
};

use super::{
    styles::{bg_secondary, fg_secondary},
    track_list::{
        empty_state, scrollable_list, section_header, track_row, track_row_layout, view_track_list,
    },
    Message, MusicPlayer,
};

pub const QUEUE_LIST_ID: Id = Id::new("queue_list");
pub const QUEUE_RECENT_LIST_ID: Id = Id::new("queue_recent_list");

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
    .width(queue_width)
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
    let p = &player.app_theme.palette;

    let icon_color = if active { p.accent } else { p.fg_muted };
    let text_color = if active { p.fg } else { p.fg_secondary };
    let bg_color = if active { p.bg_current } else { p.bg_secondary };

    Button::new(
        Row::with_children(vec![
            icons::icon(icons::MUSIC_ICON, icon_color, theme::ICON_SIZE_SM).into(),
            text(label).color(text_color).into(),
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

fn view_queue_tab(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let p = &player.app_theme.palette;

    let now_playing_header = section_header("NOW PLAYING", p);

    let now_playing_row: Element<'_, Message, AppTheme> =
        if let Some(track) = player.queue.current() {
            view_now_playing_row(track, player)
        } else {
            Container::new(
                text("No track playing")
                    .size(theme::TEXT_SIZE_SM)
                    .style(fg_secondary()),
            )
            .padding(theme::SPACING_MD)
            .into()
        };

    let up_next_header = section_header("UP NEXT", p);

    let offset = 1;
    let upcoming = if offset <= player.queue.tracks.len() {
        &player.queue.tracks[offset..]
    } else {
        &[]
    };

    let up_next: Element<'_, Message, AppTheme> = if upcoming.is_empty() {
        Container::new(
            text("No more tracks in queue")
                .size(theme::TEXT_SIZE_SM)
                .style(fg_secondary()),
        )
        .padding(theme::SPACING_MD)
        .into()
    } else {
        view_track_list(upcoming, player, TrackListKind::Queue, offset)
    };

    Column::with_children(vec![
        now_playing_header.into(),
        now_playing_row,
        rule::horizontal(1)
            .style(move |_| rule::Style {
                color: p.fg_muted,
                radius: iced::border::Radius::new(0),
                fill_mode: rule::FillMode::Padded(theme::SPACING_MD as u16),
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
    player: &'a MusicPlayer,
) -> Element<'a, Message, AppTheme> {
    let inner = track_row_layout(Row::new().into(), track, player);

    Container::new(inner)
        .width(Length::Fill)
        .height(theme::ROW_HEIGHT)
        .into()
}

fn view_recently_played_tab(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let tracks = &player.queue.recently_played;

    if tracks.is_empty() {
        return empty_state("No recently played tracks");
    }

    let items: Vec<Element<'_, Message, AppTheme>> = tracks
        .iter()
        .enumerate()
        .map(|(i, track)| view_recently_played_row(track, i, player))
        .collect();

    scrollable_list(QUEUE_RECENT_LIST_ID, items)
}

fn view_recently_played_row<'a>(
    track: &'a crate::types::Track,
    index: usize,
    player: &'a MusicPlayer,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let pos = TrackPos::new(index, TrackListKind::Recent);
    let row_bg = if player.drag.hovered_track == Some(pos) {
        p.bg_hover
    } else {
        p.bg
    };

    let leading = super::track_list::leading_control(pos, track, player);

    let inner = track_row_layout(leading, track, player);

    let track_area = MouseArea::new(inner)
        .on_right_press(Message::TrackRightClicked(pos))
        .on_move(move |_| Message::TrackHoverStart(pos));

    track_row(track_area, row_bg).into()
}
