//! The Home feed: a continue strip, the local rails, the pane's quick-pick
//! track list and the provider discovery rows.
//!
//! Local rows are read straight from their stores here (so they never go
//! stale), while provider rows come from [`HomeData`]. The whole page is one
//! card-host scrollable, like the artist page, so it works in a narrow split
//! pane and the quick-pick list nests inside it.

use iced::{
    alignment,
    widget::{text, Button, Container, Space},
    Element, Length,
};

use super::{
    shared_components::{
        art_card, card_rail, click_card, play_pause_button, section_label, thumbnail,
        CARD_IMAGE_SIZE,
    },
    styles::{bg_secondary, fg_secondary, icon_style_fg_secondary},
    theme, Message, MusicPlayer,
};
use crate::{
    app::{
        home::{HomeRailId, HomeTrackSource},
        pane::PaneId,
        update::operation::RailKey,
        ViewData, ViewKind,
    },
    data::library::LibraryItem,
    icons,
    load_state::LoadState,
    providers::{HomeCard, HomeSection, HomeSectionData, ProviderId},
    theme::AppTheme,
};

const CONTINUE_IMAGE_SIZE: f32 = 56.0;
const QUICK_PICK_ROWS: usize = 8;

pub(super) fn view_home<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
) -> Element<'a, Message, AppTheme> {
    let dir = player.strings.direction;
    let view = player.view_data_in(pane);
    let ViewKind::Home(home) = &view.kind else {
        return text(player.strings.nothing_here).into();
    };

    let mut children: Vec<Element<'a, Message, AppTheme>> = Vec::new();
    children.push(header(player, pane));

    if let Some(banner) = continue_strip(player) {
        children.push(banner);
    }

    let mut rails: Vec<Element<'a, Message, AppTheme>> = Vec::new();
    if let Some(rail) = recent_rail(player, pane) {
        rails.push(rail);
    }
    if let Some(rail) = playlists_rail(player, pane) {
        rails.push(rail);
    }
    if let Some(rail) = library_rail(player, pane) {
        rails.push(rail);
    }
    if let Some(rail) = downloads_rail(player, pane) {
        rails.push(rail);
    }
    children.extend(rails);

    if let LoadState::Ready(tracks) = &view.content {
        if !tracks.is_empty() {
            let label = match home.quick_picks_provider {
                Some(provider) => crate::providers::home::titled(
                    player.strings,
                    player.strings.quick_picks,
                    provider,
                ),
                None => player.strings.quick_picks.to_string(),
            };
            children.push(section_title(dir, label));
            // Bounded so the page scrolls around it instead of the list
            // trapping the wheel for its whole height; longer lists scroll
            // inside their own viewport.
            let rows = tracks.len().min(QUICK_PICK_ROWS);
            children.push(
                Container::new(super::track_list::view_track_list(tracks, player, pane, 0))
                    .height(Length::Fixed(theme::ROW_HEIGHT * rows as f32))
                    .into(),
            );
        }
    } else if matches!(view.content, LoadState::Loading) && home.is_loading() {
        children.push(super::shared_components::loading_state(
            player.strings.loading,
        ));
    }

    let mut block: Option<ProviderId> = None;
    for section in &home.sections {
        let provider = section.key.provider;
        if block != Some(provider) {
            block = Some(provider);
            children.push(provider_header(player, provider));
            if let Some(rail) = server_playlists_rail(player, pane, provider) {
                children.push(rail);
            }
        }
        children.push(section_rail(player, pane, section));
    }

    super::card_scrollable(
        pane,
        iced::widget::Column::with_children(children)
            .spacing(theme::SPACING_SM)
            .padding(iced::Padding {
                bottom: theme::SPACING_LG,
                ..Default::default()
            }),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

fn header(player: &MusicPlayer, pane: PaneId) -> Element<'_, Message, AppTheme> {
    let dir = player.strings.direction;
    let refresh = Button::new(
        icons::icon(icons::REFRESH_ICON, theme::ICON_SIZE_MD).style(icon_style_fg_secondary()),
    )
    .padding(theme::SPACING_2XS)
    .on_press(Message::HomeRefresh(pane));

    dir.row([
        text(player.strings.home).size(theme::TEXT_SIZE_XL).into(),
        Space::new().width(Length::Fill).into(),
        refresh.into(),
    ])
    .align_y(alignment::Vertical::Center)
    .padding([theme::SPACING_MD, theme::SPACING_XL])
    .into()
}

