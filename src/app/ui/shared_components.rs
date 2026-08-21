use iced::{
    alignment,
    widget::{text, Button, Row},
    Color, Element,
};

use crate::{
    app::Message,
    icons,
    theme::{self, AppTheme, Palette},
};

use super::styles::{button_style_primary, button_style_scope};

/// Build a segmented row of scope/provider chips. Each item is
/// `(label, selected, on_press)`; `pad_x`/`pad_y` set the button padding.
pub fn scope_tab_row<I: IntoIterator<Item = (String, bool, Message)>>(
    items: I,
) -> Element<'static, Message, AppTheme> {
    let tabs = items.into_iter().map(|(label, selected, on_press)| {
        Button::new(text(label).size(theme::TEXT_SIZE_SM))
            .padding([theme::SPACING_XS, theme::SPACING_SM])
            .style(button_style_scope(selected))
            .on_press(on_press)
            .into()
    });
    Row::with_children(tabs)
        .spacing(theme::SPACING_XS)
        .align_y(alignment::Vertical::Center)
        .wrap()
        .into()
}

pub fn play_pause_button(is_playing: bool) -> Button<'static, Message, AppTheme> {
    Button::new(icons::icon(
        if is_playing {
            icons::PAUSE_ICON
        } else {
            icons::PLAY_ICON
        },
        Color::BLACK,
        theme::ICON_SIZE_LG,
    ))
    .style(button_style_primary())
}

pub fn toggle_bookmark_button(p: &Palette, is_saved: bool) -> Button<'static, Message, AppTheme> {
    Button::new(icons::icon(
        icons::BOOKMARK_ICON,
        if is_saved { Color::BLACK } else { p.fg_muted },
        theme::ICON_SIZE_SM,
    ))
    .padding(theme::SPACING_SM)
    .style(button_style_scope(is_saved))
}
