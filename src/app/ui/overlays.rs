use std::borrow::Cow;

use iced::{
    alignment,
    widget::{column, container, row, text, Button, Column, Container, MouseArea, Row, Space},
    Element, Length, Rectangle,
};

use crate::{
    app::interaction::TrackListKind,
    icons,
    theme::{AppTheme, Palette},
};

use super::{
    styles::{bg_overlay, bg_popup, button_style_danger, button_style_popup_item, fg_secondary},
    theme, ContextMenuState, Message, MusicPlayer,
};

/// A drop-indicator line drawn on top of the UI at the given screen-space
/// rectangle (computed from captured row geometry), so it never perturbs the
/// list layout it marks.
pub(super) fn view_drop_indicator(rect: Rectangle, p: &Palette) -> Element<'_, Message, AppTheme> {
    column![
        Space::new().height(rect.y),
        row![
            Space::new().width(rect.x),
            Container::new(
                Space::new()
                    .width(rect.width)
                    .height(crate::theme::DROP_LINE_HEIGHT),
            )
            .style(move |_: &AppTheme| container::Style {
                background: Some(p.accent.into()),
                ..Default::default()
            })
        ]
    ]
    .into()
}

#[allow(clippy::too_many_lines)]
pub(super) fn view_context_menu<'a>(
    menu: &'a ContextMenuState,
    p: &'a Palette,
) -> Element<'a, Message, AppTheme> {
    let (pos_x, pos_y) = menu.position;

    let items: Vec<Element<'_, Message, AppTheme>> = {
        let mut v: Vec<Element<'_, Message, AppTheme>> = vec![menu_item(
            Cow::Borrowed("Play"),
            icons::PLAY_ICON,
            p,
            Message::ContextMenuPlayTrack(menu.pos),
        )
        .into()];

        if menu.track.artist.id.is_some() {
            v.push(
                menu_item(
                    Cow::Borrowed("Go to artist"),
                    icons::ARTIST_ICON,
                    p,
                    Message::ContextMenuGoToArtist,
                )
                .into(),
            );
        }

        // Per-provider playback/download. Providers the track already carries
        // an id for play/download directly; others show a search icon and
        // trigger an id-resolving lookup first.
        for &provider in crate::provider::ProviderId::searchable() {
            if !provider.capabilities().stream {
                continue;
            }
            let has_id = menu.track.has_provider(provider);
            let label = if has_id {
                Cow::Owned(format!("Play via {}", provider.label()))
            } else {
                Cow::Owned(format!("Play via {} (search)", provider.label()))
            };
            let icon = if has_id {
                icons::PLAY_ICON
            } else {
                icons::SEARCH_ICON
            };
            v.push(
                menu_item(
                    label,
                    icon,
                    p,
                    Message::ContextMenuPlayViaProvider(provider, menu.pos),
                )
                .into(),
            );
        }

        let target_indices = &menu.target_indices;
        let n = target_indices.len();

        let add_label = if n > 1 {
            Cow::Owned(format!("Add {n} tracks to Playlist"))
        } else {
            Cow::Borrowed("Add to Playlist")
        };
        v.push(
            menu_item(
                add_label,
                icons::FOLDER_ICON,
                p,
                Message::TogglePicker(target_indices.clone()),
            )
            .into(),
        );

        // Download: for each stream+download provider, offer direct download
        // when the track has an id, or a search-then-download otherwise.
        for &provider in crate::provider::ProviderId::defaultable() {
            if !provider.capabilities().download {
                continue;
            }
            let has_id = menu.track.has_provider(provider);
            let label = if has_id {
                Cow::Owned(format!("Download from {}", provider.label()))
            } else {
                Cow::Owned(format!("Download from {} (search)", provider.label()))
            };
            let icon = if has_id {
                icons::DOWNLOAD_ICON
            } else {
                icons::SEARCH_ICON
            };
            v.push(
                menu_item(
                    label,
                    icon,
                    p,
                    Message::ContextMenuDownloadViaProvider(provider, target_indices.clone()),
                )
                .into(),
            );
        }

        // Radio: only providers that support similarity search, and only when
        // the track already carries that provider's id.
        for &provider in crate::provider::ProviderId::searchable() {
            if !provider.capabilities().radio {
                continue;
            }
            if menu.track.has_provider(provider) {
                v.push(
                    menu_item(
                        Cow::Owned(format!("Song Radio – {}", provider.label())),
                        icons::RADIO_ICON,
                        p,
                        Message::ContextMenuSongRadioProvider(provider),
                    )
                    .into(),
                );
                v.push(
                    menu_item(
                        Cow::Owned(format!("Artist Radio – {}", provider.label())),
                        icons::RADIO_ICON,
                        p,
                        Message::ContextMenuArtistRadioProvider(provider),
                    )
                    .into(),
                );
            }
        }

        if menu.pos.list == TrackListKind::Queue {
            let label = if n > 1 {
                Cow::Owned(format!("Remove {n} tracks from queue"))
            } else {
                Cow::Borrowed("Remove from Queue")
            };
            v.push(
                menu_item(
                    label,
                    icons::DELETE_ICON,
                    p,
                    Message::ContextMenuRemoveFromQueue(target_indices.clone()),
                )
                .into(),
            );
        } else if menu.in_playlist && menu.pos.list != TrackListKind::Recent {
            let label = if n > 1 {
                Cow::Owned(format!("Remove {n} tracks from playlist"))
            } else {
                Cow::Borrowed("Remove from Playlist")
            };
            v.push(
                menu_item(
                    label,
                    icons::DELETE_ICON,
                    p,
                    Message::ContextMenuRemoveFromPlaylist(target_indices.clone()),
                )
                .into(),
            );
        }

        v
    };

    let menu_content = Container::new(
        Column::with_children(items)
            .spacing(2)
            .padding(theme::SPACING_SM),
    )
    .width(theme::CONTEXT_MENU_WIDTH)
    .style(bg_popup());

    let overlay = Container::new(pos_absolute(menu_content.into(), pos_x, pos_y))
        .width(Length::Fill)
        .height(Length::Fill);

    MouseArea::new(overlay)
        .on_press(Message::CloseContextMenu)
        .into()
}