fn continue_strip(player: &MusicPlayer) -> Option<Element<'_, Message, AppTheme>> {
    let track = player.queue.current()?;
    let dir = player.strings.direction;
    let image = thumbnail(
        CONTINUE_IMAGE_SIZE,
        player.thumbnail_index.get(track.source, track.primary_id()),
    );
    let info = iced::widget::Column::with_children([
        text(track.title.clone()).size(theme::TEXT_SIZE_MD).into(),
        text(track.artist.clone())
            .size(theme::TEXT_SIZE_SM)
            .style(fg_secondary())
            .into(),
    ])
    .spacing(theme::SPACING_XXS)
    .width(Length::Fill);

    Some(
        Container::new(
            dir.row([
                image,
                info.into(),
                play_pause_button(player.is_playing)
                    .on_press(Message::TogglePlayPause)
                    .into(),
            ])
            .spacing(theme::SPACING_MD)
            .align_y(alignment::Vertical::Center),
        )
        .padding(theme::SPACING_MD)
        .width(Length::Fill)
        .style(bg_secondary())
        .into(),
    )
}

fn provider_header(player: &MusicPlayer, provider: ProviderId) -> Element<'_, Message, AppTheme> {
    Container::new(section_label(
        player.strings.direction,
        player.strings.provider(provider),
    ))
    .width(Length::Fill)
    .padding([theme::SPACING_MD, theme::SPACING_XL])
    .into()
}

fn section_title<'a>(
    dir: crate::i18n::Direction,
    label: impl text::IntoFragment<'a>,
) -> Element<'a, Message, AppTheme> {
    Container::new(
        dir.row([text(label)
            .size(theme::TEXT_SIZE_LG)
            .width(Length::Fill)
            .into()])
            .padding([theme::SPACING_SM, theme::SPACING_XL]),
    )
    .width(Length::Fill)
    .into()
}

/// Shared shell for a horizontal rail: title above the shared card shelf,
/// which carries the rail's stable identity so its offset stays put across
/// refreshes and a card's right-click lands where the cursor is.
fn rail<'a>(
    pane: PaneId,
    rail: HomeRailId,
    title: Element<'a, Message, AppTheme>,
    cards: Vec<Element<'a, Message, AppTheme>>,
) -> Element<'a, Message, AppTheme> {
    iced::widget::Column::with_children([title, card_rail(pane, RailKey::Home(rail), cards)])
        .width(Length::Fill)
        .into()
}

fn track_tile<'a>(
    player: &'a MusicPlayer,
    track: &'a crate::types::Track,
    message: Message,
) -> Element<'a, Message, AppTheme> {
    let image = thumbnail(
        CARD_IMAGE_SIZE,
        player.thumbnail_index.get(track.source, track.primary_id()),
    );
    click_card(image, &track.title, &track.artist, message)
}

fn recent_rail<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
) -> Option<Element<'a, Message, AppTheme>> {
    let tracks: Vec<Element<'a, Message, AppTheme>> = player
        .queue
        .recently_played
        .iter()
        .take(20)
        .enumerate()
        .map(|(index, track)| {
            track_tile(
                player,
                track,
                Message::PlayHomeTrack {
                    pane,
                    source: HomeTrackSource::Recent,
                    index,
                },
            )
        })
        .collect();
    if tracks.is_empty() {
        return None;
    }
    Some(rail(
        pane,
        HomeRailId::Recent,
        section_title(player.strings.direction, player.strings.recently_played),
        tracks,
    ))
}

fn playlists_rail<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
) -> Option<Element<'a, Message, AppTheme>> {
    if player.playlists.playlists.is_empty() {
        return None;
    }
    let dir = player.strings.direction;
    let cards: Vec<Element<'a, Message, AppTheme>> = player
        .playlists
        .playlists
        .iter()
        .enumerate()
        .map(|(index, playlist)| {
            let cover = playlist
                .tracks
                .first()
                .and_then(|track| player.thumbnail_index.get(track.source, track.primary_id()));
            let image = thumbnail(CARD_IMAGE_SIZE, cover);
            click_card(
                image,
                &playlist.name,
                &playlist.tracks.len().to_string(),
                Message::NavigateTo(pane, ViewData::new_playlist(index, playlist.name.clone())),
            )
        })
        .collect();
    Some(rail(
        pane,
        HomeRailId::Playlists,
        section_title(dir, player.strings.playlists),
        cards,
    ))
}

fn library_rail<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
) -> Option<Element<'a, Message, AppTheme>> {
    if player.library.items.is_empty() {
        return None;
    }
    let cards: Vec<Element<'a, Message, AppTheme>> = player
        .library
        .items
        .iter()
        .take(30)
        .map(|item| {
            art_card(
                player,
                pane,
                ViewKind::from(item.as_ref()),
                &item.title,
                "",
                RailKey::Home(HomeRailId::Library),
            )
        })
        .collect();
    Some(rail(
        pane,
        HomeRailId::Library,
        section_title(player.strings.direction, player.strings.library),
        cards,
    ))
}

fn downloads_rail<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
) -> Option<Element<'a, Message, AppTheme>> {
    let downloaded = player.download_registry.tracks();
    if downloaded.is_empty() {
        return None;
    }
    let cards: Vec<Element<'a, Message, AppTheme>> = downloaded
        .into_iter()
        .take(20)
        .enumerate()
        .map(|(index, track)| {
            track_tile(
                player,
                track,
                Message::PlayHomeTrack {
                    pane,
                    source: HomeTrackSource::Downloads,
                    index,
                },
            )
        })
        .collect();
    Some(rail(
        pane,
        HomeRailId::Downloads,
        section_title(player.strings.direction, player.strings.downloaded_tracks),
        cards,
    ))
}

