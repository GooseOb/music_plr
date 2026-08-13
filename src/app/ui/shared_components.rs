use iced::{widget::Button, Color};

use crate::{
    app::Message,
    icons,
    theme::{self, AppTheme, Palette},
};

use super::styles::{button_style_primary, button_style_scope};

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
