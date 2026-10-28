//! The track and card context menus: anchoring, panel geometry, rows, and the
//! per-entry label/icon tables. Both share the chrome below, so only their
//! entry lists differ.

use std::borrow::Cow;

use iced::{
    alignment,
    widget::{opaque, text, Column, Container, MouseArea, Row, Space},
    Element, Length,
};

use super::{pos_absolute, theme, Message, MusicPlayer};
use crate::{
    app::{
        interaction::{
            CardAction, CardMenuState, ContextMenuFocus, ContextMenuState, CtxAction, MenuAnchor,
            SubmenuKind,
        },
        ui::{
            styles::{
                bg_popup, context_menu_item_style, icon_style_fg_muted, icon_style_fg_secondary,
            },
            CONTEXT_MENU_PANEL_ID, CONTEXT_MENU_ROW_ID,
        },
    },
    icons,
    providers::ProviderId,
    theme::AppTheme,
};

/// Length the rows of the open menu take while its geometry is still settling:
/// they shrink to content until the measured width is stable (a frame at a
/// clipped position wraps, so Fill is only safe once captures agree), then
/// they fill the captured panel width.
fn menu_row_len(player: &MusicPlayer) -> Length {
    player
        .bounds
        .context_menu
        .as_ref()
        .filter(|g| g.stable)
        .map_or(Length::Shrink, |_| Length::Fill)
}

/// Width of a menu panel: measured on the first frames, then shared by the
/// panel and its submenu so rows fill it and the submenu can flip side without
/// remeasuring.
fn menu_panel_width(player: &MusicPlayer) -> Length {
    player
        .bounds
        .context_menu
        .as_ref()
        .filter(|g| g.stable)
        .map_or(Length::Shrink, |g| Length::Fixed(g.panel.width))
}

/// One menu panel: rows in display order inside the measured popup container.
/// Untagged, so only [`main_menu_panel`] (whose rect drives the window-edge
/// flip) carries the capture id — a submenu sharing it would overwrite the main
/// panel's measured bounds.
fn menu_panel<'a>(
    player: &MusicPlayer,
    rows: Vec<Element<'a, Message, AppTheme>>,
) -> Container<'a, Message, AppTheme> {
    Container::new(
        Column::with_children(rows)
            .spacing(theme::SPACING_XXS)
            .padding(theme::SPACING_SM),
    )
    .width(menu_panel_width(player))
    .style(bg_popup())
}

/// The menu's main panel, tagged so `CaptureContextMenu` records its rect
/// alongside the row tops.
fn main_menu_panel<'a>(
    player: &MusicPlayer,
    rows: Vec<Element<'a, Message, AppTheme>>,
) -> Element<'a, Message, AppTheme> {
    menu_panel(player, rows).id(CONTEXT_MENU_PANEL_ID).into()
}

/// Place `panels` at the anchor's (possibly flipped) position under a
/// full-window backdrop that closes the menu on press. Shared by the track and
/// card context menus.
fn anchored_menu<'a>(
    anchor: &MenuAnchor,
    panels: Element<'a, Message, AppTheme>,
) -> Element<'a, Message, AppTheme> {
    let (pos_x, pos_y) = anchor.position;
    let panels = MouseArea::new(panels).on_exit(Message::ContextMenuHover(None));
    let overlay = pos_absolute(opaque(panels), pos_x.max(0.0), pos_y)
        .width(Length::Fill)
        .height(Length::Fill);

    opaque(MouseArea::new(overlay).on_press(Message::CloseContextMenu))
}

