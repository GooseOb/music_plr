//! The AI-translate lyrics popup: target language, prompt, and the Cancel/Hide
//! pair shown while a request is in flight.

use iced::{
    widget::{text, text_input, Column},
    Element, Length,
};

use super::{theme, view_dialog, Message, MusicPlayer};
use crate::{
    app::ui::{
        shared_components::{dialog_button, dialog_button_row},
        styles::{button_style_primary, fg_secondary},
    },
    theme::AppTheme,
};

pub(super) fn view_translate_dialog<'a>(
    player: &'a MusicPlayer,
    dialog: &'a crate::app::TranslateDialog,
) -> Element<'a, Message, AppTheme> {
    let tr = player.strings;
    let translating = player.translation_in_flight(dialog.pane);
    let language = text_input(tr.translate_language_lbl, &dialog.language)
        .on_input(Message::TranslateLanguageChanged)
        .on_submit(Message::TranslateLanguageSubmit)
        .align_x(tr.direction.align())
        .padding(theme::SPACING_SM)
        .into();
    let prompt = iced::widget::text_editor(&dialog.prompt)
        .on_action(Message::TranslatePromptAction)
        .padding(theme::SPACING_SM)
        .height(Length::Fixed(180.0))
        .into();

    let cancel_btn = dialog_button(tr.cancel);
    let button_row = if translating {
        dialog_button_row(
            tr.direction,
            [
                cancel_btn.on_press(Message::TranslateCancel).into(),
                dialog_button(tr.translate_hide)
                    .style(button_style_primary())
                    .on_press(Message::TranslateHide)
                    .into(),
            ]
            .into_iter(),
        )
    } else {
        dialog_button_row(
            tr.direction,
            [
                cancel_btn.on_press(Message::CloseDialog).into(),
                dialog_button(tr.translate_reset)
                    .on_press(Message::TranslateResetPrompt)
                    .into(),
                dialog_button(tr.translate_submit)
                    .style(button_style_primary())
                    .on_press_maybe(
                        (!dialog.language.trim().is_empty()).then_some(Message::TranslateSubmit),
                    )
                    .into(),
            ]
            .into_iter(),
        )
    };

    let children = [
        text(tr.translate_title).size(theme::TEXT_SIZE_LG).into(),
        language,
        text(tr.translate_prompt_lbl)
            .size(theme::TEXT_SIZE_SM)
            .style(fg_secondary())
            .into(),
        prompt,
    ]
    .into_iter()
    .chain(translating.then(|| text(tr.translating).style(fg_secondary()).into()))
    .chain([button_row.into()]);

    view_dialog(
        Column::with_children(children)
            .spacing(theme::SPACING_MD)
            .padding(theme::SPACING_MD)
            .align_x(tr.direction.align())
            .width(theme::DIALOG_WIDTH_LG)
            .into(),
        Message::CloseDialog,
    )
}
