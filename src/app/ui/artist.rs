use iced::{
    alignment,
    widget::{text, Button, Column, Container},
    Element, Length,
};

use super::{
    shared_components::{
        art_card, card_rail, empty_state, loading_state, open_in_browser_button, scope_tab_row,
        thumbnail, toggle_bookmark_button,
    },
    styles::{fg_accent, fg_secondary},
    view_track_list, Message, MusicPlayer,
};
use crate::{
    app::{
        pane::PaneId,
        update::operation::RailKey,
        view_data::{AlbumRef, PlaylistRef},
        ViewKind,
    },
    load_state::LoadState,
    providers::{ArtistSection, ArtistSectionKind, ProviderId, SectionContent},
    theme::{self, AppTheme},
};

/// The artist page: header (picture, name, stats), then one section per row,
/// each with its own "Provided by" provider picker.
fn section_kind_label(kind: ArtistSectionKind, tr: &crate::i18n::Strings) -> &str {
    match kind {
        ArtistSectionKind::Popular => tr.most_popular_songs,
        ArtistSectionKind::Albums => tr.albums,
        ArtistSectionKind::Playlists => tr.playlists,
        ArtistSectionKind::Related => tr.fans_also_like,
    }
}

pub(super) fn view_artist<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
) -> Element<'a, Message, AppTheme> {
    let ViewKind::Artist(entry) = &player.view_data_in(pane).kind else {
        return empty_state(player.strings.not_an_artist_page);
    };
    let mut children: Vec<Element<'a, Message, AppTheme>> = Vec::new();
    children.push(header(
        player,
        pane,
        &entry.id,
        &entry.name,
        entry.page.header.as_ref(),
        entry.page.header_provider,
    ));

    for kind in [
        ArtistSectionKind::Popular,
        ArtistSectionKind::Albums,
        ArtistSectionKind::Playlists,
        ArtistSectionKind::Related,
    ] {
        let section = entry.page.section(kind);
        children.push(section_header(pane, kind, section.provider, player.strings));
        children.push(section_body(player, pane, section, kind));
    }

    super::card_scrollable(
        pane,
        Column::with_children(children)
            .spacing(theme::SPACING_2XS)
            .padding(iced::Padding {
                bottom: theme::SPACING_LG,
                ..Default::default()
            }),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .into()
}

/// Thumbnail cache key for the artist header image, namespaced by the
/// provider supplying it so switching providers swaps the picture. The `large`
/// marker keeps the full-size header art from being served by an older
/// small-thumbnail cache entry.
pub(crate) fn header_thumb_key(id: &str) -> String {
    format!("artist-header:large:{id}")
}

/// Provider picker for the header block (thumbnail / description source).
fn header_provider_picker(
    pane: PaneId,
    selected: Option<ProviderId>,
    tr: &'static crate::i18n::Strings,
) -> Element<'static, Message, AppTheme> {
    tr.direction
        .row([
            text(tr.provided_by)
                .size(theme::TEXT_SIZE_XS)
                .style(fg_secondary())
                .into(),
            scope_tab_row(
                tr.direction,
                ProviderId::header_providers().iter().map(|&p| {
                    (
                        tr.provider(p),
                        selected == Some(p),
                        Message::ArtistHeaderProviderChanged(pane, p),
                    )
                }),
            ),
        ])
        .spacing(theme::SPACING_XS)
        .align_y(alignment::Vertical::Center)
        .into()
}

