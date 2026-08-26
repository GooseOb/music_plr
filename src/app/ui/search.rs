use iced::{
    alignment,
    widget::{
        button, container, opaque, scrollable, text, text_input, Button, Column, Container, Id,
        MouseArea, Row, Space,
    },
    Color, Element, Length, Rectangle,
};

use crate::app::{
    interaction::{HoverTarget, Pressed, TrackListKind},
    ui::overlays::pos_absolute,
    view_data::SearchData,
};
use crate::{
    data::library::LibraryKind,
    icons,
    load_state::LoadState,
    providers::{CardData, ProviderId, SearchTab},
    theme::AppTheme,
    types::Track,
};

use super::{
    shared_components::{
        empty_state, inner_row_layout, loading_state, scope_tab_row, thumbnail,
        toggle_bookmark_button, track_row,
    },
    styles::{
        bg_search_hist, bg_secondary, button_style_hist, button_style_primary, fg_secondary,
        scroll_padding,
    },
    theme, view_track_list, Message, MusicPlayer,
};

pub const SEARCH_INPUT_ID: Id = Id::new("search_input");

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

pub(super) fn view_search_bar(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let input = text_input(
        (player.strings.search_placeholder)(player.search_provider).as_str(),
        &player.search_query,
    )
    .on_input(Message::SearchInputChanged)
    .on_submit(Message::SearchExecute)
    .padding([theme::SPACING_SM, theme::SPACING_MD])
    .id(SEARCH_INPUT_ID)
    .width(Length::Fill)
    .into();

    let search_btn = Button::new(icons::icon(
        icons::SEARCH_ICON,
        Color::BLACK,
        theme::ICON_SIZE_MD,
    ))
    .padding(theme::SPACING_SM)
    .style(button_style_primary())
    .width(theme::SEARCH_BTN_SIZE)
    .height(theme::SEARCH_BTN_SIZE)
    .on_press(Message::SearchExecute)
    .into();

    let controls = Row::with_children([input, search_btn])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center);

    let provider_row = scope_tab_row(ProviderId::searchable().iter().map(|&provider| {
        (
            provider.label().to_string(),
            player.search_provider == provider,
            Message::SearchProviderChanged(provider),
        )
    }));

    let scope_row = scope_tab_row(
        player
            .search_provider
            .supported_scopes()
            .iter()
            .map(|&scope| {
                (
                    scope_label(scope, player).to_string(),
                    player.search_scope == scope,
                    Message::SearchScopeChanged(scope),
                )
            }),
    );

    let rows = Column::with_children([
        controls.into(),
        Row::with_children([
            scope_row,
            Space::new().width(Length::Fill).into(),
            provider_row,
        ])
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
    search: &'a SearchData,
) -> Element<'a, Message, AppTheme> {
    let tab = &search.tab;
    let content = &player.view_data().content;
    match content {
        LoadState::Failed(e) => empty_state((player.strings.search_failed)(e)),
        LoadState::Loading => loading_state(&player.app_theme.palette, player.strings.searching),
        LoadState::Ready(results) if tab.is_track_tab() => {
            view_search_track_tab(player, search, results)
        }
        LoadState::Ready(_) => view_search_card_tab(player, search, tab),
    }
}

/// The Songs/Videos tab: a scrollable, paged track list with "Load More".
fn view_search_track_tab<'a>(
    player: &'a MusicPlayer,
    search: &SearchData,
    results: &'a [Track],
) -> Element<'a, Message, AppTheme> {
    let mut children: Vec<Element<'_, Message, AppTheme>> = Vec::new();

    if results.is_empty() {
        children.push(empty_state(player.strings.no_tracks_found));
    } else {
        children.push(view_track_list(results, player, TrackListKind::Active, 0));

        if !search.exhausted {
            let btn = Button::new(text(if search.append_in_flight {
                player.strings.loading
            } else {
                player.strings.load_more
            }))
            .padding(theme::SPACING_SM)
            .width(Length::Fill)
            .on_press_maybe((!search.append_in_flight).then_some(Message::SearchLoadMore));

            children.push(Container::new(btn).padding(theme::SPACING_SM).into());
        }
    }

    Column::with_children(children).into()
}

/// An Artists/Albums/Playlists tab: the concrete card list, filling the page.
fn view_search_card_tab<'a>(
    player: &'a MusicPlayer,
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
        card_row(player, i, &c.id, &c.title, &c.subtitle, &item)
    });

    scrollable(Column::with_children(cards)).into()
}

