use iced::{
    alignment,
    widget::{rule, text, Button, Column, Container, Id, Row, Space},
    Element, Length,
};

use crate::{
    app::{
        interaction::{TrackListKind, TrackPos},
        ViewKind,
    },
    icons,
    theme::{self, AppTheme},
    types::QueueTab,
};

use super::{
    shared_components::empty_state,
    styles::{bg_secondary, button_style_album, button_style_panel_item, fg_secondary},
    track_list::{
        section_header, track_row_layout, view_track_list, view_track_row, virtual_scrollable,
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

    let track_list_search: Element<'_, Message, AppTheme> = if matches!(
        player.track_list_search.as_ref().map(|fs| fs.list),
        Some(crate::app::TrackListKind::Queue)
    ) {
        super::track_list_search::view_track_list_search(player)
    } else {
        Space::new().into()
    };

    let children: Vec<Element<'_, Message, AppTheme>> =
        vec![tab_bar.into(), track_list_search, body];

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

pub fn now_playing_source_label(kind: &ViewKind) -> Option<&str> {
    match kind {
        ViewKind::Search(s) => (!s.query.is_empty()).then_some(s.query.as_str()),
        ViewKind::SongRadio(label) | ViewKind::ArtistRadio(label) => Some(label),
        ViewKind::Artist(e) => Some(&e.name),
        ViewKind::Album(r) => Some(&r.name),
        ViewKind::PlaylistView(r) => Some(&r.name),
        ViewKind::Playlist(e) => Some(e.name.as_str()),
        ViewKind::Downloads => Some("Downloads"),
        ViewKind::Settings => None,
    }
}

fn view_queue_tab(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let p = &player.app_theme.palette;

    let now_playing_header: Element<'_, Message, AppTheme> = match player
        .now_playing_from
        .as_ref()
        .and_then(|v| now_playing_source_label(&v.kind))
    {
        Some(source) => Row::with_children([
            text("Now playing from")
                .size(theme::TEXT_SIZE_XS)
                .color(p.accent)
                .into(),
            Button::new(text(source).size(theme::TEXT_SIZE_XS))
                .padding(0)
                .style(button_style_album())
                .on_press(Message::RevealNowPlaying)
                .into(),
        ])
        .spacing(theme::SPACING_XS)
        .padding([theme::SPACING_SM, theme::SPACING_MD])
        .align_y(alignment::Vertical::Center)
        .into(),
        None => section_header("Now playing", p).into(),
    };

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

    let up_next_header = section_header("Up next", p);

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
        now_playing_header,
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
