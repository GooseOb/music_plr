//! The "Import playlist" popup: source format, its settings, then the file or
//! folder to import.

use iced::{
    alignment,
    widget::{text, text_input, Button, Column},
    Element,
};

use super::{theme, view_dialog, Message, MusicPlayer};
use crate::{
    app::{
        import::ImportPlaylistDialog,
        ui::{
            shared_components::{dialog_button, scope_tab_row, text_button, text_input_row},
            styles::{button_style_primary, fg_secondary, icon_style_fg_muted},
        },
        CsvPreset, ImportCsvField, ImportMethod,
    },
    icons,
    theme::AppTheme,
};

/// The "Import playlist" popup: pick a source format, fill in its settings,
/// then select the file/folder to import.
#[allow(clippy::too_many_lines)]
pub(super) fn view_import_playlist<'a>(
    player: &'a MusicPlayer,
    dialog: &'a ImportPlaylistDialog,
) -> Element<'a, Message, AppTheme> {
    let tr = player.strings;

    let method_row = scope_tab_row(
        tr.direction,
        [
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
        ],
    );

    let content: Element<'_, Message, AppTheme> = match dialog.method {
        ImportMethod::Native => text(tr.import_native_hint).style(fg_secondary()).into(),
        ImportMethod::Csv => {
            let preset_row = text(tr.import_csv_preset).into();
            let preset_tabs = scope_tab_row(
                tr.direction,
                [
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
                ],
            );
            let mut csv_children: Vec<Element<'_, Message, AppTheme>> = vec![
                tr.direction
                    .row([preset_row, preset_tabs])
                    .spacing(theme::SPACING_SM)
                    .align_y(alignment::Vertical::Center)
                    .into(),
                text_input_row(
                    tr.direction,
                    tr.import_csv_name_col,
                    &dialog.csv_name_col,
                    tr.import_csv_name_col,
                    |v| Message::ImportCsvColChanged(ImportCsvField::Name, v),
                ),
                text_input_row(
                    tr.direction,
                    tr.import_csv_artist_col,
                    &dialog.csv_artist_col,
                    tr.import_csv_artist_col,
                    |v| Message::ImportCsvColChanged(ImportCsvField::Artist, v),
                ),
                text_input_row(
                    tr.direction,
                    tr.import_csv_album_col,
                    &dialog.csv_album_col,
                    tr.import_csv_album_col,
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
                .align_x(tr.direction.align())
                .into()
        }
        ImportMethod::FileList => {
            let pattern_rows = dialog.patterns.iter().enumerate().map(|(i, pat)| {
                let input = text_input(tr.import_pattern_lbl, pat)
                    .on_input(move |v| Message::ImportPatternChanged(i, v))
                    .align_x(tr.direction.align())
                    .padding(theme::SPACING_SM)
                    .into();
                let remove = Button::new(
                    icons::icon(icons::DELETE_ICON, theme::ICON_SIZE_SM)
                        .style(icon_style_fg_muted()),
                )
                .padding(theme::SPACING_XS)
                .on_press(Message::ImportRemovePattern(i))
                .into();
                tr.direction
                    .row([input, remove])
                    .spacing(theme::SPACING_SM)
                    .align_y(alignment::Vertical::Center)
                    .into()
            });
            let add = text_button(tr.import_add_pattern)
                .on_press(Message::ImportAddPattern)
                .into();
            Column::with_children([
                Column::with_children(pattern_rows)
                    .spacing(theme::SPACING_XS)
                    .align_x(tr.direction.align())
                    .into(),
                add,
            ])
            .spacing(theme::SPACING_SM)
            .align_x(tr.direction.align())
            .into()
        }
    };

    let mut children: Vec<Element<'_, Message, AppTheme>> = vec![
        text(tr.import_playlist).size(theme::TEXT_SIZE_LG).into(),
        method_row,
    ];
    if matches!(dialog.method, ImportMethod::Csv | ImportMethod::FileList) {
        children.push(text_input_row(
            tr.direction,
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
    let select_btn = dialog_button(select_label)
        .style(button_style_primary())
        .on_press_maybe(if dialog.can_select() {
            Some(Message::ImportSelectFiles)
        } else {
            None
        });
    let cancel_btn = dialog_button(tr.cancel).on_press(Message::CloseDialog);

    children.push(
        tr.direction
            .row([cancel_btn.into(), select_btn.into()])
            .spacing(theme::SPACING_SM)
            .align_y(alignment::Vertical::Center)
            .into(),
    );

    let dialog_col = Column::with_children(children)
        .spacing(theme::SPACING_MD)
        .padding(theme::SPACING_MD)
        .align_x(tr.direction.align())
        .width(theme::DIALOG_WIDTH_LG);

    view_dialog(dialog_col.into(), Message::CloseDialog)
}