#[allow(clippy::too_many_lines)]
pub(in crate::app::ui) fn view_context_menu<'a>(
    player: &'a MusicPlayer,
    menu: &'a ContextMenuState,
) -> Element<'a, Message, AppTheme> {
    let pos_x = menu.anchor.position.0;
    let n = menu.target_indices.len();
    let row_len = menu_row_len(player);

    let actions = menu.actions(&player.stream_cache);
    let items: Vec<Element<'_, Message, AppTheme>> = actions
        .iter()
        .enumerate()
        .map(|(i, action)| {
            let (label, icon) = action_label(*action, n, player.strings);
            let focused = menu.anchor.hovered == Some(ContextMenuFocus::Item(i));
            let chevron = action.submenu().is_some();
            menu_item(
                label,
                icon,
                i,
                focused,
                chevron,
                action.to_message(menu),
                row_len,
            )
        })
        .collect();

    let mut row_children: Vec<Element<'_, Message, AppTheme>> =
        vec![main_menu_panel(player, items)];
    let mut anchor_x = pos_x;

    if let Some(kind) = menu.open_submenu_kind(&player.stream_cache) {
        let entries = submenu_entries(kind, menu, &player.stream_cache, row_len, player.strings);
        // Skip until the capture task has delivered geometry, so the submenu
        // doesn't render at the panel top and jump once bounds arrive.
        let geo = player
            .bounds
            .context_menu
            .as_ref()
            .filter(|_| !entries.is_empty());
        if let Some(geo) = geo {
            let parent_index = match menu.anchor.hovered {
                Some(ContextMenuFocus::Item(i)) => i,
                Some(ContextMenuFocus::Sub(..)) => actions
                    .iter()
                    .position(|a| a.submenu() == Some(kind))
                    .unwrap_or(0),
                None => 0,
            };

            let offset = geo.row_offsets.get(parent_index).copied().unwrap_or(0.0);
            let submenu_left =
                pos_x + 2.0 * geo.panel.width + theme::SPACING_XS > player.window_size.width;
            let submenu: Element<'_, Message, AppTheme> = Column::with_children([
                Space::new().height(offset).into(),
                menu_panel(player, entries).into(),
            ])
            .into();
            let spacer: Element<'_, Message, AppTheme> =
                Space::new().width(theme::SPACING_XS).into();
            if submenu_left {
                // Keep the main panel anchored at `pos_x`; open the submenu
                // to its left instead of pushing the panel right.
                anchor_x -= geo.panel.width + theme::SPACING_XS;
                row_children.insert(0, submenu);
                row_children.insert(1, spacer);
            } else {
                row_children.push(spacer);
                row_children.push(submenu);
            }
        }
    }

    let mut anchor = menu.anchor.clone();
    anchor.position = (anchor_x, menu.anchor.position.1);
    anchored_menu(&anchor, Row::with_children(row_children).into())
}

/// The card context menu: the flat "Open / Open in browser / Save as playlist"
/// list for the right-clicked artist, album or playlist card.
pub(super) fn view_card_menu<'a>(
    player: &'a MusicPlayer,
    menu: &'a CardMenuState,
) -> Element<'a, Message, AppTheme> {
    let row_len = menu_row_len(player);
    let saved = menu.saved(&player.library);
    let rows: Vec<Element<'_, Message, AppTheme>> = menu
        .actions()
        .iter()
        .enumerate()
        .map(|(i, action)| {
            let focused = menu.anchor.hovered == Some(ContextMenuFocus::Item(i));
            let (label, icon) = card_action(*action, saved, player.strings);
            menu_item(label, icon, i, focused, false, action.to_message(), row_len)
        })
        .collect();

    anchored_menu(&menu.anchor, main_menu_panel(player, rows))
}

/// Label and icon shown for a card-menu entry. `saved` decides how the library
/// entry reads ("Add" vs "Remove").
fn card_action(
    action: CardAction,
    saved: bool,
    tr: &crate::i18n::Strings,
) -> (Cow<'static, str>, &'static [u8]) {
    let label = match action {
        CardAction::Open => Cow::Borrowed(tr.ctx_open),
        CardAction::OpenInBrowser => Cow::Borrowed(tr.ctx_open_in_browser),
        CardAction::SaveAsPlaylist => Cow::Borrowed(tr.save_as_playlist),
        CardAction::ToggleLibrary if saved => Cow::Borrowed(tr.ctx_remove_from_library),
        CardAction::ToggleLibrary => Cow::Borrowed(tr.ctx_add_to_library),
    };
    let icon = match action {
        CardAction::Open => icons::FOLDER_ICON,
        CardAction::OpenInBrowser => icons::EXTERNAL_LINK_ICON,
        CardAction::SaveAsPlaylist => icons::QUEUE_ICON,
        CardAction::ToggleLibrary if saved => icons::BOOKMARK_ICON,
        CardAction::ToggleLibrary => icons::ADD_ICON,
    };
    (label, icon)
}

