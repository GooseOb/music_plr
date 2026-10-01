use iced::{
    alignment,
    widget::{
        button, container, opaque, scrollable, text, text_input, Button, Column, Container, Id,
        MouseArea, Row,
    },
    Element, Length, Rectangle,
};

use super::{
    shared_components::{
        empty_state, inner_row_layout, loading_state, thumbnail, toggle_bookmark_button, track_row,
    },
    styles::{
        bg_search_hist, bg_secondary, button_style_hist, button_style_primary, fg_secondary,
        icon_color, icon_fg_secondary, icon_primary, scroll_padding,
    },
    theme, view_track_list, Message, MusicPlayer,
};
use crate::{
    app::{
        interaction::{HoverTarget, Pressed, PressedDrag, TrackListKind},
        pane::PaneId,
        ui::{overlays::pos_absolute, shared_components::scope_tab_row_h_scroll},
        view_data::SearchData,
    },
    data::library::LibraryKind,
    icons,
    load_state::LoadState,
    providers::{CardData, ProviderId, SearchTab},
    theme::AppTheme,
    types::Track,
};

pub fn search_input_id(pane: PaneId) -> Id {
    Id::from(format!("search_input:{pane}"))
}

pub fn search_history_list_id(pane: PaneId) -> Id {
    Id::from(format!("search_history_list:{pane}"))
}

fn scope_label(scope: crate::providers::SearchScope, player: &MusicPlayer) -> &str {
    let tr = player.strings;
    match scope {
        crate::providers::SearchScope::Songs => tr.scope_songs,
        crate::providers::SearchScope::Videos => tr.scope_videos,
        crate::providers::SearchScope::Artists => tr.scope_artists,
        crate::providers::SearchScope::Albums => tr.scope_albums,
        crate::providers::SearchScope::Playlists => tr.scope_playlists,
    }
}

pub(super) fn view_search_bar(
    player: &MusicPlayer,
    pane: PaneId,
) -> Element<'_, Message, AppTheme> {
    let pane_state = player.pane(pane);
    let input = text_input(
        (player.strings.search_placeholder)(pane_state.search_provider).as_str(),
        &pane_state.search_query,
    )
    .on_input(move |q| Message::SearchInputChanged(pane, q))
    .on_submit(Message::SearchExecute(pane))
    .padding([theme::SPACING_SM, theme::SPACING_MD])
    .id(search_input_id(pane))
    .width(Length::Fill)
    .into();

    let search_btn =
        Button::new(icons::icon(icons::SEARCH_ICON, theme::ICON_SIZE_MD).style(icon_primary()))
            .padding(theme::SPACING_SM)
            .style(button_style_primary())
            .width(theme::SEARCH_BTN_SIZE)
            .height(theme::SEARCH_BTN_SIZE)
            .on_press_maybe(if pane_state.search_provider.capabilities().search {
                Some(Message::SearchExecute(pane))
            } else {
                None
            })
            .into();

    let controls = Row::with_children([input, search_btn])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center);

    let provider_row = scope_tab_row_h_scroll(
        ProviderId::searchable()
            .iter()
            .filter(|p| p.capabilities().search)
            .map(|&provider| {
                (
                    provider.label(),
                    pane_state.search_provider == provider,
                    Some(Message::SearchProviderChanged(pane, provider)),
                )
            }),
    );

    let scope_row =
        scope_tab_row_h_scroll(pane_state.search_provider.supported_scopes().iter().map(
            |&scope| {
                (
                    scope_label(scope, player).to_string(),
                    pane_state.search_scope == scope,
                    Some(Message::SearchScopeChanged(pane, scope)),
                )
            },
        ));

    let rows = Column::with_children([
        controls.into(),
        Row::with_children([
            Container::new(scope_row).align_left(Length::Fill).into(),
            Container::new(provider_row)
                .align_right(Length::Fill)
                .into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .into(),
    ])
    .spacing(theme::SPACING_SM);

    Container::new(rows)
        .padding([theme::SPACING_SM, theme::SPACING_XL])
        .style(bg_secondary())
        .into()
}

pub(super) fn view_search<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    search: &'a SearchData,
) -> Element<'a, Message, AppTheme> {
    let tab = &search.tab;
    let content = &player.view_data_in(pane).content;
    match content {
        LoadState::Failed(e) => empty_state((player.strings.search_failed)(e)),
        LoadState::Loading => loading_state(player.strings.searching),
        LoadState::Ready(results) if tab.is_track_tab() => {
            view_search_track_tab(player, pane, search, results)
        }
        LoadState::Ready(_) => view_search_card_tab(player, pane, search, tab),
    }
}