pub fn pos_absolute(
    content: Element<'_, Message, AppTheme>,
    pos_x: impl Into<Length>,
    pos_y: impl Into<Length>,
) -> Column<'_, Message, AppTheme> {
    column![
        Space::new().height(pos_y),
        Row::with_children([Space::new().width(pos_x).into(), content])
    ]
}

fn menu_item<'a>(
    label: Cow<'a, str>,
    icon: &'static [u8],
    p: &'a Palette,
    on_press: Message,
) -> Button<'a, Message, AppTheme> {
    Button::new(
        Row::with_children([
            icons::icon(icon, p.fg_muted, theme::ICON_SIZE_SM).into(),
            text(label).into(),
        ])
        .spacing(theme::SPACING_SM)
        .padding([theme::SPACING_XS, theme::SPACING_SM])
        .align_y(alignment::Vertical::Center)
        .width(Length::Fill),
    )
    .padding(0)
    .style(button_style_popup_item())
    .on_press(on_press)
}

pub(super) fn view_playlist_picker(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let items = player
        .playlists
        .playlists
        .iter()
        .enumerate()
        .map(|(i, pl)| {
            Button::new(
                Row::with_children([text(&pl.name).into()])
                    .spacing(theme::SPACING_SM)
                    .align_y(alignment::Vertical::Center)
                    .width(Length::Fill),
            )
            .padding([theme::SPACING_SM, theme::SPACING_MD])
            .style(button_style_popup_item())
            .on_press(Message::AddToPlaylist(i))
            .into()
        });

    let cancel_btn = Button::new(
        Container::new(text("Cancel").size(theme::TEXT_SIZE_SM)).center_x(Length::Fill),
    )
    .padding(theme::SPACING_SM)
    .on_press(Message::ClosePicker);

    view_dialog(
        Column::with_children([
            text("Add to Playlist").size(theme::TEXT_SIZE_LG).into(),
            Column::with_children(items)
                .spacing(theme::SPACING_XS)
                .width(Length::Fill)
                .into(),
            cancel_btn.into(),
        ])
        .align_x(alignment::Horizontal::Center)
        .spacing(theme::SPACING_SM)
        .width(theme::DIALOG_WIDTH)
        .padding(theme::SPACING_MD)
        .into(),
        Message::ClosePicker,
    )
}

pub fn no_click_propagation(
    content: Element<'_, Message, AppTheme>,
) -> Element<'_, Message, AppTheme> {
    MouseArea::new(content).on_press(Message::Noop).into()
}

fn view_dialog(
    dialog: Element<'_, Message, AppTheme>,
    close_msg: Message,
) -> Element<'_, Message, AppTheme> {
    let dialog = no_click_propagation(Container::new(dialog).style(bg_popup()).into());

    Container::new(MouseArea::new(Container::new(dialog).center(Length::Fill)).on_press(close_msg))
        .style(bg_overlay())
        .into()
}

pub(super) fn view_delete_confirm() -> Element<'static, Message, AppTheme> {
    let cancel_btn = Button::new(Container::new(text("Cancel")).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .on_press(Message::HideDeleteConfirm);

    let delete_btn = Button::new(Container::new(text("Delete")).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .style(button_style_danger())
        .on_press(Message::ConfirmDeletePlaylist);

    view_dialog(
        Column::with_children([
            text("Delete playlist?").size(theme::TEXT_SIZE_LG).into(),
            text("Tracks will not be deleted.")
                .style(fg_secondary())
                .into(),
            Row::with_children([cancel_btn.into(), delete_btn.into()])
                .spacing(theme::SPACING_XL)
                .align_y(alignment::Vertical::Center)
                .into(),
        ])
        .width(theme::DIALOG_WIDTH)
        .align_x(alignment::Horizontal::Center)
        .spacing(theme::SPACING_LG)
        .padding(theme::SPACING_XL)
        .into(),
        Message::HideDeleteConfirm,
    )
}
