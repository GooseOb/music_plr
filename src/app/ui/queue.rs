use iced::{
    alignment,
    widget::{rule, text, Button, Column, Container, Id, Row},
    Element, Length,
};

use crate::{
    app::interaction::{TrackListKind, TrackPos},
    icons,
    theme::{self, AppTheme},
    types::QueueTab,
};

use super::{
    styles::{bg_secondary, button_style_panel_item, fg_secondary},
    track_list::{
        empty_state, section_header, track_row_layout, view_track_list, view_track_row,
        virtual_scrollable,
    },
    Message, MusicPlayer,
};

pub const QUEUE_LIST_ID: Id = Id::new("queue_list");
pub const QUEUE_RECENT_LIST_ID: Id = Id::new("queue_recent_list");

pub(super) fn view_queue_panel(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let queue_width =
        (player.window_size.width * theme::QUEUE_WIDTH_RATIO).max(theme::QUEUE_MIN_WIDTH);

    let tab_bar = Container::new(view_queue_tabs(player))
        .width(Length::Fill)
        .style(bg_secondary());

    let body: Element<'_, Message, AppTheme> = match player.queue.queue_tab {
        QueueTab::Queue => view_queue_tab(player),
        QueueTab::RecentlyPlayed => view_recently_played_tab(player),
    };

    let mut children: Vec<Element<'_, Message, AppTheme>> = vec![tab_bar.into(), body];
    if matches!(
        player.floating_search.as_ref().map(|fs| fs.list),
        Some(crate::app::TrackListKind::Queue)
    ) {
        children.insert(1, super::floating_search::view_floating_search(player));
    }

    Container::new(Column::with_children(children))
        .width(queue_width)
        .style(bg_secondary())
        .into()
}

fn view_queue_tabs(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    Row::with_children([
        queue_tab(player, "Queue", QueueTab::Queue),
        queue_tab(player, "Recently played", QueueTab::RecentlyPlayed),
    ])
    .into()
}

fn queue_tab<'a>(
    player: &'a MusicPlayer,
    label: &'a str,
    queue_tab: QueueTab,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let active = player.queue.queue_tab == queue_tab;

    let icon_color = if active { p.accent } else { p.fg_muted };
    let text_color = if active { p.fg } else { p.fg_secondary };

    Button::new(
        Row::with_children([
            icons::icon(icons::MUSIC_ICON, icon_color, theme::ICON_SIZE_SM).into(),
            text(label).color(text_color).into(),
        ])
        .spacing(theme::SPACING_SM)
        .padding([theme::SPACING_SM, theme::SPACING_MD])
        .align_y(alignment::Vertical::Center)
        .width(Length::Fill),
    )
    .padding(0)
    .style(button_style_panel_item(active, text_color))
    .on_press(Message::SwitchQueueTab(queue_tab))
    .into()
}

fn view_queue_tab(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let p = &player.app_theme.palette;

    let now_playing_header = section_header("NOW PLAYING", p);

    let now_playing_row: Element<'_, Message, AppTheme> =
        if let Some(track) = player.queue.current() {
            Container::new(track_row_layout(Row::new().into(), track, player, false))
                .height(theme::ROW_HEIGHT)
                .into()
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

    Column::with_children([
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

fn view_recently_played_tab(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let tracks = &player.queue.recently_played;

    if tracks.is_empty() {
        return empty_state("No recently played tracks");
    }

    virtual_scrollable(tracks.len(), TrackListKind::Recent, player, |i| {
        view_track_row(
            &tracks[i],
            TrackPos::new(i, TrackListKind::Recent),
            player,
            false,
        )
    })
}
