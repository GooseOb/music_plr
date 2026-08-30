use std::borrow::Cow;

use iced::{
    alignment,
    widget::{
        checkbox, column, container, opaque, scrollable, text, text_input, Button, Column,
        Container, MouseArea, Row, Space,
    },
    Element, Length, Rectangle,
};

use super::{
    shared_components::{disabled_text_input_row, scope_tab_row, text_input_row, thumbnail},
    styles::{
        bg_overlay, bg_popup, bg_secondary, button_style_danger, button_style_popup_item,
        button_style_primary, context_menu_item_style, fg_accent, fg_secondary, icon_fg_muted,
        icon_fg_secondary, scroll_padding,
    },
    theme, ContextMenuState, Message, MusicPlayer,
};
use crate::{
    app::{
        interaction::{ContextMenuFocus, CtxAction, SubmenuKind},
        CsvPreset, EditTrackField, ImportCsvField, ImportMethod,
    },
    deps::DepKind,
    icons,
    providers::{ProviderId, ProviderTrack},
    theme::AppTheme,
};

fn provider_row<'a>(
    player: &'a MusicPlayer,
    provider: ProviderId,
    pt: Option<&'a ProviderTrack>,
    source: ProviderId,
    finding: bool,
) -> Element<'a, Message, AppTheme> {
    let header: Element<'a, Message, AppTheme> = match pt {
        Some(_pt) => Row::with_children([
            text(provider.label()).into(),
            if provider == source {
                text(player.strings.current).style(fg_secondary()).into()
            } else {
                Button::new(text(player.strings.select))
                    .padding([theme::SPACING_XS, theme::SPACING_MD])
                    .on_press(Message::EditTrackSelectProvider(provider))
                    .into()
            },
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .into(),
        None if provider.capabilities().search => Row::with_children([
            text(provider.label()).into(),
            if finding {
                Button::new(text(player.strings.finding))
                    .padding([theme::SPACING_XS, theme::SPACING_MD])
                    .into()
            } else {
                Button::new(text(player.strings.find))
                    .padding([theme::SPACING_XS, theme::SPACING_MD])
                    .on_press(Message::EditTrackFindProvider(provider))
                    .into()
            },
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .into(),
        None => text(provider.label()).into(),
    };

    let body: Element<'a, Message, AppTheme> = match pt {
        Some(pt) => {
            let album = pt
                .album
                .as_ref()
                .map(|a| a.name.clone())
                .unwrap_or_default();

            Column::with_children([
                disabled_text_input_row(player.strings.lbl_id, &pt.id),
                disabled_text_input_row(player.strings.lbl_url, &pt.url),
                disabled_text_input_row(
                    player.strings.lbl_artist_id,
                    &pt.artist_id.clone().unwrap_or_default(),
                ),
                disabled_text_input_row(player.strings.lbl_duration_secs, &pt.duration.to_string()),
                Row::with_children([
                    thumbnail(
                        theme::PLAYBAR_THUMBNAIL_SIZE,
                        player.thumbnail_index.get(&pt.id),
                    ),
                    disabled_text_input_row(player.strings.lbl_thumbnail, &pt.thumbnail),
                ])
                .spacing(theme::SPACING_SM)
                .into(),
                disabled_text_input_row(player.strings.lbl_album, &album),
            ])
            .spacing(theme::SPACING_XS)
            .into()
        }
        None => text(player.strings.not_linked).style(fg_secondary()).into(),
    };

    Column::with_children([header, body])
        .spacing(theme::SPACING_SM)
        .into()
}

pub(super) fn view_drop_indicator(rect: Rectangle) -> Element<'static, Message, AppTheme> {
    pos_absolute(
        Container::new(
            Space::new()
                .width(rect.width)
                .height(crate::theme::DROP_LINE_HEIGHT),
        )
        .style(|theme: &AppTheme| container::Style {
            background: Some(theme.palette.accent.into()),
            ..Default::default()
        })
        .into(),
        rect.x,
        rect.y,
    )
    .into()
}

