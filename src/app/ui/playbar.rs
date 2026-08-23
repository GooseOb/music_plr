use iced::{
    alignment,
    widget::{slider, text, Button, Column, Container, Row},
    Color, Element, Length,
};

use crate::{icons, theme::AppTheme, util::format_duration};

use super::{
    shared_components::thumbnail,
    shared_components::{play_pause_button, subtitle_artist},
    styles::{bg_tertiary, button_style_queue, fg_secondary},
    theme, Message, MusicPlayer,
};

fn time_text(time: u32) -> Element<'static, Message, AppTheme> {
    text(format_duration(time))
        .size(theme::TEXT_SIZE_XS)
        .style(fg_secondary())
        .width(theme::TIME_TEXT_WIDTH)
        .center()
        .into()
}

#[allow(clippy::too_many_lines)]
pub(super) fn view_playbar<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;

    let track = player.queue.current();
    let title = track.map_or("Not playing", |t| t.title.as_str());
    let artist = track.map_or("", |t| t.artist.as_str());

    let track_thumb: Element<'a, Message, AppTheme> = if let Some(t) = track {
        let thumb = player.thumbnail_index.get(t.primary_id());

        thumbnail(p, theme::PLAYBAR_THUMBNAIL_SIZE, thumb)
    } else {
        icons::icon(icons::MUSIC_ICON, p.fg_muted, theme::PLAYBAR_THUMBNAIL_SIZE).into()
    };

    let artist_target = track.and_then(|t| {
        t.provider_artist_id(t.source)
            .map(|id| (id.to_string(), t.source))
    });

    let artist_el = subtitle_artist(artist, theme::TEXT_SIZE_SM, artist_target);

    let track_info = Column::with_children([text(title).into(), artist_el]).spacing(2);

    let elapsed_text = time_text((player.progress * player.duration) as u32);
    let total_text = time_text(player.duration as u32);

    let controls = Container::new(
        Row::with_children([
            Button::new(icons::icon(
                icons::SKIP_BACK_ICON,
                p.fg,
                theme::ICON_SIZE_MD,
            ))
            .padding(theme::SPACING_2XS)
            .on_press(Message::PreviousTrack)
            .into(),
            play_pause_button(player.is_playing)
                .padding(theme::SPACING_SM)
                .on_press(Message::TogglePlayPause)
                .into(),
            Button::new(icons::icon(
                icons::SKIP_FORWARD_ICON,
                p.fg,
                theme::ICON_SIZE_MD,
            ))
            .padding(theme::SPACING_2XS)
            .on_press(Message::NextTrack)
            .into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center),
    )
    .center_x(Length::Fill);

    let progress = slider(0.0..=1.0, player.progress, Message::Seek)
        .width(Length::Fill)
        .step(0.01f32);

    let controls_and_progress =
        Column::with_children([controls.into(), progress.into()]).spacing(theme::SPACING_XS);

    let volume_slider = slider(0.0..=1.0, player.volume, Message::SetVolume)
        .width(theme::VOLUME_SLIDER_WIDTH)
        .step(0.01f32);

    let queue_btn = Button::new(icons::icon(
        icons::QUEUE_ICON,
        if player.show_queue {
            Color::BLACK
        } else {
            p.fg_secondary
        },
        theme::ICON_SIZE_MD,
    ))
    .padding(theme::SPACING_XS)
    .style(button_style_queue(player.show_queue))
    .on_press(Message::ToggleQueue)
    .width(theme::QUEUE_BTN_WIDTH)
    .height(theme::QUEUE_BTN_WIDTH);

    let repeat_btn = Button::new(icons::icon(
        icons::REPEAT_ICON,
        if player.repeat {
            Color::BLACK
        } else {
            p.fg_secondary
        },
        theme::ICON_SIZE_MD,
    ))
    .padding(theme::SPACING_XS)
    .style(button_style_queue(player.repeat))
    .on_press(Message::ToggleRepeat)
    .width(theme::QUEUE_BTN_WIDTH)
    .height(theme::QUEUE_BTN_WIDTH);

    let lyrics_btn = Button::new(icons::icon(
        icons::LYRICS_ICON,
        if player.lyrics.is_some() {
            Color::BLACK
        } else {
            p.fg_secondary
        },
        theme::ICON_SIZE_MD,
    ))
    .padding(theme::SPACING_XS)
    .style(button_style_queue(player.lyrics.is_some()))
    .on_press(Message::ShowLyrics)
    .width(theme::QUEUE_BTN_WIDTH)
    .height(theme::QUEUE_BTN_WIDTH);

    Container::new(
        Row::with_children([
            Container::new(track_thumb)
                .width(theme::PLAYBAR_THUMBNAIL_SIZE)
                .height(theme::PLAYBAR_THUMBNAIL_SIZE)
                .into(),
            Container::new(track_info)
                .width(theme::PLAYBAR_TRACK_INFO_WIDTH)
                .into(),
            Container::new(elapsed_text).into(),
            Container::new(controls_and_progress)
                .width(Length::Fill)
                .into(),
            Container::new(total_text).into(),
            icons::icon(icons::VOLUME_ICON, p.fg_secondary, theme::ICON_SIZE_SM).into(),
            volume_slider.into(),
            repeat_btn.into(),
            queue_btn.into(),
            lyrics_btn.into(),
        ])
        .spacing(theme::SPACING_MD)
        .align_y(alignment::Vertical::Center)
        .padding([theme::SPACING_LG, theme::SPACING_XL]),
    )
    .style(bg_tertiary())
    .into()
}