/// The Songs/Videos tab: a scrollable, paged track list with "Load More".
fn view_search_track_tab<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    search: &SearchData,
    results: &'a [Track],
) -> Element<'a, Message, AppTheme> {
    let mut children: Vec<Element<'_, Message, AppTheme>> = Vec::new();

    if results.is_empty() {
        children.push(empty_state(player.strings.no_tracks_found));
    } else {
        children.push(view_track_list(
            results,
            player,
            pane,
            TrackListKind::Active,
            0,
        ));

        if !search.exhausted {
            let btn = Button::new(text(if search.append_in_flight {
                player.strings.loading
            } else {
                player.strings.load_more
            }))
            .padding(theme::SPACING_SM)
            .width(Length::Fill)
            .on_press_maybe((!search.append_in_flight).then_some(Message::SearchLoadMore(pane)));

            children.push(Container::new(btn).padding(theme::SPACING_SM).into());
        }
    }

    Column::with_children(children).into()
}

/// An Artists/Albums/Playlists tab: the concrete card list, filling the page.
fn view_search_card_tab<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    search: &SearchData,
    tab: &'a SearchTab,
) -> Element<'a, Message, AppTheme> {
    let (items, kind): (&[CardData], LibraryKind) = match tab {
        SearchTab::Artists(items) => (items, LibraryKind::Artist),
        SearchTab::Albums(items) => (items, LibraryKind::Album),
        SearchTab::Playlists(items) => (items, LibraryKind::Playlist),
        _ => unreachable!(
            "view_search_card_tab should only be called for Artists, Albums, or Playlists tabs"
        ),
    };

    if items.is_empty() {
        return empty_state(player.strings.no_results_found);
    }

    let cards = items.iter().enumerate().map(|(i, c)| {
        let item = crate::data::library::LibraryItem {
            kind,
            id: c.id.clone(),
            title: c.title.clone(),
            thumbnail: c.thumbnail.clone(),
            provider: search.provider,
        };
        card_row(player, pane, i, &c.id, &c.title, &c.subtitle, &item)
    });

    scrollable(Column::with_children(cards)).into()
}

/// A single drill-down card row. The main area is a `MouseArea` so the
/// card can be dragged (onto the playlist list to become a local playlist, or
/// onto the library to save/reorder); a plain click drills down into it. The
/// trailing bookmark button toggles library membership.
fn card_row<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    index: usize,
    id: &'a str,
    title: &'a str,
    subtitle: &'a str,
    item: &crate::data::library::LibraryItem,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let thumb = player.thumbnail_index.get(item.provider, id);
    let leading = text((index + 1).to_string())
        .size(theme::TEXT_SIZE_SM)
        .style(fg_secondary())
        .width(theme::TRACK_LEADING_WIDTH)
        .center();
    let thumb = thumbnail(theme::THUMBNAIL_SIZE, thumb);
    let saved = player.library.contains(item.kind, &item.id);
    let toggle = toggle_bookmark_button(saved)
        .on_press(Message::ToggleLibrarySave(item.clone()))
        .into();
    let subtitle_el = text(subtitle)
        .size(theme::TEXT_SIZE_SM)
        .style(fg_secondary())
        .into();
    let main = inner_row_layout(leading.into(), thumb, title, subtitle_el, toggle);
    let is_hovered = player.drag.is_hovered_card(item);
    let is_dragging_this = matches!(
        player.drag.pressed,
        Some(PressedDrag { what: Pressed::Card(ref c, _), .. }) if c == item
    );
    let main = MouseArea::new(main)
        .interaction(player.drag.clickable_cursor_interaction())
        .on_press(Message::DragPress(Pressed::Card(item.clone(), Some(pane))))
        .on_enter(Message::HoverStart(HoverTarget::Card(item.clone())))
        .on_exit(Message::HoverEnd(HoverTarget::Card(item.clone())));
    track_row(
        main,
        // TODO: make consistent
        if is_dragging_this || is_hovered {
            p.bg_hover
        } else {
            p.bg
        },
        None,
        is_dragging_this.then_some(p.accent),
    )
    .into()
}

