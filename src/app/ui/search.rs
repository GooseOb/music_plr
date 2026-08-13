use iced::{
    alignment,
    widget::{scrollable, text, text_input, Button, Column, Container, Row},
    Color, Element, Length,
};

use crate::{
    app::{interaction::TrackListKind, ui::styles::button_style_result_card, ViewKind},
    data::library::LibraryKind,
    icons,
    theme::AppTheme,
    youtube::{SearchScope, SearchTab},
};

use super::{
    shared_components::toggle_bookmark_button,
    styles::{
        bg_secondary, button_style_delete, button_style_list_item, button_style_primary,
        button_style_scope, fg_secondary,
    },
    theme, track_list, view_track_list, Message, MusicPlayer,
};

pub(super) fn view_search_bar(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let input = Container::new(
        text_input("Search YouTube Music...", &player.search_query)
            .on_input(Message::SearchInputChanged)
            .on_submit(Message::SearchExecute)
            .padding([theme::SPACING_SM, theme::SPACING_MD]),
    )
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

    // Scope selector: a segmented row of tabs under the search input.
    let scope_tabs = SearchScope::all().iter().map(|&scope| {
        let selected = player.search_scope == scope;
        Button::new(text(scope.label()).size(theme::TEXT_SIZE_SM))
            .padding([theme::SPACING_XS, theme::SPACING_SM])
            .style(button_style_scope(selected))
            .on_press(Message::SearchScopeChanged(scope))
            .into()
    });

    let scope_row = Row::with_children(scope_tabs)
        .spacing(theme::SPACING_XS)
        .wrap();

    let controls = Row::with_children([input, search_btn])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center);

    Container::new(
        Column::with_children([controls.into(), scope_row.into()])
            .spacing(theme::SPACING_SM)
            .padding([theme::SPACING_LG, theme::SPACING_XL]),
    )
    .width(Length::Fill)
    .style(bg_secondary())
    .into()
}

pub(super) fn view_search(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    // Dispatched from `content.rs` on `ViewKind::Search`; pull the active tab
    // out of the view kind. Track tabs (Songs/Videos) render the playable
    // list; card tabs (Artists/Albums/Playlists) render their own list.
    let ViewKind::Search { tab, .. } = &player.view_data().kind else {
        return Column::new().into();
    };

    if player.view_data().loading {
        track_list::empty_state("Searching...")
    } else if tab.is_track_tab() {
        view_search_track_tab(player)
    } else {
        view_search_card_tab(player, tab)
    }
}

/// The Songs/Videos tab: a scrollable, paged track list with "Load More".
fn view_search_track_tab(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let results = player.view_data().tracks.as_slice();
    let exhausted = player.view_data().exhausted();

    let mut children: Vec<Element<'_, Message, AppTheme>> = Vec::new();

    if results.is_empty() {
        children.push(track_list::empty_state("No tracks found"));
    } else {
        children.push(view_track_list(results, player, TrackListKind::Active, 0));

        if !exhausted {
            let btn = Button::new(text("Load More").color(Color::WHITE))
                .padding(theme::SPACING_SM)
                .width(Length::Fill)
                .on_press(Message::SearchLoadMore);

            children.push(Container::new(btn).padding(theme::SPACING_SM).into());
        }
    }

    Column::with_children(children)
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// An Artists/Albums/Playlists tab: the concrete card list, filling the page.
fn view_search_card_tab<'a>(
    player: &'a MusicPlayer,
    tab: &'a SearchTab,
) -> Element<'a, Message, AppTheme> {
    // Each card tab shares the same row shape; only the drill-down message
    // differs, so pull out the slice and its click constructor once.
    type CardClick = fn(String, String) -> Message;
    let (items, open, kind): (&[crate::youtube::CardData], CardClick, LibraryKind) = match tab {
        SearchTab::Artists(items) => (items, Message::OpenArtist, LibraryKind::Artist),
        SearchTab::Albums(items) => (items, Message::OpenAlbum, LibraryKind::Album),
        SearchTab::Playlists(items) => (items, Message::OpenPlaylist, LibraryKind::Playlist),
        _ => (&[], |_, _| Message::Noop, LibraryKind::Artist), // unreachable, but needed for type inference
    };

    if items.is_empty() {
        return track_list::empty_state(if player.view_data().loading {
            "Searching..."
        } else {
            "No results found"
        });
    }

    let cards = items.iter().enumerate().map(|(i, c)| {
        let item = crate::data::library::LibraryItem {
            kind,
            id: c.id.clone(),
            title: c.title.clone(),
            thumbnail: c.thumbnail.clone(),
        };
        card_row(
            player,
            i,
            &c.id,
            &c.title,
            &c.subtitle,
            open(c.id.clone(), c.title.clone()),
            item,
        )
    });

    scrollable(Column::with_children(cards)).into()
}

