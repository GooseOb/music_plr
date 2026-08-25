use iced::{
    alignment,
    widget::{scrollable, text, Button, Column, Container, Row, Space},
    Element, Length,
};

use crate::{
    app::{
        view_data::{AlbumRef, PlaylistRef},
        TrackListKind, ViewKind,
    },
    load_state::LoadState,
    providers::{ArtistSection, ArtistSectionKind, ProviderId, SectionContent},
    theme::{self, AppTheme},
};

use super::{
    shared_components::{
        empty_state, loading_state, scope_tab_row, thumbnail, toggle_bookmark_button,
    },
    styles::{fg_accent, fg_secondary},
    view_track_list, Message, MusicPlayer,
};

const CARD_WIDTH: f32 = 140.0;
const CARD_IMAGE_SIZE: f32 = 120.0;

/// The artist page: header (picture, name, stats), then one section per row,
/// each with its own "Provided by" provider picker.
pub(super) fn view_artist<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let ViewKind::Artist(entry) = &player.view_data().kind else {
        return empty_state("Not an artist page");
    };
    let mut children: Vec<Element<'a, Message, AppTheme>> = Vec::new();
    children.push(header(
        player,
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
        children.push(section_header(kind, section.provider));
        children.push(section_body(player, section, kind));
    }

    scrollable(
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
/// provider supplying it so switching providers swaps the picture.
pub(crate) fn header_thumb_key(id: &str, provider: ProviderId) -> String {
    format!("artist-header:{}:{}", id, provider.label())
}

/// "Provided by [YT | SC]" picker for the header block (thumbnail /
/// description source).
fn header_provider_picker(selected: Option<ProviderId>) -> Element<'static, Message, AppTheme> {
    Row::with_children([
        text("Provided by")
            .size(theme::TEXT_SIZE_XS)
            .style(fg_secondary())
            .into(),
        scope_tab_row([ProviderId::YouTube, ProviderId::SoundCloud].map(|p| {
            (
                p.label().to_string(),
                selected == Some(p),
                Message::ArtistHeaderProviderChanged(p),
            )
        })),
    ])
    .spacing(theme::SPACING_XS)
    .align_y(alignment::Vertical::Center)
    .into()
}

/// Library bookmark toggle for this artist.
fn library_button(player: &MusicPlayer) -> Element<'static, Message, AppTheme> {
    let p = &player.app_theme.palette;
    match player.current_library_item() {
        Some(item) => {
            let saved = player.library.contains(item.kind, &item.id);
            Container::new(
                toggle_bookmark_button(p, saved).on_press(Message::ToggleLibrarySave(item)),
            )
            .into()
        }
        None => Space::new().width(theme::ICON_SIZE_SM).into(),
    }
}

/// Header block: picture, artist name and the "label: value" stat pairs of
/// whichever provider's header arrived first.
fn header<'a>(
    player: &'a MusicPlayer,
    id: &str,
    name: &'a str,
    header: Option<&'a crate::providers::ArtistHeader>,
    header_provider: Option<ProviderId>,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let thumb = player
        .thumbnail_index
        .get(&header_thumb_key(id, header_provider.unwrap_or_default()));
    let image = thumbnail(p, theme::PAGE_THUMBNAIL_SIZE, thumb);

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

    Row::with_children([
        image,
        Column::with_children(info)
            .spacing(theme::SPACING_2XS)
            .width(Length::Fill)
            .into(),
        Column::with_children([
            header_provider_picker(header_provider),
            Space::new().height(Length::Fill).into(),
            library_button(player),
        ])
        .align_x(alignment::Horizontal::Right)
        .into(),
    ])
    .spacing(theme::SPACING_LG)
    .align_y(alignment::Vertical::Center)
    .padding([theme::SPACING_MD, theme::SPACING_XL])
    .into()
}