/// "Badge · date" line for album views; empty when both are empty.
/// Borrows when only one side is present, allocating just for the joined
/// pair.
pub(super) fn browse_meta<'a>(badge: &'a str, date: &'a str) -> std::borrow::Cow<'a, str> {
    match (badge.trim(), date.trim()) {
        ("", "") => "".into(),
        (badge, "") => badge.into(),
        ("", date) => date.into(),
        (badge, date) => format!("{badge} \u{00b7} {date}").into(),
    }
}

pub(super) fn view_browse<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    provider: ProviderId,
    label: &'a str,
    thumb_key: &'a str,
    meta: std::borrow::Cow<'a, str>,
) -> Element<'a, Message, AppTheme> {
    let item = player
        .current_library_item(pane)
        .expect("view_browse should only be called for album/playlist views");

    let saved = player.library.contains(item.kind, &item.id);
    view_track_page(
        player,
        pane,
        TrackPage {
            thumbnail: player.thumbnail_index.get(provider, thumb_key),
            title: label,
            meta,
            loading: player.strings.loading,
        },
        [toggle_bookmark_button(saved)
            .on_press(Message::ToggleLibrarySave(item))
            .into()],
    )
}

pub(super) fn view_radio<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    data: &'a crate::app::RadioData,
    meta: &'static str,
) -> Element<'a, Message, AppTheme> {
    view_track_page(
        player,
        pane,
        TrackPage {
            thumbnail: player
                .thumbnail_index
                .get(data.thumb_provider, &data.thumb_id),
            title: &data.title,
            meta: meta.into(),
            loading: player.strings.generating_radio,
        },
        std::iter::empty(),
    )
}

struct TrackPage<'a> {
    thumbnail: Option<&'a std::path::PathBuf>,
    title: &'a str,
    meta: std::borrow::Cow<'a, str>,
    loading: &'a str,
}

/// Shared artwork/title/meta header plus track list for album/playlist
/// browse pages and radio pages. Callers prepend their own buttons via
/// `leading_buttons` (the save-to-library bookmark for browse, none for
/// radio); the save-as-playlist button is always appended while tracks
/// are loaded.
fn view_track_page<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    spec: TrackPage<'a>,
    leading_buttons: impl IntoIterator<Item = Element<'a, Message, AppTheme>>,
) -> Element<'a, Message, AppTheme> {
    let content = &player.view_data_in(pane).content;

    let header = Row::with_children([
        thumbnail(theme::PAGE_THUMBNAIL_SIZE, spec.thumbnail),
        Column::with_children([
            text(spec.title).size(theme::TEXT_SIZE_LG).into(),
            text(spec.meta)
                .size(theme::TEXT_SIZE_SM)
                .style(fg_secondary())
                .into(),
            Row::with_children(
                leading_buttons.into_iter().chain([Button::new(text(
                    player.strings.save_as_playlist,
                ))
                .padding([theme::SPACING_XS, theme::SPACING_SM])
                .on_press_maybe(
                    matches!(content, LoadState::Ready(tracks) if !tracks.is_empty())
                        .then_some(Message::SaveBrowseAsPlaylist(pane)),
                )
                .into()]),
            )
            .spacing(theme::SPACING_SM)
            .into(),
        ])
        .spacing(theme::SPACING_MD)
        .into(),
    ])
    .align_y(alignment::Vertical::Center)
    .spacing(theme::SPACING_MD)
    .padding([theme::SPACING_SM, theme::SPACING_XL]);

    let track_list =
        match super::shared_components::load_state_tracks(content, player.strings, spec.loading) {
            Ok(tracks) => view_track_list(tracks, player, pane, TrackListKind::Active, 0),
            Err(el) => el,
        };

    Column::with_children([header.into(), track_list]).into()
}

