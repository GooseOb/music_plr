//! The startup dialog for missing external dependencies.

use iced::{
    alignment,
    widget::{checkbox, scrollable, text, Column},
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

    let missing = !dialog.missing.is_empty();
    let found = !dialog.found.is_empty();
    let rows = missing
        .then(|| {
            [
                text(tr.deps_title).size(theme::TEXT_SIZE_LG).into(),
                text(tr.deps_intro).style(fg_secondary()).into(),
                scrollable(
                    Column::with_children(
                        dialog
                            .missing
                            .iter()
                            .map(|&kind| dep_checkbox_row(player, dialog, kind)),
                    )
                    .spacing(theme::SPACING_SM),
                )
                .into(),
            ]
        })
        .into_iter()
        .flatten()
        .chain(
            found
                .then(|| {
                    [
                        text(tr.deps_found_section_title)
                            .size(theme::TEXT_SIZE_LG)
                            .into(),
                        text(tr.deps_found_section_intro)
                            .style(fg_secondary())
                            .into(),
                        scrollable(
                            Column::with_children(
                                dialog
                                    .found
                                    .iter()
                                    .map(|&kind| dep_checkbox_row(player, dialog, kind)),
                            )
                            .spacing(theme::SPACING_SM),
                        )
                        .into(),
                    ]
                })
                .into_iter()
                .flatten(),
        );

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

    let children = rows.chain([dialog_button_row(
        tr.direction,
        [discard_btn.into(), install_btn.into()].into_iter(),
    )
    .into()]);

    let content = Column::with_children(children)
        .spacing(theme::SPACING_MD)
        .padding(theme::SPACING_MD)
        .align_x(tr.direction.align())
        .width(theme::DIALOG_WIDTH_LG);

    view_dialog(content.into(), Message::DepDismiss)
}

/// A checkable dependency row with its name, description, and live status.
fn dep_checkbox_row<'a>(
    player: &'a MusicPlayer,
    dialog: &'a crate::app::DependencyDialog,
    kind: DepKind,
) -> Element<'a, Message, AppTheme> {
    let tr = &player.strings;
    let checked = dialog.selected.contains(&kind);
    let op = player.dep_ops.get(&kind);

    let children = [
        tr.direction
            .row(
                [
                    checkbox(checked)
                        .on_toggle(move |_| Message::DepToggle(kind))
                        .into(),
                    text(kind.name()).style(fg_accent()).into(),
                ]
                .into_iter(),
            )
            .spacing(theme::SPACING_SM)
            .align_y(alignment::Vertical::Center)
            .into(),
        text(dep_desc(tr, kind)).style(fg_secondary()).into(),
    ]
    .into_iter()
    .chain(dep_install_status(op, tr));
    Column::with_children(children)
        .spacing(theme::SPACING_XS)
        .align_x(tr.direction.align())
        .into()
}
