//! The startup dialog for missing external dependencies.

use iced::{
    alignment,
    widget::{checkbox, scrollable, text, Column, Row, Space},
    Element,
};

use super::{theme, view_dialog, Message, MusicPlayer};
use crate::{
    app::{
        dependency_dialog::dep_desc,
        ui::{
            shared_components::{dep_install_status, dialog_button, dialog_button_row},
            styles::{button_style_primary, fg_accent, fg_secondary},
        },
    },
    deps::DepKind,
    theme::AppTheme,
};

/// Startup dialog listing missing external dependencies. Each auto-installable
/// dep is a checkbox (default-checked); the user installs the checked ones or
/// discards.
pub(super) fn view_dependency_dialog<'a>(
    player: &'a MusicPlayer,
    dialog: &'a crate::app::DependencyDialog,
) -> Element<'a, Message, AppTheme> {
    let tr = &player.strings;
    let python3_available = crate::deps::is_available(DepKind::Python3);

    let mut children: Vec<Element<'_, Message, AppTheme>> = Vec::new();

    let missing_rows: Vec<Element<'_, Message, AppTheme>> = dialog
        .missing
        .iter()
        .map(|&kind| match kind {
            DepKind::YtDlp | DepKind::Python3 => dep_checkbox_row(player, dialog, kind, None),
            DepKind::YtMusicApi if python3_available => {
                dep_checkbox_row(player, dialog, kind, None)
            }
            DepKind::YtMusicApi => dep_manual_row(player, kind, tr.deps_ytmusicapi_requires_python),
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
        let found_rows = dialog
            .found
            .iter()
            .map(|&kind| dep_checkbox_row(player, dialog, kind, None));
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

    let pending = dialog.pending(&player.dep_ops);
    let installing_any = player.dep_ops.values().any(|o| o.installing);
    let install_btn = dialog_button(tr.deps_install_selected)
        .style(button_style_primary())
        .on_press_maybe(if pending.is_empty() && !installing_any {
            None
        } else {
            Some(Message::DepInstall)
        });
    let discard_btn = dialog_button(tr.deps_discard).on_press(Message::DepDismiss);

    children.push(dialog_button_row([discard_btn.into(), install_btn.into()]).into());

    let content = Column::with_children(children)
        .spacing(theme::SPACING_MD)
        .padding(theme::SPACING_MD)
        .width(theme::DIALOG_WIDTH_LG);

    view_dialog(content.into(), Message::DepDismiss)
}

/// A checkable dependency row with its name, description, and live status.
fn dep_checkbox_row<'a>(
    player: &'a MusicPlayer,
    dialog: &'a crate::app::DependencyDialog,
    kind: DepKind,
    note: Option<&'static str>,
) -> Element<'a, Message, AppTheme> {
    let tr = &player.strings;
    let checked = dialog.selected.contains(&kind);
    let op = player.dep_ops.get(&kind);
    let status: Element<'_, Message, AppTheme> =
        dep_install_status(op, tr).unwrap_or_else(|| Space::new().into());

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
    children.push(text(dep_desc(tr, kind)).style(fg_secondary()).into());
    if let Some(note) = note {
        children.push(text(note).style(fg_secondary()).into());
    }
    children.push(status);
    Column::with_children(children)
        .spacing(theme::SPACING_XS)
        .into()
}

/// A non-installable dependency row (e.g. Python 3) shown with a manual hint.
fn dep_manual_row<'a>(
    player: &'a MusicPlayer,
    kind: DepKind,
    note: &'a str,
) -> Element<'a, Message, AppTheme> {
    Column::with_children([
        text(kind.name()).style(fg_accent()).into(),
        text(dep_desc(player.strings, kind))
            .style(fg_secondary())
            .into(),
        text(note).style(fg_secondary()).into(),
    ])
    .spacing(theme::SPACING_XS)
    .into()
}