pub(super) fn view_search_history(
    player: &MusicPlayer,
    pane: PaneId,
    input_rect: Rectangle,
) -> Element<'_, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let pane_state = player.pane(pane);

    let content: Element<'_, Message, AppTheme> = if pane_state.last_filtered_history.is_empty() {
        Container::new(
            text(player.strings.no_recent_searches)
                .style(fg_secondary())
                .width(Length::Fill),
        )
        .padding([theme::SPACING_XS, theme::SPACING_MD])
        .into()
    } else {
        let items = pane_state
            .last_filtered_history
            .iter()
            .enumerate()
            .map(|(i, q)| {
                let is_hovered = player.drag.hovered_search_history() == Some(i);
                let text_color = if is_hovered { p.fg } else { p.fg_muted };
                let row = Container::new(
                    Row::with_children([
                        Button::new(
                            Row::with_children([
                                icons::icon(icons::SEARCH_ICON, theme::ICON_SIZE_SM)
                                    .style(icon_color(text_color))
                                    .into(),
                                text(q).size(theme::TEXT_SIZE_SM).into(),
                            ])
                            .spacing(theme::SPACING_SM)
                            .padding([theme::SPACING_XS, theme::SPACING_MD])
                            .align_y(alignment::Vertical::Center),
                        )
                        .width(Length::Fill)
                        .padding(0)
                        .style(move |_, _| button::Style {
                            background: None,
                            text_color,
                            ..Default::default()
                        })
                        .on_press(Message::SearchHistorySelected(pane, i))
                        .into(),
                        Button::new(
                            icons::icon(icons::DELETE_ICON, theme::ICON_SIZE_SM)
                                .style(icon_fg_secondary()),
                        )
                        .padding(theme::SPACING_XS)
                        .style(button_style_hist())
                        .on_press(Message::DeleteSearchHistory(pane, i))
                        .into(),
                    ])
                    .align_y(alignment::Vertical::Center),
                )
                .style(move |theme: &AppTheme| container::Style {
                    background: if is_hovered {
                        Some(theme.palette.bg_secondary.into())
                    } else {
                        None
                    },
                    border: iced::border::rounded(theme::RADIUS_SM),
                    ..Default::default()
                })
                .id(iced::widget::Id::from(format!("search_history:{i}")));
                MouseArea::new(row)
                    .on_enter(Message::HoverStart(HoverTarget::SearchHistory(i)))
                    .on_exit(Message::HoverEnd(HoverTarget::SearchHistory(i)))
                    .into()
            });

        let dropdown_height = player
            .bounds
            .search_history
            .get(&pane)
            .map_or(0.0, |g| g.bounds.height);

        scrollable(Column::with_children(items).padding(scroll_padding()))
            .id(search_history_list_id(pane))
            .height(dropdown_height)
            .into()
    };

    let dropdown = Container::new(content)
        .padding([theme::SPACING_SM, theme::SPACING_XS])
        .style(bg_search_hist())
        .width(input_rect.width);

    pos_absolute(
        opaque(dropdown),
        input_rect.x,
        input_rect.y + input_rect.height,
    )
    .into()
}