/// A custom server's playlist index, drawn straight from the store like the
/// local rails (the tracks themselves are fetched when a card is opened).
fn server_playlists_rail<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    provider: ProviderId,
) -> Option<Element<'a, Message, AppTheme>> {
    let cards: Vec<Element<'a, Message, AppTheme>> = player
        .server_playlists
        .for_provider(provider)
        .take(20)
        .map(|entry| {
            let kind = ViewKind::PlaylistView(crate::app::view_data::PlaylistRef {
                id: entry.id.clone(),
                name: entry.name.clone(),
                provider: entry.provider,
                thumbnail: String::new(),
                url: String::new(),
            });
            art_card(
                player,
                pane,
                kind,
                &entry.name,
                &entry.track_count.to_string(),
                RailKey::Home(HomeRailId::ServerPlaylists(provider)),
            )
        })
        .collect();
    if cards.is_empty() {
        return None;
    }
    Some(rail(
        pane,
        HomeRailId::ServerPlaylists(provider),
        section_title(player.strings.direction, player.strings.playlists),
        cards,
    ))
}

fn section_rail<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    section: &'a HomeSection,
) -> Element<'a, Message, AppTheme> {
    let rail_id = HomeRailId::Section(section.key);
    let cards: Vec<Element<'a, Message, AppTheme>> = match &section.data {
        HomeSectionData::Tracks(tracks) => tracks
            .iter()
            .enumerate()
            .map(|(track_index, track)| {
                track_tile(
                    player,
                    track,
                    Message::PlayHomeTrack {
                        pane,
                        source: HomeTrackSource::Section(section.key),
                        index: track_index,
                    },
                )
            })
            .collect(),
        HomeSectionData::Cards(cards) => cards
            .iter()
            .map(|card| card_rail_tile(player, pane, rail_id, card))
            .collect(),
    };
    rail(
        pane,
        rail_id,
        section_title(
            player.strings.direction,
            section.display_title(player.strings),
        ),
        cards,
    )
}

fn card_rail_tile<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    rail: HomeRailId,
    card: &'a HomeCard,
) -> Element<'a, Message, AppTheme> {
    let item = LibraryItem::from(card);
    art_card(
        player,
        pane,
        ViewKind::from(&item),
        &card.title,
        &card.subtitle,
        RailKey::Home(rail),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        app::MusicPlayer,
        data::config::Config,
        providers::{HomeSection, HomeSectionKey, HomeSectionKind},
    };

    fn home_view() -> ViewData {
        let mut view = ViewData::new_home();
        if let ViewKind::Home(home) = &mut view.kind {
            let server = ProviderId::Custom(
                crate::data::custom_providers::CustomProviderKind::Subsonic,
                1,
            );
            home.sections.push(HomeSection::new(
                HomeSectionKey::new(server, HomeSectionKind::Discover),
                String::new(),
                HomeSectionData::Cards(vec![HomeCard {
                    kind: crate::data::library::LibraryKind::Album,
                    provider: server,
                    id: "al-1".to_string(),
                    title: "Server album".to_string(),
                    subtitle: "Artist".to_string(),
                    thumbnail: String::new(),
                    url: String::new(),
                }]),
            ));
            home.sections.push(HomeSection::new(
                HomeSectionKey::new(ProviderId::Bandcamp, HomeSectionKind::NewReleases),
                String::new(),
                HomeSectionData::Cards(vec![HomeCard {
                    kind: crate::data::library::LibraryKind::Album,
                    provider: ProviderId::Bandcamp,
                    id: "1:2:a".to_string(),
                    title: "Album".to_string(),
                    subtitle: "Artist".to_string(),
                    thumbnail: String::new(),
                    url: String::new(),
                }]),
            ));
        }
        view
    }

    #[test]
    fn an_empty_home_renders_only_the_header() {
        let mut player = MusicPlayer::new_with(Config::default());
        player.reset_test_pane(vec![ViewData::new_home()]);
        let _ = view_home(&player, 0);
    }

    #[test]
    fn provider_rows_and_quick_picks_render() {
        let mut player = MusicPlayer::new_with(Config::default());
        let server = ProviderId::Custom(
            crate::data::custom_providers::CustomProviderKind::Subsonic,
            1,
        );
        player
            .server_playlists
            .entries
            .push(crate::data::server_playlists::ServerPlaylistEntry {
                provider: server,
                id: "pl-1".to_string(),
                name: "Server playlist".to_string(),
                track_count: 4,
            });
        let mut view = home_view();
        view.content = LoadState::Ready(Vec::new());
        player.reset_test_pane(vec![view]);
        let _ = view_home(&player, 0);
    }
}