fn section_header(
    kind: ArtistSectionKind,
    selected: Option<ProviderId>,
) -> Element<'static, Message, AppTheme> {
    let picker = scope_tab_row(kind.providers().iter().map(|&provider| {
        (
            provider.label(),
            selected == Some(provider),
            Message::ArtistSectionProviderChanged(kind, provider),
        )
    }));
    Container::new(
        Row::with_children([
            text(kind.label())
                .style(fg_accent())
                .size(theme::TEXT_SIZE_LG)
                .into(),
            Space::new().width(Length::Fill).into(),
            text("Provided by")
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

/// Render one card section's contents into card widgets.
fn cards<'a>(
    player: &'a MusicPlayer,
    provider: ProviderId,
    content: &'a SectionContent,
) -> Vec<Element<'a, Message, AppTheme>> {
    match content {
        SectionContent::Albums(v) => v
            .iter()
            .map(|c| {
                let subtitle: String = match (c.badge.as_str(), c.date.as_str()) {
                    ("", date) => date.to_string(),
                    (badge, "") => (*badge).to_string(),
                    (badge, date) => format!("{badge} \u{00b7} {date}"),
                };
                h_card(
                    player,
                    &c.id,
                    &c.title,
                    &subtitle,
                    Message::Browse(
                        ViewKind::Album(AlbumRef {
                            id: c.id.clone(),
                            name: c.title.clone(),
                            badge: c.badge.clone(),
                            date: c.date.clone(),
                            thumbnail: c.thumbnail.clone(),
                        }),
                        provider,
                    ),
                )
            })
            .collect(),
        SectionContent::Playlists(v) => v
            .iter()
            .map(|c| {
                h_card(
                    player,
                    &c.id,
                    &c.title,
                    "",
                    Message::Browse(
                        ViewKind::PlaylistView(PlaylistRef {
                            id: c.id.clone(),
                            name: c.title.clone(),
                            thumbnail: c.thumbnail.clone(),
                        }),
                        provider,
                    ),
                )
            })
            .collect(),
        SectionContent::Related(v) => v
            .iter()
            .map(|r| {
                h_card(
                    player,
                    &r.id,
                    &r.name,
                    &r.stat,
                    Message::OpenArtist {
                        id: r.id.clone(),
                        name: r.name.clone(),
                        source: provider,
                    },
                )
            })
            .collect(),
        SectionContent::Tracks(_) => Vec::new(),
    }
}

fn failed_state<'a>(
    provider: Option<ProviderId>,
    kind: ArtistSectionKind,
    e: &str,
) -> Element<'a, Message, AppTheme> {
    Container::new(
        Column::with_children([
            text(format!("Couldn't load: {e}")).into(),
            Button::new("Retry")
                .padding([theme::SPACING_2XS, theme::SPACING_SM])
                .on_press_maybe(provider.map(|p| Message::ArtistSectionProviderChanged(kind, p)))
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
    section: &'a ArtistSection,
    kind: ArtistSectionKind,
) -> Element<'a, Message, AppTheme> {
    let view_data = player.view_data();

    if kind == ArtistSectionKind::Popular {
        // Popular tracks live in the view's track list so all the usual
        // interactions (play, context menu, drag) work on them.
        return match &view_data.content {
            LoadState::Ready(tracks) => {
                if tracks.is_empty() {
                    empty_state("Nothing here")
                } else {
                    view_track_list(tracks.as_slice(), player, TrackListKind::Active, 0)
                }
            }
            LoadState::Failed(e) => return failed_state(section.provider, kind, e),
            LoadState::Loading => return loading_state(&player.app_theme.palette, "Loading..."),
        };
    }

    // Failed sections offer an in-place retry (re-requesting the provider).
    let content = match &section.state {
        LoadState::Ready(content) => content,
        LoadState::Failed(e) => return failed_state(section.provider, kind, e),
        LoadState::Loading => return loading_state(&player.app_theme.palette, "Loading..."),
    };
    let provider = section.provider.unwrap_or_default();
    h_scroll_cards(cards(player, provider, content))
}

/// A horizontal row of square art cards (pic on top, text below).
fn h_scroll_cards<'a, I>(cards: I) -> Element<'a, Message, AppTheme>
where
    I: IntoIterator<Item = Element<'a, Message, AppTheme>>,
{
    let row: Vec<Element<'a, Message, AppTheme>> = cards.into_iter().collect();
    if row.is_empty() {
        return empty_state("Nothing here");
    }
    scrollable(
        Row::with_children(row)
            .spacing(theme::SPACING_MD)
            .padding([theme::SPACING_SM, theme::SPACING_XL]),
    )
    .direction(scrollable::Direction::Horizontal(
        scrollable::Scrollbar::new(),
    ))
    .into()
}

/// One square art card: picture with title/date underneath. Clicking fires
/// `on_press` (drill-down).
fn h_card<'a>(
    player: &'a MusicPlayer,
    thumb_id: &str,
    title: &'a str,
    subtitle: &str,
    on_press: Message,
) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let thumb_path = player.thumbnail_index.get(thumb_id);
    let image = Container::new(thumbnail(p, CARD_IMAGE_SIZE, thumb_path)).height(CARD_IMAGE_SIZE);
    let mut body = vec![image.into()];
    body.push(
        text(title)
            .size(theme::TEXT_SIZE_SM)
            .width(CARD_WIDTH)
            .wrapping(text::Wrapping::Word)
            .into(),
    );
    if !subtitle.trim().is_empty() {
        body.push(
            text(subtitle.trim().to_string())
                .size(theme::TEXT_SIZE_XS)
                .style(fg_secondary())
                .into(),
        );
    }
    Button::new(Column::with_children(body).spacing(theme::SPACING_2XS))
        .padding(theme::SPACING_2XS)
        .width(CARD_WIDTH)
        .style(super::styles::button_style_album())
        .on_press(on_press)
        .into()
}