/// Header block: picture, artist name and the "label: value" stat pairs of
/// whichever provider's header arrived first.
fn header<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    id: &str,
    name: &'a str,
    header: Option<&'a crate::providers::ArtistHeader>,
    header_provider: Option<ProviderId>,
) -> Element<'a, Message, AppTheme> {
    let dir = player.strings.direction;
    let thumb = player
        .thumbnail_index
        .get(header_provider.unwrap_or_default(), &header_thumb_key(id));
    let image = thumbnail(theme::PAGE_THUMBNAIL_SIZE, thumb);

    let stats_line = header
        .as_ref()
        .map(|h| {
            h.stats
                .iter()
                .map(|(label, value)| format!("{label}: {value}"))
                .collect::<Vec<_>>()
                .join("  \u{00b7}  ")
        })
        .unwrap_or_default();

    let mut info = vec![
        text(name).size(theme::TEXT_SIZE_XL).into(),
        text(stats_line)
            .size(theme::TEXT_SIZE_SM)
            .style(fg_secondary())
            .into(),
    ];
    if let Some(description) = header
        .as_ref()
        .map(|h| h.description.as_str())
        .filter(|d| !d.is_empty())
    {
        info.push(
            text(description)
                .size(theme::TEXT_SIZE_XS)
                .style(fg_secondary())
                .into(),
        );
    }

    let mut actions = vec![header_provider_picker(
        pane,
        header_provider,
        player.strings,
    )];
    if let Some(item) = player.current_library_item(pane) {
        let saved = player.library.contains(item.kind, &item.id);
        // The artist's public page, when the provider advertised one.
        let browser_url = item.web_url().map(str::to_string);
        let mut buttons: Vec<Element<'_, Message, AppTheme>> = vec![toggle_bookmark_button(saved)
            .on_press(Message::ToggleLibrarySave(item))
            .into()];
        if let Some(url) = browser_url {
            buttons.push(open_in_browser_button(player.strings, &url));
        }
        actions.push(
            Container::new(dir.row(buttons).spacing(theme::SPACING_XS))
                .align_y(alignment::Vertical::Bottom)
                .height(Length::Fill)
                .into(),
        );
    }

    let dir = player.strings.direction;
    dir.row([
        image,
        Column::with_children(info)
            .spacing(theme::SPACING_2XS)
            .width(Length::Fill)
            .height(Length::Fill)
            .align_x(dir.align())
            .into(),
        Column::with_children(actions)
            .align_x(player.strings.direction.end_align())
            .into(),
    ])
    .spacing(theme::SPACING_LG)
    .padding([theme::SPACING_MD, theme::SPACING_XL])
    .into()
}

fn section_header(
    pane: PaneId,
    kind: ArtistSectionKind,
    selected: Option<ProviderId>,
    tr: &'static crate::i18n::Strings,
) -> Element<'static, Message, AppTheme> {
    let picker = scope_tab_row(
        tr.direction,
        kind.providers().iter().map(|&provider| {
            (
                tr.provider(provider),
                selected == Some(provider),
                Message::ArtistSectionProviderChanged(pane, kind, provider),
            )
        }),
    );
    Container::new(
        tr.direction
            .row([
                text(section_kind_label(kind, tr))
                    .style(fg_accent())
                    .size(theme::TEXT_SIZE_LG)
                    .width(Length::Fill)
                    .into(),
                text(tr.provided_by)
                    .size(theme::TEXT_SIZE_SM)
                    .style(fg_secondary())
                    .into(),
                picker,
            ])
            .align_y(alignment::Vertical::Center)
            .spacing(theme::SPACING_MD)
            .padding([theme::SPACING_SM, theme::SPACING_XL]),
    )
    .into()
}

