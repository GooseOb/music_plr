use super::{theme, Message, MusicPlayer};
use crate::theme::AppTheme;
use iced::{
    alignment,
    widget::{text, text_input, Button, Container, Id, Row},
    Element,
};

pub const TRACK_LIST_SEARCH_ID: Id = Id::new("track_list_search_input");

pub(super) fn view_track_list_search(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let fs = player
        .track_list_search
        .as_ref()
        .expect("view_track_list_search called without a floating search");
    let p = &player.app_theme.palette;

    let count = match player.track_list_match_position() {
        Some(pos) => format!("{}/{}", pos, fs.matches.len()),
        None => format!("0/{}", fs.matches.len()),
    };

    let input = text_input(player.strings.find_in_list, &fs.query)
        .id(TRACK_LIST_SEARCH_ID)
        .on_input(Message::TrackListSearchInput)
        .padding([theme::SPACING_XS, theme::SPACING_SM]);

    let prev = icon_button(
        crate::icons::CHEVRON_UP_ICON,
        Message::TrackListSearchPrev,
        p,
    );
    let next = icon_button(
        crate::icons::CHEVRON_DOWN_ICON,
        Message::TrackListSearchNext,
        p,
    );

    let close = icon_button(crate::icons::CLOSE_ICON, Message::TrackListSearchClose, p);

    let bar = Row::with_children([
        close.into(),
        input.into(),
        text(count).color(p.fg_secondary).into(),
        prev.into(),
        next.into(),
    ])
    .spacing(theme::SPACING_SM)
    .align_y(alignment::Vertical::Center);

    Container::new(bar)
        .padding([theme::SPACING_SM, theme::SPACING_MD])
        .into()
}

fn icon_button(
    icon_data: &'static [u8],
    on_press: Message,
    p: &crate::theme::Palette,
) -> Button<'static, Message, AppTheme> {
    Button::new(crate::icons::icon(icon_data, p.fg, theme::ICON_SIZE_MD))
        .padding(theme::SPACING_XS)
        .on_press(on_press)
}
