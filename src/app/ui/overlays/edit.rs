//! The Edit Track popup: the working copy's fields and provider rows, plus the
//! manual-search step that replaces the popup's body while it is open.

use iced::{
    alignment,
    widget::{scrollable, text, text_input, Column},
    Element, Length,
};

use super::{theme, view_dialog, Message, MusicPlayer};
use crate::{
    app::{
        pane::MANUAL_SEARCH_PANE_ID,
        ui::{
            shared_components::{
                dialog_button, dialog_button_row, disabled_text_input_row, empty_state,
                loading_state, scope_tab_row, search_button, text_button, text_input_row,
                thumbnail,
            },
            styles::{button_style_primary, fg_accent, fg_secondary, scroll_padding},
            view_track_list,
        },
        EditTrackField,
    },
    load_state::LoadState,
    providers::{ProviderId, ProviderTrack},
    theme::AppTheme,
};

/// Opens the Edit popup's manual-search step for one provider. Offered only
/// where a search could actually answer it.
fn manual_search_button(
    player: &MusicPlayer,
    provider: ProviderId,
) -> Option<Element<'_, Message, AppTheme>> {
    provider.capabilities().search.then(|| {
        text_button(player.strings.manual_search)
            .on_press(Message::EditTrackManualSearch(provider))
            .into()
    })
}

fn provider_row<'a>(
    player: &'a MusicPlayer,
    provider: ProviderId,
    pt: Option<&'a ProviderTrack>,
    source: ProviderId,
    finding: bool,
) -> Element<'a, Message, AppTheme> {
    let dir = player.strings.direction;
    let manual_search = manual_search_button(player, provider);
    let header: Element<'a, Message, AppTheme> = match pt {
        Some(_pt) => {
            let mut children: Vec<Element<'a, Message, AppTheme>> = vec![
                text(player.strings.provider(provider)).into(),
                if provider == source {
                    text(player.strings.current).style(fg_secondary()).into()
                } else {
                    text_button(player.strings.select)
                        .on_press(Message::EditTrackSelectProvider(provider))
                        .into()
                },
            ];
            children.extend(manual_search);
            dir.row(children)
                .spacing(theme::SPACING_SM)
                .align_y(alignment::Vertical::Center)
                .into()
        }
        None if provider.capabilities().search => {
            let mut children: Vec<Element<'a, Message, AppTheme>> = vec![
                text(player.strings.provider(provider)).into(),
                if finding {
                    text_button(player.strings.finding).into()
                } else {
                    text_button(player.strings.find)
                        .on_press(Message::EditTrackFindProvider(provider))
                        .into()
                },
            ];
            children.extend(manual_search);
            dir.row(children)
                .spacing(theme::SPACING_SM)
                .align_y(alignment::Vertical::Center)
                .into()
        }
        None => text(player.strings.provider(provider)).into(),
    };

    let body: Element<'a, Message, AppTheme> = match pt {
        Some(pt) => {
            let album: &str = pt
                .album
                .as_ref()
                .map(|a| a.name.as_str())
                .unwrap_or_default();

            Column::with_children([
                disabled_text_input_row(player.strings.direction, player.strings.lbl_id, &pt.id),
                disabled_text_input_row(player.strings.direction, player.strings.lbl_url, &pt.url),
                disabled_text_input_row(
                    player.strings.direction,
                    player.strings.lbl_artist_id,
                    pt.artist_id.as_deref().unwrap_or_default(),
                ),
                disabled_text_input_row(
                    player.strings.direction,
                    player.strings.lbl_duration_secs,
                    &pt.duration.to_string(),
                ),
                dir.row([
                    thumbnail(
                        theme::PLAYBAR_THUMBNAIL_SIZE,
                        player.thumbnail_index.get(provider, &pt.id),
                    ),
                    disabled_text_input_row(
                        player.strings.direction,
                        player.strings.lbl_thumbnail,
                        &pt.thumbnail,
                    ),
                ])
                .spacing(theme::SPACING_SM)
                .into(),
                disabled_text_input_row(player.strings.direction, player.strings.lbl_album, album),
            ])
            .spacing(theme::SPACING_XS)
            .align_x(player.strings.direction.align())
            .into()
        }
        None => text(player.strings.not_linked).style(fg_secondary()).into(),
    };

    Column::with_children([header, body])
        .spacing(theme::SPACING_SM)
        .align_x(player.strings.direction.align())
        .into()
}