#[allow(clippy::too_many_lines)]
pub(super) fn view_context_menu<'a>(
    player: &'a MusicPlayer,
    menu: &'a ContextMenuState,
) -> Element<'a, Message, AppTheme> {
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
            let (label, icon) = action_label(*action, n, player.strings);
            let focused = menu.hovered == Some(ContextMenuFocus::Item(i));
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
        let entries = submenu_entries(kind, menu, row_len, player.strings);
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

            let offset = geo.row_offsets.get(parent_index).copied().unwrap_or(0.0);
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

    let overlay = pos_absolute(opaque(panels), anchor_x.max(0.0), pos_y)
        .width(Length::Fill)
        .height(Length::Fill);

    opaque(MouseArea::new(overlay).on_press(Message::CloseContextMenu))
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
        CtxAction::SongRadio => (Cow::Borrowed(tr.ctx_song_radio), icons::RADIO_ICON),
        CtxAction::ArtistRadio => (Cow::Borrowed(tr.ctx_artist_radio), icons::RADIO_ICON),
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
            .style(icon_fg_muted())
            .into(),
        text(label).width(Length::Fill).into(),
    ];
    if chevron {
        children.push(
            icons::icon(icons::CHEVRON_RIGHT_ICON, theme::ICON_SIZE_SM)
                .style(icon_fg_secondary())
                .into(),
        );
    }
    let item = context_menu_item(children, focused);

    context_menu_button(
        item.id(super::CONTEXT_MENU_ROW_ID).width(row_len),
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
    row_len: Length,
    tr: &'a crate::i18n::Strings,
) -> Vec<Element<'a, Message, AppTheme>> {
    kind.providers(&menu.track)
        .into_iter()
        .enumerate()
        .map(|(i, provider)| {
            let focused = menu.hovered == Some(ContextMenuFocus::Sub(kind, i));
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
            };
            let by_search = search_fallback && !has_id;
            let label = if provider == ProviderId::Local {
                tr.ctx_play_local.to_string()
            } else if by_search {
                format!("{} {} {}", base, provider.label(), tr.via_search_suffix)
            } else {
                format!("{} {}", base, provider.label())
            };
            let icon = if by_search { icons::SEARCH_ICON } else { icon };
            let message = kind.entry_message(provider, menu);
            let item = context_menu_item(
                [
                    icons::icon(icon, theme::ICON_SIZE_SM)
                        .style(icon_fg_muted())
                        .into(),
                    text(label).into(),
                ],
                focused,
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
    // List every searchable provider plus any extra provider the track
    // already carries an identity for (e.g. Local for imported files), so
    // unresolved providers show a "Find" action next to them.
    let mut provider_ids: Vec<ProviderId> = ProviderId::searchable().to_vec();
    for key in edit.original.providers.keys() {
        if !provider_ids.contains(key) {
            provider_ids.push(*key);
        }
    }
    provider_ids.sort_by_key(|p| p.label());

    let provider_rows: Vec<Element<'_, Message, AppTheme>> = provider_ids
        .iter()
        .map(|&provider| {
            provider_row(
                player,
                provider,
                edit.original.providers.get(&provider),
                edit.source,
                edit.finding == Some(provider),
            )
        })
        .collect();

    let providers_block = Column::with_children([
        text(player.strings.providers)
            .style(fg_accent())
            .size(theme::TEXT_SIZE_LG)
            .into(),
        if provider_rows.is_empty() {
            text(player.strings.no_provider_data)
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

    let save_btn = Button::new(Container::new(text(player.strings.save)).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .style(button_style_primary())
        .on_press(Message::SaveEditTrack);

    let cancel_btn =
        Button::new(Container::new(text(player.strings.cancel)).center_x(Length::Fill))
            .padding(theme::SPACING_SM)
            .on_press(Message::CloseEditTrack);

    let body = Column::with_children([
        Column::with_children([
            text_input_row(
                player.strings.lbl_title,
                &edit.title,
                player.strings.ph_track_title,
                |v| Message::EditTrackField(EditTrackField::Title, v),
            ),
            text_input_row(
                player.strings.lbl_artist,
                &edit.artist,
                player.strings.ph_track_artist,
                |v| Message::EditTrackField(EditTrackField::Artist, v),
            ),
        ])
        .spacing(theme::SPACING_SM)
        .into(),
        providers_block,
    ])
    .spacing(theme::SPACING_MD)
    .padding(scroll_padding());

    let dialog = Column::with_children([
        text(player.strings.edit_track)
            .size(theme::TEXT_SIZE_LG)
            .into(),
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
        Container::new(text(player.strings.cancel).size(theme::TEXT_SIZE_SM))
            .center_x(Length::Fill),
    )
    .padding(theme::SPACING_SM)
    .on_press(Message::ClosePicker);

    view_dialog(
        Column::with_children([
            text(player.strings.ctx_add_to_playlist)
                .size(theme::TEXT_SIZE_LG)
                .into(),
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

/// Startup dialog listing missing external dependencies. Each auto-installable
/// dep is a checkbox (default-checked); the user installs the checked ones or
/// discards. `Python3` (when missing) is shown as a manual step with no box.
pub(super) fn view_dependency_dialog(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let dialog = player.dep_dialog.as_ref().expect("dep_dialog present");
    let tr = &player.strings;
    let python3_missing = dialog.missing.contains(&DepKind::Python3);

    let mut children: Vec<Element<'_, Message, AppTheme>> = Vec::new();

    let missing_rows: Vec<Element<'_, Message, AppTheme>> = dialog
        .missing
        .iter()
        .map(|&kind| match kind {
            DepKind::YtDlp => dep_checkbox_row(player, kind, None),
            DepKind::YtMusicApi if !python3_missing => dep_checkbox_row(player, kind, None),
            DepKind::YtMusicApi => dep_manual_row(player, kind, tr.deps_ytmusicapi_requires_python),
            DepKind::Python3 => dep_manual_row(player, kind, tr.deps_python3_manual),
        })
        .collect();
    if !missing_rows.is_empty() {
        children.push(text(tr.deps_title).size(theme::TEXT_SIZE_LG).into());
        children.push(text(tr.deps_intro).style(fg_secondary()).into());
        children.push(
            scrollable(Column::with_children(missing_rows).spacing(theme::SPACING_SM)).into(),
        );
    }

    if !dialog.found.is_empty() {
        let found_rows = dialog.found.iter().map(|&kind| match kind {
            DepKind::YtDlp | DepKind::YtMusicApi => dep_checkbox_row(player, kind, None),
            DepKind::Python3 => dep_manual_row(player, kind, ""),
        });
        children.push(
            text(tr.deps_found_section_title)
                .size(theme::TEXT_SIZE_LG)
                .into(),
        );
        children.push(
            text(tr.deps_found_section_intro)
                .style(fg_secondary())
                .into(),
        );
        children
            .push(scrollable(Column::with_children(found_rows).spacing(theme::SPACING_SM)).into());
    }

    let pending = dialog.pending();
    let install_btn =
        Button::new(Container::new(text(tr.deps_install_selected)).center_x(Length::Fill))
            .padding(theme::SPACING_SM)
            .style(button_style_primary())
            .on_press_maybe(if pending.is_empty() && dialog.installing.is_empty() {
                None
            } else {
                Some(Message::DepInstall)
            });
    let discard_btn = Button::new(Container::new(text(tr.deps_discard)).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .on_press(Message::DepDismiss);

    children.push(
        Row::with_children([discard_btn.into(), install_btn.into()])
            .spacing(theme::SPACING_SM)
            .into(),
    );

    let content = Column::with_children(children)
        .spacing(theme::SPACING_MD)
        .padding(theme::SPACING_MD)
        .width(theme::DIALOG_WIDTH * 2.0);

    view_dialog(content.into(), Message::DepDismiss)
}

/// A checkable dependency row with its name, description, and live status.
fn dep_checkbox_row<'a>(
    player: &'a MusicPlayer,
    kind: DepKind,
    note: Option<&'static str>,
) -> Element<'a, Message, AppTheme> {
    let tr = &player.strings;
    let dialog = player.dep_dialog.as_ref();
    let checked = dialog.is_some_and(|d| d.selected.contains(&kind));
    let status: Element<'_, Message, AppTheme> =
        if dialog.is_some_and(|d| d.installing.contains(&kind)) {
            if let Some((downloaded, total)) = dialog.and_then(|d| d.progress.get(&kind)) {
                if *total > 0 {
                    let pct = ((*downloaded as f64 / *total as f64) * 100.0) as u16;
                    let bar = Container::new(iced::widget::ProgressBar::new(
                        std::ops::RangeInclusive::new(0.0, *total as f32),
                        *downloaded as f32,
                    ))
                    .height(8)
                    .style(bg_secondary());
                    Column::with_children([
                        bar.into(),
                        text(format!("{pct}%"))
                            .size(theme::TEXT_SIZE_XS)
                            .style(fg_secondary())
                            .into(),
                    ])
                    .spacing(theme::SPACING_XS)
                    .into()
                } else {
                    text(tr.deps_installing).style(fg_secondary()).into()
                }
            } else {
                text(tr.deps_installing).style(fg_secondary()).into()
            }
        } else if dialog.is_some_and(|d| d.done.contains(&kind)) {
            text(tr.deps_installed).style(fg_accent()).into()
        } else if let Some(err) = dialog.and_then(|d| d.errors.get(&kind)) {
            text(format!("{}: {}", tr.deps_failed, err))
                .style(fg_secondary())
                .into()
        } else {
            Space::new().into()
        };

    let mut children: Vec<Element<'_, Message, AppTheme>> = Vec::with_capacity(4);
    children.push(
        Row::with_children([
            checkbox(checked)
                .on_toggle(move |_| Message::DepToggle(kind))
                .into(),
            text(kind.name()).style(fg_accent()).into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center)
        .into(),
    );
    children.push(text(dep_description(tr, kind)).style(fg_secondary()).into());
    if let Some(note) = note {
        children.push(text(note).style(fg_secondary()).into());
    }
    children.push(status);
    Column::with_children(children)
        .spacing(theme::SPACING_XS)
        .into()
}

fn dep_description(tr: &crate::i18n::Strings, kind: DepKind) -> &'static str {
    match kind {
        DepKind::YtDlp => tr.deps_yt_dlp_desc,
        DepKind::YtMusicApi => tr.deps_ytmusicapi_desc,
        DepKind::Python3 => tr.deps_python3_desc,
    }
}

/// A non-installable dependency row (e.g. Python 3) shown with a manual hint.
fn dep_manual_row<'a>(
    player: &'a MusicPlayer,
    kind: DepKind,
    note: &'a str,
) -> Element<'a, Message, AppTheme> {
    Column::with_children([
        text(kind.name()).style(fg_accent()).into(),
        text(dep_description(&player.strings, kind))
            .style(fg_secondary())
            .into(),
        text(note).style(fg_secondary()).into(),
    ])
    .spacing(theme::SPACING_XS)
    .into()
}

pub(super) fn view_delete_confirm(
    strings: &crate::i18n::Strings,
) -> Element<'_, Message, AppTheme> {
    let cancel_btn = Button::new(Container::new(text(strings.cancel)).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .on_press(Message::HideDeleteConfirm);

    let delete_btn = Button::new(Container::new(text(strings.delete)).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .style(button_style_danger())
        .on_press(Message::ConfirmDeletePlaylist);

    view_dialog(
        Column::with_children([
            text(strings.delete_playlist_q)
                .size(theme::TEXT_SIZE_LG)
                .into(),
            text(strings.tracks_wont_be_deleted)
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

/// The "Import playlist" popup: pick a source format, fill in its settings,
/// then select the file/folder to import.
#[allow(clippy::too_many_lines)]
pub(super) fn view_import_playlist(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let dialog = player
        .import_dialog
        .as_ref()
        .expect("import_dialog present");
    let tr = player.strings;

    let method_row = scope_tab_row([
        (
            tr.import_method_native,
            dialog.method == ImportMethod::Native,
            Message::ImportMethodChanged(ImportMethod::Native),
        ),
        (
            tr.import_method_filelist,
            dialog.method == ImportMethod::FileList,
            Message::ImportMethodChanged(ImportMethod::FileList),
        ),
        (
            tr.import_method_csv,
            dialog.method == ImportMethod::Csv,
            Message::ImportMethodChanged(ImportMethod::Csv),
        ),
    ]);

    let content: Element<'_, Message, AppTheme> = match dialog.method {
        ImportMethod::Native => text(tr.import_native_hint).style(fg_secondary()).into(),
        ImportMethod::Csv => {
            let preset_row = text(tr.import_csv_preset).into();
            let preset_tabs = scope_tab_row([
                (
                    tr.import_csv_preset_default,
                    dialog.csv_preset == CsvPreset::Default,
                    Message::ImportCsvPresetChanged(CsvPreset::Default),
                ),
                (
                    tr.import_csv_preset_exportify,
                    dialog.csv_preset == CsvPreset::Exportify,
                    Message::ImportCsvPresetChanged(CsvPreset::Exportify),
                ),
            ]);
            let mut csv_children: Vec<Element<'_, Message, AppTheme>> = vec![
                Row::with_children([preset_row, preset_tabs])
                    .spacing(theme::SPACING_SM)
                    .align_y(alignment::Vertical::Center)
                    .into(),
                text_input_row(tr.import_csv_name_col, &dialog.csv_name_col, "name", |v| {
                    Message::ImportCsvColChanged(ImportCsvField::Name, v)
                }),
                text_input_row(
                    tr.import_csv_artist_col,
                    &dialog.csv_artist_col,
                    "artist",
                    |v| Message::ImportCsvColChanged(ImportCsvField::Artist, v),
                ),
                text_input_row(
                    tr.import_csv_album_col,
                    &dialog.csv_album_col,
                    "album",
                    |v| Message::ImportCsvColChanged(ImportCsvField::Album, v),
                ),
            ];
            if dialog.csv_preset == CsvPreset::Exportify {
                csv_children.push(
                    text(tr.import_csv_exportify_note)
                        .style(fg_secondary())
                        .into(),
                );
            }
            Column::with_children(csv_children)
                .spacing(theme::SPACING_SM)
                .into()
        }
        ImportMethod::FileList => {
            let pattern_rows: Vec<Element<'_, Message, AppTheme>> = dialog
                .patterns
                .iter()
                .enumerate()
                .map(|(i, pat)| {
                    let input = text_input(tr.import_pattern_lbl, pat)
                        .on_input(move |v| Message::ImportPatternChanged(i, v))
                        .padding(theme::SPACING_SM)
                        .into();
                    let remove = Button::new(
                        icons::icon(icons::DELETE_ICON, theme::ICON_SIZE_SM).style(icon_fg_muted()),
                    )
                    .padding(theme::SPACING_XS)
                    .on_press(Message::ImportRemovePattern(i))
                    .into();
                    Row::with_children([input, remove])
                        .spacing(theme::SPACING_SM)
                        .align_y(alignment::Vertical::Center)
                        .into()
                })
                .collect();
            let add = Button::new(text(tr.import_add_pattern))
                .padding([theme::SPACING_XS, theme::SPACING_MD])
                .on_press(Message::ImportAddPattern)
                .into();
            Column::with_children([
                Column::with_children(pattern_rows)
                    .spacing(theme::SPACING_XS)
                    .into(),
                add,
            ])
            .spacing(theme::SPACING_SM)
            .into()
        }
    };

    let mut children: Vec<Element<'_, Message, AppTheme>> = vec![
        text(tr.import_playlist).size(theme::TEXT_SIZE_LG).into(),
        method_row,
    ];
    if matches!(dialog.method, ImportMethod::Csv | ImportMethod::FileList) {
        children.push(text_input_row(
            tr.import_playlist_name,
            &dialog.playlist_name,
            "",
            Message::ImportPlaylistNameChanged,
        ));
    }
    children.push(content);
    if let Some((a, b)) = dialog.conflict_pair() {
        children.push(
            text((tr.import_pattern_conflict)(&a, &b))
                .style(fg_secondary())
                .into(),
        );
    }

    let select_label = match dialog.method {
        ImportMethod::FileList => tr.import_select_folder,
        ImportMethod::Native | ImportMethod::Csv => tr.import_select_file,
    };
    let select_btn = Button::new(Container::new(text(select_label)).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .style(button_style_primary())
        .on_press_maybe(if dialog.can_select() {
            Some(Message::ImportSelectFiles)
        } else {
            None
        });
    let cancel_btn = Button::new(Container::new(text(tr.cancel)).center_x(Length::Fill))
        .padding(theme::SPACING_SM)
        .on_press(Message::CloseImportPlaylist);

    children.push(
        Row::with_children([cancel_btn.into(), select_btn.into()])
            .spacing(theme::SPACING_SM)
            .align_y(alignment::Vertical::Center)
            .into(),
    );

    let dialog_col = Column::with_children(children)
        .spacing(theme::SPACING_MD)
        .padding(theme::SPACING_MD)
        .width(theme::DIALOG_WIDTH * 2.0);

    view_dialog(dialog_col.into(), Message::CloseImportPlaylist)
}
