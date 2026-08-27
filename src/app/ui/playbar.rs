use iced::{
    alignment,
    widget::{slider, text, Button, Column, Container, Row},
    Element, Length,
};

use super::{
    shared_components::{play_pause_button, subtitle_artist, thumbnail},
    styles::{
        bg_tertiary, button_style_playbar, fg_secondary, icon_fg, icon_fg_muted, icon_fg_secondary,
    },
    theme, Message, MusicPlayer,
};
use crate::{app::ui::styles::icon_playbar_button, icons, theme::AppTheme, util::format_duration};

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
    let track = player.queue.current();
    let title = track.map_or(player.strings.not_playing, |t| t.title.as_str());
    let artist = track.map_or("", |t| t.artist.as_str());

    let track_thumb: Element<'a, Message, AppTheme> = if let Some(t) = track {
        let thumb = player.thumbnail_index.get(t.primary_id());

        thumbnail(theme::PLAYBAR_THUMBNAIL_SIZE, thumb)
    } else {
        icons::icon(icons::MUSIC_ICON, theme::PLAYBAR_THUMBNAIL_SIZE)
            .style(icon_fg_muted())
            .into()
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
            Button::new(icons::icon(icons::SKIP_BACK_ICON, theme::ICON_SIZE_MD).style(icon_fg()))
                .padding(theme::SPACING_2XS)
                .on_press(Message::PreviousTrack)
                .into(),
            play_pause_button(player.is_playing)
                .padding(theme::SPACING_SM)
                .on_press(Message::TogglePlayPause)
                .into(),
            Button::new(
                icons::icon(icons::SKIP_FORWARD_ICON, theme::ICON_SIZE_MD).style(icon_fg()),
            )
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

    let queue_btn = Button::new(
        icons::icon(icons::QUEUE_ICON, theme::ICON_SIZE_MD)
            .style(icon_playbar_button(player.show_queue)),
    )
    .padding(theme::SPACING_XS)
    .style(button_style_playbar(player.show_queue))
    .on_press(Message::ToggleQueue)
    .width(theme::QUEUE_BTN_WIDTH)
    .height(theme::QUEUE_BTN_WIDTH);

    let repeat_btn = Button::new(
        icons::icon(icons::REPEAT_ICON, theme::ICON_SIZE_MD)
            .style(icon_playbar_button(player.repeat)),
    )
    .padding(theme::SPACING_XS)
    .style(button_style_playbar(player.repeat))
    .on_press(Message::ToggleRepeat)
    .width(theme::QUEUE_BTN_WIDTH)
    .height(theme::QUEUE_BTN_WIDTH);

    let lyrics_btn = Button::new(
        icons::icon(icons::LYRICS_ICON, theme::ICON_SIZE_MD)
            .style(icon_playbar_button(player.lyrics.is_some())),
    )
    .padding(theme::SPACING_XS)
    .style(button_style_playbar(player.lyrics.is_some()))
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
            icons::icon(icons::VOLUME_ICON, theme::ICON_SIZE_SM)
                .style(icon_fg_secondary())
                .into(),
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
