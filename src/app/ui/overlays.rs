use std::borrow::Cow;

use iced::{
    alignment,
    widget::{
        column, container, opaque, row, scrollable, text, Button, Column, Container, MouseArea,
        Row, Space,
    },
    Element, Length, Rectangle,
};

use crate::{
    app::{
        interaction::{ContextMenuFocus, CtxAction, SubmenuKind},
        EditTrackField,
    },
    icons,
    providers::{ProviderId, ProviderTrack},
    theme::{AppTheme, Palette},
};

use super::{
    shared_components::{disabled_text_input_row, text_input_row, thumbnail},
    styles::{
        bg_overlay, bg_popup, button_style_danger, button_style_popup_item, button_style_primary,
        context_menu_item_style, fg_accent, fg_secondary, scroll_padding,
    },
    theme, ContextMenuState, Message, MusicPlayer,
};

fn provider_row<'a>(
    player: &'a MusicPlayer,
    provider: ProviderId,
    pt: &'a ProviderTrack,
    source: ProviderId,
) -> Element<'a, Message, AppTheme> {
    let album = pt
        .album
        .as_ref()
        .map(|a| a.name.clone())
        .unwrap_or_default();

    let header = Row::with_children([
        text(provider.label()).into(),
        if provider == source {
            text("(current)").style(fg_secondary()).into()
        } else {
            Button::new(text("select"))
                .padding([theme::SPACING_XS, theme::SPACING_MD])
                .on_press(Message::EditTrackSelectProvider(provider))
                .into()
        },
    ])
    .spacing(theme::SPACING_SM)
    .align_y(alignment::Vertical::Center);

    let props = Column::with_children([
        disabled_text_input_row("Id", &pt.id),
        disabled_text_input_row("Url", &pt.url),
        disabled_text_input_row("Artist ID", &pt.artist_id.clone().unwrap_or_default()),
        disabled_text_input_row("Duration (in seconds)", &pt.duration.to_string()),
        Row::with_children([
            thumbnail(
                &player.app_theme.palette,
                theme::PLAYBAR_THUMBNAIL_SIZE,
                player.thumbnail_index.get(&pt.id),
            ),
            disabled_text_input_row("Thumbnail", &pt.thumbnail),
        ])
        .spacing(theme::SPACING_SM)
        .into(),
        disabled_text_input_row("Album", &album),
    ])
    .spacing(theme::SPACING_XS);

    Column::with_children([header.into(), props.into()])
        .spacing(theme::SPACING_SM)
        .into()
}

pub(super) fn view_drop_indicator(rect: Rectangle) -> Element<'static, Message, AppTheme> {
    column![
        Space::new().height(rect.y),
        row![
            Space::new().width(rect.x),
            Container::new(
                Space::new()
                    .width(rect.width)
                    .height(crate::theme::DROP_LINE_HEIGHT),
            )
            .style(|theme: &AppTheme| container::Style {
                background: Some(theme.palette.accent.into()),
                ..Default::default()
            })
        ]
    ]
    .into()
}