/// Label and icon shown for a main-menu entry (`n` = selected track count).
fn action_label(
    action: CtxAction,
    n: usize,
    tr: &crate::i18n::Strings,
) -> (Cow<'_, str>, &'static [u8]) {
    match action {
        CtxAction::Play => (Cow::Borrowed(tr.ctx_play), icons::PLAY_ICON),
        CtxAction::Edit => (Cow::Borrowed(tr.ctx_edit), icons::EDIT_ICON),
        CtxAction::GoToArtist => (Cow::Borrowed(tr.ctx_go_to_artist), icons::ARTIST_ICON),
        CtxAction::AddToQueue => (
            if n > 1 {
                Cow::Owned((tr.ctx_add_to_queue_n)(n))
            } else {
                Cow::Borrowed(tr.ctx_add_to_queue)
            },
            icons::QUEUE_ICON,
        ),
        CtxAction::AddToPlaylist => (
            if n > 1 {
                Cow::Owned((tr.ctx_add_to_playlist_n)(n))
            } else {
                Cow::Borrowed(tr.ctx_add_to_playlist)
            },
            icons::FOLDER_ICON,
        ),
        CtxAction::Download => (
            if n > 1 {
                Cow::Owned((tr.ctx_download_n)(n))
            } else {
                Cow::Borrowed(tr.ctx_download)
            },
            icons::DOWNLOAD_ICON,
        ),
        CtxAction::SongRadio => (Cow::Borrowed(tr.song_radio), icons::RADIO_ICON),
        CtxAction::ArtistRadio => (Cow::Borrowed(tr.artist_radio), icons::RADIO_ICON),
        CtxAction::RemoveFromQueue => (
            if n > 1 {
                Cow::Owned((tr.ctx_remove_from_queue_n)(n))
            } else {
                Cow::Borrowed(tr.ctx_remove_from_queue)
            },
            icons::DELETE_ICON,
        ),
        CtxAction::RemoveFromPlaylist => (
            if n > 1 {
                Cow::Owned((tr.ctx_remove_from_playlist_n)(n))
            } else {
                Cow::Borrowed(tr.ctx_remove_from_playlist)
            },
            icons::DELETE_ICON,
        ),
        CtxAction::RemoveFromRecent => (
            if n > 1 {
                Cow::Owned((tr.ctx_remove_from_recent_n)(n))
            } else {
                Cow::Borrowed(tr.ctx_remove_from_recent)
            },
            icons::DELETE_ICON,
        ),
        CtxAction::AddToTrashbin => (
            if n > 1 {
                Cow::Owned((tr.ctx_add_to_trashbin_n)(n))
            } else {
                Cow::Borrowed(tr.ctx_add_to_trashbin)
            },
            icons::DELETE_ICON,
        ),
        CtxAction::RemoveFromTrashbin => (
            if n > 1 {
                Cow::Owned((tr.ctx_remove_from_trashbin_n)(n))
            } else {
                Cow::Borrowed(tr.ctx_remove_from_trashbin)
            },
            icons::DELETE_ICON,
        ),
        CtxAction::ClearCache => (Cow::Borrowed(tr.ctx_clear_cache), icons::CACHE_ICON),
        CtxAction::OpenInBrowser => (
            Cow::Borrowed(tr.ctx_open_in_browser),
            icons::EXTERNAL_LINK_ICON,
        ),
    }
}

