use iced::{
    alignment,
    widget::{slider, text, Button, Column, Container, Row},
    Color, Element, Length,
};

use crate::{icons, theme::AppTheme};

use super::{
    styles::{bg_tertiary, button_style_primary, button_style_queue},
    theme,
    track_list::thumbnail,
    Message, MusicPlayer,
};

pub(super) fn view_playbar<'a>(player: &'a MusicPlayer) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;

    let track = player.queue.current();
    let title = track.map_or("Not playing", |t| t.title.as_str());
    let artist = track.map_or("", |t| t.artist.as_str());

    let play_pause_icon = if player.is_playing {
        "pause.svg"
    } else {
        "play.svg"
    };

    let track_thumb: Element<'a, Message, AppTheme> = if let Some(t) = track {
        thumbnail(t, p, theme::PLAYBAR_THUMBNAIL_SIZE)
    } else {
        icons::icon("music.svg", p.fg_muted, theme::PLAYBAR_THUMBNAIL_SIZE).into()
    };

    let track_info = Column::with_children(vec![
        text(title).size(theme::TEXT_SIZE_DEFAULT).into(),
        text(artist)
            .size(theme::TEXT_SIZE_SM)
            .color(p.fg_secondary)
            .into(),
    ])
    .spacing(2);

    let elapsed_text = text(player.elapsed_text.clone())
        .size(theme::TEXT_SIZE_XS)
        .color(p.fg_secondary)
        .width(Length::Fixed(theme::TIME_TEXT_WIDTH))
        .center();

    let total_text = text(player.total_text.clone())
        .size(theme::TEXT_SIZE_XS)
        .color(p.fg_secondary)
        .width(Length::Fixed(theme::TIME_TEXT_WIDTH))
        .center();

    let controls = Container::new(
        Row::with_children(vec![
            Button::new(icons::icon("skip-back.svg", p.fg, theme::ICON_SIZE_MD))
                .padding(theme::SPACING_XS2)
                .on_press(Message::PreviousTrack)
                .into(),
            Button::new(icons::icon(
                play_pause_icon,
                Color::BLACK,
                theme::ICON_SIZE_LG,
            ))
            .padding(theme::SPACING_SM)
            .style(button_style_primary())
            .on_press(Message::TogglePlayPause)
            .into(),
            Button::new(icons::icon("skip-forward.svg", p.fg, theme::ICON_SIZE_MD))
                .padding(theme::SPACING_XS2)
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
        Column::with_children(vec![controls.into(), progress.into()]).spacing(theme::SPACING_XS);

    let volume_slider = slider(0.0..=1.0, player.volume, Message::SetVolume)
        .width(Length::Fixed(theme::VOLUME_SLIDER_WIDTH))
        .step(0.01f32);

    let queue_btn = Button::new(icons::icon(
        "queue.svg",
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
    .width(Length::Fixed(theme::QUEUE_BTN_WIDTH))
    .height(Length::Fixed(theme::QUEUE_BTN_WIDTH));

    Container::new(
        Row::with_children(vec![
            Container::new(track_thumb)
                .width(Length::Fixed(theme::PLAYBAR_THUMBNAIL_SIZE))
                .height(Length::Fixed(theme::PLAYBAR_THUMBNAIL_SIZE))
                .into(),
            Container::new(track_info)
                .width(Length::Fixed(theme::PLAYBAR_TRACK_INFO_WIDTH))
                .into(),
            Container::new(elapsed_text).into(),
            Container::new(controls_and_progress)
                .width(Length::Fill)
                .into(),
            Container::new(total_text).into(),
            icons::icon("volume.svg", p.fg_secondary, theme::ICON_SIZE_SM).into(),
            volume_slider.into(),
            queue_btn.into(),
        ])
        .spacing(theme::SPACING_MD)
        .align_y(alignment::Vertical::Center)
        .padding([theme::SPACING_LG, theme::SPACING_XL]),
    )
    .width(Length::Fill)
    .style(bg_tertiary())
    .into()
}
