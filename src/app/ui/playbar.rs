use iced::{
    alignment,
    widget::{slider, text, Column, Container, Row, Space},
    Element, Length,
};

use super::{
    progress_bar::ProgressBar,
    shared_components::{
        icon_button, like_button, play_pause_button, playbar_toggle_button, subtitle_artist,
        thumbnail,
    },
    styles::{
        bg_tertiary, fg_secondary, icon_style_fg, icon_style_fg_muted, icon_style_fg_secondary,
    },
    theme, Message, MusicPlayer,
};
use crate::{icons, theme::AppTheme, util::format_duration};

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
        let thumb = player.thumbnail_index.get(t.source, t.primary_id());

        thumbnail(theme::PLAYBAR_THUMBNAIL_SIZE, thumb)
    } else {
        icons::icon(icons::MUSIC_ICON, theme::PLAYBAR_THUMBNAIL_SIZE)
            .style(icon_style_fg_muted())
            .into()
    };

    let artist_target = track.and_then(|t| {
        t.provider_artist_id(t.source)
            .map(|id| (id.to_string(), t.source))
    });

    let artist_el = subtitle_artist(
        player.focused_pane_id,
        artist,
        theme::TEXT_SIZE_SM,
        artist_target,
    );

    let track_info =
        Column::with_children([text(title).into(), artist_el]).spacing(theme::SPACING_XXS);

    let elapsed_text = time_text((player.playback_progress() * player.duration) as u32);
    let total_text = time_text(player.duration as u32);

    let controls = Container::new(
        Row::with_children([
            icon_button(icons::SKIP_BACK_ICON, theme::ICON_SIZE_MD, icon_style_fg())
                .padding(theme::SPACING_2XS)
                .on_press(Message::PreviousTrack)
                .into(),
            play_pause_button(player.is_playing)
                .padding(theme::SPACING_SM)
                .on_press(Message::TogglePlayPause)
                .into(),
            icon_button(
                icons::SKIP_FORWARD_ICON,
                theme::ICON_SIZE_MD,
                icon_style_fg(),
            )
            .padding(theme::SPACING_2XS)
            .on_press(Message::NextTrack)
            .into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_y(alignment::Vertical::Center),
    )
    .center_x(Length::Fill);

    let progress: Element<'_, Message, AppTheme> = ProgressBar::new(
        player.progress,
        player.progress_at,
        player.duration,
        player.is_playing,
    )
    .into();

    let controls_and_progress =
        Column::with_children([controls.into(), progress]).spacing(theme::SPACING_XS);

    let volume_slider = slider(0.0..=1.0, player.volume, Message::SetVolume)
        .width(theme::VOLUME_SLIDER_WIDTH)
        .step(0.01f32);

    let queue_btn =
        playbar_toggle_button(icons::QUEUE_ICON, player.show_queue).on_press(Message::ToggleQueue);

    let repeat_btn =
        playbar_toggle_button(icons::REPEAT_ICON, player.repeat).on_press(Message::ToggleRepeat);

    let liked = player.is_current_track_liked();
    let like_btn = track.map_or_else(
        || Space::new().into(),
        |_| {
            like_button(liked)
                .on_press(Message::ToggleCurrentTrackLike)
                .width(theme::QUEUE_BTN_WIDTH)
                .height(theme::QUEUE_BTN_WIDTH)
                .into()
        },
    );

    let lyrics_open = player.focused_pane().lyrics.is_some();
    let lyrics_btn = playbar_toggle_button(icons::LYRICS_ICON, lyrics_open)
        .on_press(Message::ShowLyrics(player.focused_pane_id));

    Container::new(
        Row::with_children([
            Container::new(track_thumb)
                .width(theme::PLAYBAR_THUMBNAIL_SIZE)
                .height(theme::PLAYBAR_THUMBNAIL_SIZE)
                .into(),
            Container::new(track_info)
                .width(theme::PLAYBAR_TRACK_INFO_WIDTH)
                .into(),
            like_btn,
            Container::new(elapsed_text).into(),
            Container::new(controls_and_progress)
                .width(Length::Fill)
                .into(),
            Container::new(total_text).into(),
            icons::icon(icons::VOLUME_ICON, theme::ICON_SIZE_SM)
                .style(icon_style_fg_secondary())
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