#[allow(clippy::too_many_lines)]
pub(super) fn view_context_menu(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let p = &player.app_theme.palette;
    let menu = player.context_menu.as_ref().expect("context menu open");
    let (pos_x, pos_y) = menu.position;
    let n = menu.target_indices.len();

    // Rows shrink to content until the measured width is stable (a frame at
    // a clipped position wraps, so Fill is only safe once captures agree);
    // then they fill the captured panel width.
    let row_len = player
        .bounds
        .context_menu
        .as_ref()
        .filter(|g| g.stable)
        .map_or(Length::Shrink, |_| Length::Fill);
    let items: Vec<Element<'_, Message, AppTheme>> = menu
        .actions()
        .iter()
        .enumerate()
        .map(|(i, action)| {
            let (label, icon) = action_label(*action, n);
            let focused = menu.hovered == Some(ContextMenuFocus::Item(i));
            let chevron = action.submenu().is_some();
            menu_item(
                label,
                icon,
                i,
                focused,
                chevron,
                action.to_message(menu),
                p,
                row_len,
            )
        })
        .collect();

    // Width measured on the first frames; both panels share it once stable
    // so buttons can fill the row and the submenu can flip side without
    // remeasuring.
    let panel_width = player
        .bounds
        .context_menu
        .as_ref()
        .filter(|g| g.stable)
        .map_or(Length::Shrink, |g| Length::Fixed(g.panel.width));

    let mut row_children: Vec<Element<'_, Message, AppTheme>> = vec![Container::new(
        Column::with_children(items)
            .spacing(2)
            .padding(theme::SPACING_SM),
    )
    .id(super::CONTEXT_MENU_PANEL_ID)
    .width(panel_width)
    .style(bg_popup())
    .into()];

    let mut anchor_x = pos_x;

    if let Some(kind) = menu.open_submenu_kind() {
        let entries = submenu_entries(kind, menu, p, row_len);
        // Skip until the capture task has delivered geometry, so the submenu
        // doesn't render at the panel top and jump once bounds arrive.
        let geo = player
            .bounds
            .context_menu
            .as_ref()
            .filter(|_| !entries.is_empty());
        if let Some(geo) = geo {
            let parent_index = match menu.hovered {
                Some(ContextMenuFocus::Item(i)) => i,
                Some(ContextMenuFocus::Sub(..)) => menu
                    .actions()
                    .iter()
                    .position(|a| a.submenu() == Some(kind))
                    .unwrap_or(0),
                None => 0,
            };
            let submenu_h = entries.len() as f32 * geo.row_height;
            let max_offset =
                (player.window_size.height - pos_y - submenu_h - theme::SPACING_SM).max(0.0);
            let offset = geo
                .row_offsets
                .get(parent_index)
                .copied()
                .unwrap_or(0.0)
                .clamp(0.0, max_offset);
            let submenu_left =
                pos_x + 2.0 * geo.panel.width + theme::SPACING_XS > player.window_size.width;
            let submenu: Element<'_, Message, AppTheme> = Column::with_children([
                Space::new().height(offset).into(),
                Container::new(
                    Column::with_children(entries)
                        .spacing(2)
                        .padding(theme::SPACING_SM),
                )
                .width(panel_width)
                .style(bg_popup())
                .into(),
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

    let panels =
        MouseArea::new(Row::with_children(row_children)).on_exit(Message::ContextMenuHover(None));

    let overlay = Container::new(pos_absolute(opaque(panels), anchor_x.max(0.0), pos_y))
        .width(Length::Fill)
        .height(Length::Fill);

    opaque(MouseArea::new(overlay).on_press(Message::CloseContextMenu))
}

/// Label and icon shown for a main-menu entry (`n` = selected track count).
fn action_label<'a>(action: CtxAction, n: usize) -> (Cow<'a, str>, &'static [u8]) {
    match action {
        CtxAction::Play => (Cow::Borrowed("Play"), icons::PLAY_ICON),
        CtxAction::Edit => (Cow::Borrowed("Edit"), icons::EDIT_ICON),
        CtxAction::GoToArtist => (Cow::Borrowed("Go to artist"), icons::ARTIST_ICON),
        CtxAction::AddToPlaylist => (
            if n > 1 {
                Cow::Owned(format!("Add {n} tracks to Playlist"))
            } else {
                Cow::Borrowed("Add to Playlist")
            },
            icons::FOLDER_ICON,
        ),
        CtxAction::Download => (
            if n > 1 {
                Cow::Owned(format!("Download {n} tracks"))
            } else {
                Cow::Borrowed("Download")
            },
            icons::DOWNLOAD_ICON,
        ),
        CtxAction::SongRadio => (Cow::Borrowed("Song Radio"), icons::RADIO_ICON),
        CtxAction::ArtistRadio => (Cow::Borrowed("Artist Radio"), icons::RADIO_ICON),
        CtxAction::RemoveFromQueue => (
            if n > 1 {
                Cow::Owned(format!("Remove {n} tracks from queue"))
            } else {
                Cow::Borrowed("Remove from Queue")
            },
            icons::DELETE_ICON,
        ),
        CtxAction::RemoveFromPlaylist => (
            if n > 1 {
                Cow::Owned(format!("Remove {n} tracks from playlist"))
            } else {
                Cow::Borrowed("Remove from Playlist")
            },
            icons::DELETE_ICON,
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
    p: &'a Palette,
    row_len: Length,
) -> Element<'a, Message, AppTheme> {
    let mut children = vec![
        icons::icon(icon, p.fg_muted, theme::ICON_SIZE_SM).into(),
        text(label).into(),
    ];
    if chevron {
        children.push(Space::new().width(row_len).into());
        children.push(
            icons::icon(
                icons::CHEVRON_RIGHT_ICON,
                p.fg_secondary,
                theme::ICON_SIZE_SM,
            )
            .into(),
        );
    }
    let item = context_menu_item(children, focused, row_len);

    context_menu_button(
        item.id(super::CONTEXT_MENU_ROW_ID).width(row_len),
        on_press,
        ContextMenuFocus::Item(index),
    )
}

fn context_menu_item<'a>(
    children: impl IntoIterator<Item = Element<'a, Message, AppTheme>>,
    focused: bool,
    row_len: Length,
) -> Container<'a, Message, AppTheme> {
    Container::new(
        Row::with_children(children)
            .spacing(theme::SPACING_SM)
            .padding([theme::SPACING_XS, theme::SPACING_SM])
            .align_y(alignment::Vertical::Center)
            .width(row_len),
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
    p: &'a Palette,
    row_len: Length,
) -> Vec<Element<'a, Message, AppTheme>> {
    kind.providers()
        .into_iter()
        .enumerate()
        .map(|(i, provider)| {
            let focused = menu.hovered == Some(ContextMenuFocus::Sub(kind, i));
            // `(base label, icon, whether the track carries an id here,
            // whether a missing id falls back to search)`.
            let (base, icon, has_id, search_fallback) = match kind {
                SubmenuKind::Play => (
                    "Play via",
                    icons::PLAY_ICON,
                    menu.track.has_provider(provider),
                    true,
                ),
                SubmenuKind::Download => (
                    "Download from",
                    icons::DOWNLOAD_ICON,
                    menu.track.has_provider(provider),
                    true,
                ),
                SubmenuKind::SongRadio | SubmenuKind::ArtistRadio => {
                    ("Via", icons::RADIO_ICON, true, false)
                }
                SubmenuKind::GoToArtist => (
                    "On",
                    icons::ARTIST_ICON,
                    menu.track.provider_artist_id(provider).is_some(),
                    true,
                ),
            };
            let by_search = search_fallback && !has_id;
            let label = if by_search {
                Cow::Owned(format!("{} {} (search)", base, provider.label()))
            } else {
                Cow::Owned(format!("{} {}", base, provider.label()))
            };
            let icon = if by_search { icons::SEARCH_ICON } else { icon };
            let message = kind.entry_message(provider, menu);
            let item = context_menu_item(
                [
                    icons::icon(icon, p.fg_muted, theme::ICON_SIZE_SM).into(),
                    text(label).into(),
                ],
                focused,
                row_len,
            );

            context_menu_button(
                item.id(super::CONTEXT_MENU_ROW_ID).width(row_len),
                message,
                ContextMenuFocus::Sub(kind, i),
            )
        })
        .collect()
}

