use iced::{
    alignment,
    widget::{scrollable, text, text_input, Button, Column, Container, Row},
    Color, Element, Length,
};

use crate::app::{interaction::TrackListKind, ViewKind};
use crate::{icons, theme::AppTheme, youtube::SearchScope};

use super::{
    styles::{
        bg_secondary, button_style_delete, button_style_list_item, button_style_primary,
        button_style_result_card, button_style_scope, fg_secondary,
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
    let scope_tabs: Vec<Element<'_, Message, AppTheme>> = SearchScope::all()
        .iter()
        .map(|&scope| {
            let selected = player.search_scope == scope;
            Button::new(
                text(scope.label())
                    .size(theme::TEXT_SIZE_SM)
                    .color(if selected {
                        Color::WHITE
                    } else {
                        player.app_theme.palette.fg_secondary
                    }),
            )
            .padding([theme::SPACING_XS, theme::SPACING_SM])
            .style(button_style_scope(selected))
            .on_press(Message::SearchScopeChanged(scope))
            .into()
        })
        .collect();

    let scope_row = Row::with_children(scope_tabs)
        .spacing(theme::SPACING_XS)
        .wrap();

    let controls = Row::with_children(vec![input, search_btn])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center);

    Container::new(
        Column::with_children(vec![controls.into(), scope_row.into()])
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
        loading_placeholder("Searching...")
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
    tab: &'a crate::youtube::SearchTab,
) -> Element<'a, Message, AppTheme> {
    let cards: Vec<Element<'_, Message, AppTheme>> = match tab {
        crate::youtube::SearchTab::Artists(items) => items
            .iter()
            .enumerate()
            .map(|(i, c)| {
                card_row(
                    player,
                    i,
                    &c.id,
                    &c.title,
                    &c.subtitle,
                    Message::OpenArtist(c.id.clone(), c.title.clone()),
                )
            })
            .collect(),
        crate::youtube::SearchTab::Albums(items) => items
            .iter()
            .enumerate()
            .map(|(i, c)| {
                card_row(
                    player,
                    i,
                    &c.id,
                    &c.title,
                    &c.subtitle,
                    Message::OpenAlbum(c.id.clone(), c.title.clone()),
                )
            })
            .collect(),
        crate::youtube::SearchTab::Playlists(items) => items
            .iter()
            .enumerate()
            .map(|(i, c)| {
                card_row(
                    player,
                    i,
                    &c.id,
                    &c.title,
                    &c.subtitle,
                    Message::OpenPlaylist(c.id.clone(), c.title.clone()),
                )
            })
            .collect(),
        _ => Vec::new(),
    };

    if cards.is_empty() {
        if player.view_data().loading {
            return Container::new(
                text("Searching...")
                    .color(player.app_theme.palette.fg_secondary)
                    .center(),
            )
            .width(Length::Fill)
            .height(Length::Fill)
            .padding(theme::SPACING_XL)
            .into();
        }
        return track_list::empty_state("No results found");
    }

    scrollable(Column::with_children(cards))
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

/// A single drill-down card row rendered in the same style as a track row:
/// a leading index number, the item's thumbnail (or a placeholder), and the
/// title/subtitle. Clickable to drill down into the artist/album/playlist.
fn card_row<'a>(
    player: &'a MusicPlayer,
    index: usize,
    id: &'a str,
    title: &'a str,
    subtitle: &'a str,
    on_press: Message,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let thumb = player.thumbnail_index.get(id);
    let leading = text((index + 1).to_string())
        .size(theme::TEXT_SIZE_SM)
        .style(fg_secondary())
        .width(theme::TRACK_LEADING_WIDTH)
        .center();
    let thumb = track_list::thumbnail(p, theme::THUMBNAIL_SIZE, thumb);
    let inner = track_list::inner_row_layout(
        leading.into(),
        Some(thumb),
        title,
        if subtitle.is_empty() {
            None
        } else {
            Some(subtitle)
        },
        None,
    );
    track_list::track_row(
        Button::new(inner)
            .width(Length::Fill)
            .padding(0)
            .style(button_style_result_card())
            .on_press(on_press),
        p.bg,
    )
    .into()
}

pub(super) fn view_browse(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let tracks = player.view_data().tracks.as_slice();
    let loading = player.view_data().loading;

    let label: &str = match &player.view_data().kind {
        ViewKind::Artist { name, .. } => name,
        ViewKind::Album { title, .. } | ViewKind::PlaylistView { title, .. } => title,
        _ => "",
    };
    let header = Container::new(text(label).width(Length::Fill).center())
        .padding([theme::SPACING_SM, theme::SPACING_XL]);

    let track_list = if loading && tracks.is_empty() {
        loading_placeholder("Loading...")
    } else {
        view_track_list(tracks, player, TrackListKind::Active, 0)
    };

    Column::with_children(vec![header.into(), track_list])
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

pub(super) fn view_search_radio(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let label = player.view_data().label();
    let tracks = player.view_data().tracks.as_slice();
    let loading = player.view_data().loading;

    let header = Container::new(text(label).width(Length::Fill).center())
        .padding([theme::SPACING_SM, theme::SPACING_XL]);

    let track_list = if loading && tracks.is_empty() {
        loading_placeholder("Generating radio...")
    } else {
        view_track_list(tracks, player, TrackListKind::Active, 0)
    };

    Column::with_children(vec![header.into(), track_list])
        .spacing(0)
        .width(Length::Fill)
        .height(Length::Fill)
        .into()
}

fn loading_placeholder(msg: &str) -> Element<'_, Message, AppTheme> {
    Container::new(text(msg).style(fg_secondary()).center())
        .width(Length::Fill)
        .height(Length::Fill)
        .padding(theme::SPACING_XL)
        .into()
}

pub(super) fn view_search_history<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;

    if player.last_filtered_history.is_empty() {
        return Container::new(text("No recent searches").style(fg_secondary()))
            .width(Length::Fill)
            .height(theme::SEARCH_HISTORY_ITEM_HEIGHT)
            .padding([theme::SPACING_SM, theme::SPACING_XL])
            .style(bg_secondary())
            .into();
    }

    let items: Vec<Element<'a, Message, AppTheme>> = player
        .last_filtered_history
        .iter()
        .enumerate()
        .map(|(i, q)| {
            Container::new(
                Row::with_children(vec![
                    Button::new(
                        Row::with_children(vec![
                            icons::icon(icons::SEARCH_ICON, p.fg_muted, theme::ICON_SIZE_SM).into(),
                            text(q).size(theme::TEXT_SIZE_SM).into(),
                        ])
                        .spacing(theme::SPACING_SM)
                        .padding([theme::SPACING_XS, theme::SPACING_MD])
                        .align_y(alignment::Vertical::Center)
                        .width(Length::Fill),
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
            .width(Length::Fill)
            .style(bg_secondary())
            .into()
        })
        .collect();

    let dropdown_height = (player.last_filtered_history.len() as f32
        * theme::SEARCH_HISTORY_ITEM_HEIGHT)
        .min(theme::SEARCH_DROPDOWN_MAX_HEIGHT);

    let content = scrollable(Column::with_children(items).spacing(0).width(Length::Fill))
        .id(iced::widget::Id::new("search_history_list"))
        .width(Length::Fill)
        .height(dropdown_height);

    Container::new(content)
        .width(Length::Fill)
        .height(dropdown_height)
        .style(bg_secondary())
        .into()
}
