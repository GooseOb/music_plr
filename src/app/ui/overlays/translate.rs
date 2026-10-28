//! The AI-translate lyrics popup: target language, prompt, in-flight flag.

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
    let language = text_input(tr.translate_language_lbl, &dialog.language)
        .on_input(Message::TranslateLanguageChanged)
        .on_submit(Message::TranslateLanguageSubmit)
        .padding(theme::SPACING_SM)
        .into();
    let prompt = iced::widget::text_editor(&dialog.prompt)
        .on_action(Message::TranslatePromptAction)
        // .style(|theme: &AppTheme, _| {
        //     let p = &theme.palette;
        //     iced::widget::text_editor::Style {
        //         background: Color::TRANSPARENT.into(),
        //         border: iced::Border::default(),
        //         placeholder: Color::TRANSPARENT,
        //         value: p.fg_secondary,
        //         selection: p.accent.scale_alpha(0.4),
        //     }
        // })
        .padding(theme::SPACING_SM)
        .height(Length::Fixed(180.0))
        .into();

    let cancel_btn = dialog_button(tr.cancel).on_press(Message::CloseDialog);
    let reset_btn = dialog_button(tr.translate_reset).on_press(Message::TranslateResetPrompt);
    let submit_btn = dialog_button(if dialog.translating {
        tr.translating
    } else {
        tr.translate_submit
    })
    .style(button_style_primary())
    .on_press_maybe(
        (!dialog.translating && !dialog.language.trim().is_empty())
            .then_some(Message::TranslateSubmit),
    );

    let mut children: Vec<Element<'_, Message, AppTheme>> = vec![
        text(tr.translate_title).size(theme::TEXT_SIZE_LG).into(),
        language,
        text(tr.translate_prompt_lbl)
            .size(theme::TEXT_SIZE_SM)
            .style(fg_secondary())
            .into(),
        prompt,
    ];
    if dialog.translating {
        children.push(text(tr.translating).style(fg_secondary()).into());
    }
    children
        .push(dialog_button_row([cancel_btn.into(), reset_btn.into(), submit_btn.into()]).into());

    view_dialog(
        Column::with_children(children)
            .spacing(theme::SPACING_MD)
            .padding(theme::SPACING_MD)
            .width(theme::DIALOG_WIDTH_LG)
            .into(),
        Message::CloseDialog,
    )
}