pub(super) fn view_edit_track<'a>(
    player: &'a MusicPlayer,
    edit: &'a crate::app::EditTrackState,
) -> Element<'a, Message, AppTheme> {
    // The manual-search step takes over the popup while it is open; the
    // working copy waits underneath it.
    if let Some(manual) = &edit.manual {
        return view_edit_manual_search(player, manual);
    }

    // List every searchable provider plus any extra provider the track
    // already carries an identity for (e.g. Local for imported files), so
    // unresolved providers show a "Find" action next to them.
    let mut provider_ids: Vec<ProviderId> = ProviderId::searchable().to_vec();
    for key in edit.original.providers.keys() {
        if !provider_ids.contains(key) {
            provider_ids.push(*key);
        }
    }
    provider_ids.sort_by_key(|p| p.name());

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
                .align_x(player.strings.direction.align())
                .into()
        },
    ])
    .spacing(theme::SPACING_SM)
    .align_x(player.strings.direction.align())
    .into();

    let save_btn = dialog_button(player.strings.save)
        .style(button_style_primary())
        .on_press(Message::SaveEditTrack);

    let cancel_btn = dialog_button(player.strings.cancel).on_press(Message::CloseDialog);

    let body = Column::with_children([
        Column::with_children([
            text_input_row(
                player.strings.direction,
                player.strings.lbl_title,
                &edit.title,
                player.strings.ph_track_title,
                |v| Message::EditTrackField(EditTrackField::Title, v),
            ),
            text_input_row(
                player.strings.direction,
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
    .padding(scroll_padding())
    .align_x(player.strings.direction.align());

    let dialog = Column::with_children([
        text(player.strings.edit_track)
            .size(theme::TEXT_SIZE_LG)
            .into(),
        scrollable(body).height(Length::Fill).into(),
        dialog_button_row(
            player.strings.direction,
            [cancel_btn.into(), save_btn.into()],
        )
        .into(),
    ])
    .spacing(theme::SPACING_MD)
    .padding(theme::SPACING_MD)
    .align_x(player.strings.direction.align())
    .width(theme::DIALOG_WIDTH_XL)
    .height(player.window_size.height * 0.7);

    view_dialog(dialog.into(), Message::CloseDialog)
}

pub fn edit_manual_search_input_id() -> iced::widget::Id {
    iced::widget::Id::new("edit_manual_search_input")
}

/// The manual search's result list: ordinary track rows, so a result gets the
/// same hover, now-playing highlight, right-click menu, playback and keyboard
/// navigation every other track list has. The list is addressed by
/// [`MANUAL_SEARCH_PANE_ID`], a virtual pane resolved against the popup.
fn manual_search_results<'a>(
    player: &'a MusicPlayer,
    manual: &'a crate::app::EditManualSearch,
) -> Element<'a, Message, AppTheme> {
    let tr = player.strings;
    match &manual.results {
        LoadState::Loading => loading_state(tr.searching),
        LoadState::Failed(e) => empty_state((tr.couldnt_load)(e)),
        LoadState::Ready(tracks) if tracks.is_empty() => empty_state(tr.no_results_found),
        LoadState::Ready(tracks) => view_track_list(tracks, player, MANUAL_SEARCH_PANE_ID, 0),
    }
}

/// The Edit popup's manual-search step: search one provider, pick at most one
/// result, then confirm it onto the track being edited.
pub(super) fn view_edit_manual_search<'a>(
    player: &'a MusicPlayer,
    manual: &'a crate::app::EditManualSearch,
) -> Element<'a, Message, AppTheme> {
    let tr = player.strings;

    let placeholder = tr.search_placeholder(manual.provider);
    let query = text_input(&placeholder, &manual.query)
        .on_input(Message::EditTrackManualQuery)
        .on_submit(Message::EditTrackManualRun)
        .align_x(tr.direction.align())
        .padding([theme::SPACING_SM, theme::SPACING_MD])
        .id(edit_manual_search_input_id())
        .width(Length::Fill)
        .into();
    let search_btn = search_button(manual.can_run().then_some(Message::EditTrackManualRun));

    let scopes = scope_tab_row(
        player.strings.direction,
        manual
            .provider
            .track_scopes()
            .into_iter()
            .map(|scope| {
                (
                    player.scope_name(scope),
                    scope == manual.scope,
                    Message::EditTrackManualScope(scope),
                )
            })
            .collect::<Vec<_>>(),
    );

    let cancel_btn = dialog_button(tr.cancel).on_press(Message::EditTrackManualCancel);
    let confirm_btn = dialog_button(tr.confirm)
        .style(button_style_primary())
        .on_press_maybe(
            manual
                .selected_track()
                .map(|_| Message::EditTrackManualConfirm),
        );

    view_dialog(
        Column::with_children([
            text((tr.manual_search_on)(tr.provider(manual.provider)))
                .size(theme::TEXT_SIZE_LG)
                .into(),
            tr.direction
                .row([query, search_btn.into()])
                .spacing(theme::SPACING_SM)
                .align_y(alignment::Vertical::Center)
                .into(),
            scopes,
            manual_search_results(player, manual),
            dialog_button_row(tr.direction, [cancel_btn.into(), confirm_btn.into()]).into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_x(alignment::Horizontal::Center)
        .width(theme::DIALOG_WIDTH_XL)
        .height(player.window_size.height * 0.7)
        .padding(theme::SPACING_MD)
        .into(),
        Message::EditTrackManualCancel,
    )
}