/// A context-menu row; `chevron` marks a submenu parent (clicking it still
/// triggers the default action). Mouse hover feeds keyboard focus.
#[allow(clippy::too_many_arguments)]
fn menu_item<'a>(
    label: Cow<'a, str>,
    icon: &'static [u8],
    index: usize,
    focused: bool,
    chevron: bool,
    on_press: Message,
    row_len: Length,
) -> Element<'a, Message, AppTheme> {
    let mut children = vec![
        icons::icon(icon, theme::ICON_SIZE_SM)
            .style(icon_style_fg_muted())
            .into(),
        text(label).width(Length::Fill).into(),
    ];
    if chevron {
        children.push(
            icons::icon(icons::CHEVRON_RIGHT_ICON, theme::ICON_SIZE_SM)
                .style(icon_style_fg_secondary())
                .into(),
        );
    }
    let item = context_menu_item(children, focused);

    context_menu_button(
        item.id(CONTEXT_MENU_ROW_ID).width(row_len),
        on_press,
        ContextMenuFocus::Item(index),
    )
}

fn context_menu_item<'a>(
    children: impl IntoIterator<Item = Element<'a, Message, AppTheme>>,
    focused: bool,
) -> Container<'a, Message, AppTheme> {
    Container::new(
        Row::with_children(children)
            .spacing(theme::SPACING_SM)
            .padding([theme::SPACING_XS, theme::SPACING_SM])
            .align_y(alignment::Vertical::Center),
    )
    .style(context_menu_item_style(focused))
}

fn context_menu_button<'a>(
    item: impl Into<Element<'a, Message, AppTheme>>,
    on_press: Message,
    target: ContextMenuFocus,
) -> Element<'a, Message, AppTheme> {
    MouseArea::new(item.into())
        .interaction(iced::mouse::Interaction::Pointer)
        .on_press(on_press)
        .on_enter(Message::ContextMenuHover(Some(target)))
        .into()
}

#[allow(clippy::too_many_lines)]
fn submenu_entries<'a>(
    kind: SubmenuKind,
    menu: &'a ContextMenuState,
    cache: &crate::data::cache::StreamCache,
    row_len: Length,
    tr: &'a crate::i18n::Strings,
) -> Vec<Element<'a, Message, AppTheme>> {
    menu.submenu_providers(kind, cache)
        .into_iter()
        .enumerate()
        .map(|(i, provider)| {
            let focused = menu.anchor.hovered == Some(ContextMenuFocus::Sub(kind, i));
            // `(base label, icon, whether the track carries an id here,
            // whether a missing id falls back to search)`.
            let (base, icon, has_id, search_fallback) = match kind {
                SubmenuKind::Play => (
                    tr.sub_play_via,
                    icons::PLAY_ICON,
                    if provider == ProviderId::Local {
                        menu.track.local_path().is_some()
                    } else {
                        menu.track.has_provider(provider)
                    },
                    true,
                ),
                SubmenuKind::Download => (
                    tr.sub_download_from,
                    icons::DOWNLOAD_ICON,
                    menu.track.has_provider(provider),
                    true,
                ),
                SubmenuKind::SongRadio | SubmenuKind::ArtistRadio => {
                    (tr.sub_via, icons::RADIO_ICON, true, false)
                }
                SubmenuKind::GoToArtist => (
                    tr.sub_on,
                    icons::ARTIST_ICON,
                    menu.track.provider_artist_id(provider).is_some(),
                    true,
                ),
                SubmenuKind::ClearCache => {
                    (tr.sub_clear_cache_from, icons::CACHE_ICON, true, false)
                }
            };
            let by_search = search_fallback && !has_id;
            let label = if provider == ProviderId::Local {
                tr.ctx_play_local.to_string()
            } else if by_search {
                (tr.sub_menu_label)(base, provider.label(), tr.via_search_suffix)
            } else {
                (tr.sub_menu_label)(base, provider.label(), "")
                    .trim_end()
                    .to_string()
            };
            let icon = if by_search { icons::SEARCH_ICON } else { icon };
            let message = kind.entry_message(provider, menu);
            let item = context_menu_item(
                [
                    icons::icon(icon, theme::ICON_SIZE_SM)
                        .style(icon_style_fg_muted())
                        .into(),
                    text(label).into(),
                ],
                focused,
            );

            context_menu_button(
                item.id(CONTEXT_MENU_ROW_ID).width(row_len),
                message,
                ContextMenuFocus::Sub(kind, i),
            )
        })
        .collect()
}