/// A single drill-down card row. The main area is a `MouseArea` so the
/// card can be dragged (onto the playlist list to become a local playlist, or
/// onto the library to save/reorder); a plain click drills down into it. The
/// trailing bookmark button toggles library membership.
fn card_row<'a>(
    player: &'a MusicPlayer,
    index: usize,
    id: &'a str,
    title: &'a str,
    subtitle: &'a str,
    item: &crate::data::library::LibraryItem,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let thumb = player.thumbnail_index.get(id);
    let leading = text((index + 1).to_string())
        .size(theme::TEXT_SIZE_SM)
        .style(fg_secondary())
        .width(theme::TRACK_LEADING_WIDTH)
        .center();
    let thumb = thumbnail(p, theme::THUMBNAIL_SIZE, thumb);
    let saved = player.library.contains(item.kind, &item.id);
    let toggle = toggle_bookmark_button(p, saved)
        .on_press(Message::ToggleLibrarySave(item.clone()))
        .into();
    let subtitle_el = text(subtitle)
        .size(theme::TEXT_SIZE_SM)
        .style(fg_secondary())
        .into();
    let main = inner_row_layout(leading.into(), thumb, title, subtitle_el, toggle);
    let is_hovered = player.drag.is_hovered_card(item);
    let is_dragging_this = player.drag.pressed.as_ref() == Some(&Pressed::Card(item.clone()));
    let hover_item = item.clone();
    let main = MouseArea::new(main)
        .interaction(player.drag.clickable_cursor_interaction())
        .on_press(Message::DragPress(Pressed::Card(item.clone())))
        .on_move(move |_| Message::HoverStart(HoverTarget::Card(hover_item.clone())));
    track_row(
        main,
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

/// "Badge · date" line for album views; `None` when both are empty.
pub(super) fn browse_meta(badge: &str, date: &str) -> Option<String> {
    match (badge.trim(), date.trim()) {
        ("", "") => None,
        (badge, "") => Some(badge.to_string()),
        ("", date) => Some(date.to_string()),
        (badge, date) => Some(format!("{badge} \u{00b7} {date}")),
    }
}

pub(super) fn view_browse<'a>(
    player: &'a MusicPlayer,
    label: &'a str,
    thumb_key: &str,
    meta: Option<String>,
) -> Element<'a, Message, AppTheme> {
    let content = &player.view_data().content;
    let p = &player.app_theme.palette;

    let image = thumbnail(
        p,
        theme::PAGE_THUMBNAIL_SIZE,
        player.thumbnail_index.get(thumb_key),
    );
    let header = Row::with_children([
        image,
        Column::with_children([
            text(label).size(theme::TEXT_SIZE_LG).into(),
            text(meta.unwrap_or_default())
                .size(theme::TEXT_SIZE_SM)
                .style(fg_secondary())
                .into(),
            view_library_button(player),
        ])
        .spacing(theme::SPACING_MD)
        .into(),
    ])
    .align_y(alignment::Vertical::Center)
    .spacing(theme::SPACING_MD)
    .padding([theme::SPACING_SM, theme::SPACING_XL]);

    let track_list = match content {
        LoadState::Failed(e) => empty_state((player.strings.couldnt_load)(e)),
        LoadState::Loading => loading_state(&player.app_theme.palette, player.strings.loading),
        LoadState::Ready(tracks) => {
            view_track_list(tracks.as_slice(), player, TrackListKind::Active, 0)
        }
    };

    Column::with_children([header.into(), track_list]).into()
}

fn view_library_button(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let item = player
        .current_library_item()
        .expect("view_library_button should only be called when a library item is present");
    let saved = player.library.contains(item.kind, &item.id);
    toggle_bookmark_button(p, saved)
        .on_press(Message::ToggleLibrarySave(item))
        .into()
}

pub(super) fn view_search_radio<'a>(
    player: &'a MusicPlayer,
    label: &'a str,
) -> Element<'a, Message, AppTheme> {
    let content = &player.view_data().content;

    let header = Container::new(text(label).width(Length::Fill).center())
        .padding([theme::SPACING_SM, theme::SPACING_XL]);

    let track_list = match content {
        LoadState::Failed(e) => empty_state(format!("Radio failed: {e}")),
        LoadState::Loading => {
            loading_state(&player.app_theme.palette, player.strings.generating_radio)
        }
        LoadState::Ready(tracks) => {
            view_track_list(tracks.as_slice(), player, TrackListKind::Active, 0)
        }
    };

    Column::with_children([header.into(), track_list]).into()
}

pub const SEARCH_HISTORY_LIST_ID: Id = Id::new("search_history_list");

pub(super) fn view_search_history(
    player: &MusicPlayer,
    input_rect: Rectangle,
) -> Element<'_, Message, AppTheme> {
    let p = &player.app_theme.palette;

    let content: Element<'_, Message, AppTheme> = if player.last_filtered_history.is_empty() {
        Container::new(
            text(player.strings.no_recent_searches)
                .style(fg_secondary())
                .width(Length::Fill),
        )
        .padding([theme::SPACING_XS, theme::SPACING_MD])
        .into()
    } else {
        let items = player
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
                                icons::icon(icons::SEARCH_ICON, text_color, theme::ICON_SIZE_SM)
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
                        .on_press(Message::SearchHistorySelected(i))
                        .into(),
                        Button::new(icons::icon(
                            icons::DELETE_ICON,
                            p.fg_secondary,
                            theme::ICON_SIZE_SM,
                        ))
                        .padding(theme::SPACING_XS)
                        .style(button_style_hist())
                        .on_press(Message::DeleteSearchHistory(i))
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
                    ..Default::default()
                })
                .id(iced::widget::Id::from(format!("search_history:{i}")));
                MouseArea::new(row)
                    .on_move(move |_| Message::HoverStart(HoverTarget::SearchHistory(i)))
                    .into()
            });

        let dropdown_height = player
            .bounds
            .search_history
            .as_ref()
            .map_or(0.0, |g| g.bounds.height);

        scrollable(Column::with_children(items).padding(scroll_padding()))
            .id(SEARCH_HISTORY_LIST_ID)
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