pub(super) fn view_edit_track(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let edit = player.edit_track.as_ref().expect("edit_track present");
    // Providers the track carries an id for, in a stable order.
    let mut provider_ids: Vec<ProviderId> = edit.original.providers.keys().copied().collect();
    provider_ids.sort_by_key(|p| p.label());

    let provider_rows: Vec<Element<'_, Message, AppTheme>> = provider_ids
        .iter()
        .map(|&provider| {
            provider_row(
                player,
                provider,
                &edit.original.providers[&provider],
                edit.source,
            )
        })
        .collect();

    let providers_block = Column::with_children([
        text("Providers")
            .style(fg_accent())
            .size(theme::TEXT_SIZE_LG)
            .into(),
        if provider_rows.is_empty() {
            text("This track has no provider data.")
                .style(fg_secondary())
                .into()
        } else {
            Column::with_children(provider_rows)
                .spacing(theme::SPACING_SM)
                .into()
        },
    ])
    .spacing(theme::SPACING_SM)
    .into();

    let save_btn = Button::new(Container::new(text("Save")).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .style(button_style_primary())
        .on_press(Message::SaveEditTrack);

    let cancel_btn = Button::new(Container::new(text("Cancel")).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .on_press(Message::CloseEditTrack);

    let body = Column::with_children([
        Column::with_children([
            text_input_row("Title", &edit.title, "Track title", |v| {
                Message::EditTrackField(EditTrackField::Title, v)
            }),
            text_input_row("Artist", &edit.artist, "Track artist", |v| {
                Message::EditTrackField(EditTrackField::Artist, v)
            }),
        ])
        .spacing(theme::SPACING_SM)
        .into(),
        providers_block,
    ])
    .spacing(theme::SPACING_MD)
    .padding(scroll_padding());

    let dialog = Column::with_children([
        text("Edit Track").size(theme::TEXT_SIZE_LG).into(),
        scrollable(body).height(Length::Fill).into(),
        Row::with_children([cancel_btn.into(), save_btn.into()])
            .spacing(theme::SPACING_SM)
            .align_y(alignment::Vertical::Center)
            .into(),
    ])
    .spacing(theme::SPACING_MD)
    .padding(theme::SPACING_MD)
    .width(theme::DIALOG_WIDTH * 2.5)
    .height(player.window_size.height * 0.7);

    view_dialog(dialog.into(), Message::CloseEditTrack)
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

fn view_dialog(
    dialog: Element<'_, Message, AppTheme>,
    close_msg: Message,
) -> Element<'_, Message, AppTheme> {
    let dialog = opaque(Container::new(dialog).style(bg_popup()));

    let backdrop = MouseArea::new(Container::new(dialog).center(Length::Fill)).on_press(close_msg);

    Container::new(opaque(backdrop)).style(bg_overlay()).into()
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