/// A single drill-down card row rendered in the same style as a track row:
/// a leading index number, the item's thumbnail (or a placeholder), and the
/// title/subtitle. Clickable to drill down into the artist/album/playlist.
/// A trailing bookmark button toggles library membership.
fn card_row<'a>(
    player: &'a MusicPlayer,
    index: usize,
    id: &'a str,
    title: &'a str,
    subtitle: &'a str,
    on_press: Message,
    item: crate::data::library::LibraryItem,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let thumb = player.thumbnail_index.get(id);
    let leading = text((index + 1).to_string())
        .size(theme::TEXT_SIZE_SM)
        .style(fg_secondary())
        .width(theme::TRACK_LEADING_WIDTH)
        .center();
    let thumb = track_list::thumbnail(p, theme::THUMBNAIL_SIZE, thumb);
    let saved = player.library.contains(item.kind, &item.id);
    let toggle =
        Container::new(toggle_bookmark_button(p, saved).on_press(Message::ToggleLibrarySave(item)))
            .padding([0.0, theme::SPACING_MD]);
    let inner = track_list::inner_row_layout(
        leading.into(),
        Some(thumb),
        title,
        if subtitle.is_empty() {
            None
        } else {
            Some(subtitle)
        },
        Some(toggle.into()),
    );
    let drill = Button::new(inner)
        .style(button_style_result_card())
        .padding(0)
        .on_press(on_press);
    track_list::track_row(Row::with_children([drill.into()]), p.bg).into()
}

pub(super) fn view_browse(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let tracks = player.view_data().tracks.as_slice();
    let loading = player.view_data().loading;

    let label: &str = match &player.view_data().kind {
        ViewKind::Artist { name, .. } => name,
        ViewKind::Album { title, .. } | ViewKind::PlaylistView { title, .. } => title,
        _ => unreachable!("view_browse should only be called for Artist, Album, or Playlist views"),
    };
    let header = Row::with_children([
        text(label)
            .size(theme::TEXT_SIZE_LG)
            .width(Length::Fill)
            .center()
            .into(),
        view_library_button(player),
    ])
    .align_y(alignment::Vertical::Center)
    .spacing(theme::SPACING_SM)
    .padding([theme::SPACING_SM, theme::SPACING_XL]);

    let track_list = if loading && tracks.is_empty() {
        track_list::empty_state("Loading...")
    } else {
        view_track_list(tracks, player, TrackListKind::Active, 0)
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

pub(super) fn view_search_radio(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let label = player.view_data().label();
    let tracks = player.view_data().tracks.as_slice();
    let loading = player.view_data().loading;

    let header = Container::new(text(label).width(Length::Fill).center())
        .padding([theme::SPACING_SM, theme::SPACING_XL]);

    let track_list = if loading && tracks.is_empty() {
        track_list::empty_state("Generating radio...")
    } else {
        view_track_list(tracks, player, TrackListKind::Active, 0)
    };

    Column::with_children([header.into(), track_list]).into()
}

pub(super) fn view_search_history(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let p = &player.app_theme.palette;

    if player.last_filtered_history.is_empty() {
        return Container::new(text("No recent searches").style(fg_secondary()))
            .width(Length::Fill)
            .height(theme::SEARCH_HISTORY_ITEM_HEIGHT)
            .padding([theme::SPACING_SM, theme::SPACING_XL])
            .style(bg_secondary())
            .into();
    }

    let items = player
        .last_filtered_history
        .iter()
        .enumerate()
        .map(|(i, q)| {
            Container::new(
                Row::with_children([
                    Button::new(
                        Row::with_children([
                            icons::icon(icons::SEARCH_ICON, p.fg_muted, theme::ICON_SIZE_SM).into(),
                            text(q).size(theme::TEXT_SIZE_SM).into(),
                        ])
                        .spacing(theme::SPACING_SM)
                        .padding([theme::SPACING_XS, theme::SPACING_MD])
                        .align_y(alignment::Vertical::Center),
                    )
                    .width(Length::Fill)
                    .padding(0)
                    .style(button_style_list_item(false))
                    .on_press(Message::SearchHistorySelected(i))
                    .into(),
                    Button::new(icons::icon(
                        icons::DELETE_ICON,
                        p.fg_muted,
                        theme::ICON_SIZE_SM,
                    ))
                    .padding(theme::SPACING_XS)
                    .style(button_style_delete())
                    .on_press(Message::DeleteSearchHistory(i))
                    .width(theme::DELETE_BTN_SIZE)
                    .height(theme::DELETE_BTN_SIZE)
                    .into(),
                ])
                .align_y(alignment::Vertical::Center),
            )
            .style(bg_secondary())
            .into()
        });

    let dropdown_height = (player.last_filtered_history.len() as f32
        * theme::SEARCH_HISTORY_ITEM_HEIGHT)
        .min(theme::SEARCH_DROPDOWN_MAX_HEIGHT);

    let content = scrollable(Column::with_children(items))
        .id(iced::widget::Id::new("search_history_list"))
        .height(dropdown_height);

    Container::new(content)
        .height(dropdown_height)
        .style(bg_secondary())
        .into()
}
