//! The in-app cheatsheet (?/F1).

use iced::{
    alignment,
    widget::{scrollable, text, Column},
    Element, Length,
};

use super::{theme, view_dialog, Message, MusicPlayer};
use crate::{
    app::ui::{
        shared_components::dialog_button,
        styles::{fg_accent, fg_secondary},
    },
    theme::AppTheme,
};

pub(super) fn view_shortcuts(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let tr = player.strings;
    let mut sections: Vec<Element<'_, Message, AppTheme>> = Vec::new();
    for section in crate::app::shortcuts::SECTIONS {
        let mut children: Vec<Element<'_, Message, AppTheme>> = vec![text((section.title)(tr))
            .size(theme::TEXT_SIZE_SM)
            .style(fg_accent())
            .into()];
        for row in section.rows {
            children.push(
                tr.direction
                    .row([
                        text(row.keys)
                            .size(theme::TEXT_SIZE_SM)
                            .style(fg_secondary())
                            .width(theme::DIALOG_WIDTH)
                            .into(),
                        text((row.action)(tr))
                            .size(theme::TEXT_SIZE_SM)
                            .width(Length::Fill)
                            .into(),
                    ])
                    .spacing(theme::SPACING_SM)
                    .into(),
            );
        }
        sections.push(
            Column::with_children(children)
                .spacing(theme::SPACING_XS)
                .align_x(tr.direction.align())
                .into(),
        );
    }

    let close_btn = dialog_button(player.strings.cancel).on_press(Message::CloseDialog);

    view_dialog(
        Column::with_children([
            text(tr.sc_title).size(theme::TEXT_SIZE_LG).into(),
            scrollable(Column::with_children(sections).spacing(theme::SPACING_MD))
                .height(Length::Fill)
                .into(),
            close_btn.into(),
        ])
        .align_x(alignment::Horizontal::Center)
        .spacing(theme::SPACING_SM)
        .width(theme::DIALOG_WIDTH_LG)
        .height(player.window_size.height * 0.7)
        .padding(theme::SPACING_MD)
        .into(),
        Message::CloseDialog,
    )
}