/// Render one card section's contents into card widgets. Each card carries the
/// exact view its click opens, which is also what its right-click menu acts on.
fn cards<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    provider: ProviderId,
    content: &'a SectionContent,
    row: ArtistSectionKind,
) -> Vec<Element<'a, Message, AppTheme>> {
    match content {
        SectionContent::Albums(v) => v
            .iter()
            .map(|c| {
                let subtitle: std::borrow::Cow<'_, str> =
                    super::search::browse_meta(&c.badge, &c.date);
                let target = ViewKind::Album(AlbumRef {
                    id: c.id.clone(),
                    name: c.title.clone(),
                    badge: c.badge.clone(),
                    date: c.date.clone(),
                    thumbnail: c.thumbnail.clone(),
                    provider,
                    url: c.url.clone(),
                });
                art_card(
                    player,
                    pane,
                    target,
                    &c.title,
                    &subtitle,
                    RailKey::Artist(row),
                )
            })
            .collect(),
        SectionContent::Playlists(v) => v
            .iter()
            .map(|c| {
                let target = ViewKind::PlaylistView(PlaylistRef {
                    id: c.id.clone(),
                    name: c.title.clone(),
                    thumbnail: c.thumbnail.clone(),
                    provider,
                    url: c.url.clone(),
                });
                art_card(player, pane, target, &c.title, "", RailKey::Artist(row))
            })
            .collect(),
        SectionContent::Related(v) => v
            .iter()
            .map(|r| {
                let target = ViewKind::Artist(crate::app::view_data::ArtistEntry {
                    id: r.id.clone(),
                    name: r.name.clone(),
                    source: provider,
                    url: r.url.clone(),
                    thumbnail: r.thumbnail.clone(),
                    page: Box::new(crate::providers::ArtistPageState::new(provider, &r.id)),
                });
                art_card(player, pane, target, &r.name, &r.stat, RailKey::Artist(row))
            })
            .collect(),
        SectionContent::Tracks(_) => Vec::new(),
    }
}

fn failed_state<'a>(
    pane: PaneId,
    provider: Option<ProviderId>,
    kind: ArtistSectionKind,
    e: &str,
    tr: &'a crate::i18n::Strings,
) -> Element<'a, Message, AppTheme> {
    Container::new(
        Column::with_children([
            text((tr.couldnt_load)(e)).into(),
            Button::new(tr.retry)
                .padding([theme::SPACING_2XS, theme::SPACING_SM])
                .on_press_maybe(
                    provider.map(|p| Message::ArtistSectionProviderChanged(pane, kind, p)),
                )
                .into(),
        ])
        .align_x(alignment::Horizontal::Center)
        .spacing(theme::SPACING_SM),
    )
    .center(Length::Fill)
    .into()
}

fn section_body<'a>(
    player: &'a MusicPlayer,
    pane: PaneId,
    section: &'a ArtistSection,
    kind: ArtistSectionKind,
) -> Element<'a, Message, AppTheme> {
    let view_data = player.view_data_in(pane);

    if kind == ArtistSectionKind::Popular {
        // Popular tracks live in the view's track list so all the usual
        // interactions (play, context menu, drag) work on them.
        return match &view_data.content {
            LoadState::Ready(tracks) => {
                if tracks.is_empty() {
                    empty_state(player.strings.nothing_here)
                } else {
                    view_track_list(tracks.as_slice(), player, pane, 0)
                }
            }
            LoadState::Failed(e) => {
                return failed_state(pane, section.provider, kind, e, player.strings);
            }
            LoadState::Loading => {
                return loading_state(player.strings.loading);
            }
        };
    }

    // Failed sections offer an in-place retry (re-requesting the provider).
    let content = match &section.state {
        LoadState::Ready(content) => content,
        LoadState::Failed(e) => {
            return failed_state(pane, section.provider, kind, e, player.strings);
        }
        LoadState::Loading => {
            return loading_state(player.strings.loading);
        }
    };
    let provider = section.provider.unwrap_or_default();
    h_scroll_cards(
        cards(player, pane, provider, content, kind),
        player.strings,
        pane,
        kind,
    )
}

/// A horizontal row of square art cards, or the section's empty state.
fn h_scroll_cards<'a, I>(
    cards: I,
    tr: &'a crate::i18n::Strings,
    pane: PaneId,
    section: ArtistSectionKind,
) -> Element<'a, Message, AppTheme>
where
    I: IntoIterator<Item = Element<'a, Message, AppTheme>>,
{
    let row: Vec<Element<'a, Message, AppTheme>> = cards.into_iter().collect();
    if row.is_empty() {
        return empty_state(tr.nothing_here);
    }
    card_rail(pane, RailKey::Artist(section), row)
}
