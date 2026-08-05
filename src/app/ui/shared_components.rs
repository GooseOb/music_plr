use iced::{widget::Button, Color};

use crate::{
    app::{ui::styles::button_style_primary, Message},
    icons,
    theme::{self, AppTheme},
};

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
