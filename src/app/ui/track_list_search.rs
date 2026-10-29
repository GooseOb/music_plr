use iced::{
    alignment,
    widget::{text, text_input, Container, Id},
    Element,
};

use super::{
    shared_components::icon_button,
    styles::{fg_secondary, icon_style_fg},
    theme, Message, MusicPlayer,
};
use crate::{app::TrackListSearch, icons, theme::AppTheme};

pub const TRACK_LIST_SEARCH_ID: Id = Id::new("track_list_search_input");

pub(super) fn view_track_list_search<'a>(
    player: &'a MusicPlayer,
    fs: &'a TrackListSearch,
) -> Element<'a, Message, AppTheme> {
    let count = match player.track_list_match_position() {
        Some(pos) => (player.strings.list_match_pos)(pos, fs.matches.len()),
        None => (player.strings.list_match_pos)(0, fs.matches.len()),
    };

    let dir = player.strings.direction;
    let input = text_input(player.strings.find_in_list, &fs.query)
        .id(TRACK_LIST_SEARCH_ID)
        .on_input(Message::TrackListSearchInput)
        .align_x(dir.align())
        .padding([theme::SPACING_XS, theme::SPACING_SM]);

    let prev = icon_button(icons::CHEVRON_UP_ICON, theme::ICON_SIZE_MD, icon_style_fg())
        .on_press(Message::TrackListSearchPrev);
    let next = icon_button(
        icons::CHEVRON_DOWN_ICON,
        theme::ICON_SIZE_MD,
        icon_style_fg(),
    )
    .on_press(Message::TrackListSearchNext);

    let close = icon_button(icons::CLOSE_ICON, theme::ICON_SIZE_MD, icon_style_fg())
        .on_press(Message::TrackListSearchClose);

    let bar = dir
        .row([
            close.into(),
            input.into(),
            text(count).style(fg_secondary()).into(),
            prev.into(),
            next.into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center);

    Container::new(bar)
        .padding([theme::SPACING_SM, theme::SPACING_MD])
        .into()
}
