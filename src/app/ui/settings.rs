use iced::{
    widget::{checkbox, scrollable, text, text_input, Button, Column, Container},
    Element,
};

use crate::theme::{self, AppTheme};

use super::{Message, MusicPlayer};

fn section_header<'a>(player: &'a MusicPlayer, label: &'a str) -> Element<'a, Message, AppTheme> {
    let p = &player.app_theme.palette;
    Container::new(text(label).size(theme::TEXT_SIZE_LG).color(p.accent))
        .padding([theme::SPACING_MD, theme::SPACING_XL])
        .into()
}

fn text_input_row<'a>(
    label: &'a str,
    value: &str,
    placeholder: &'a str,
    on_input: fn(String) -> Message,
) -> Element<'a, Message, AppTheme> {
    Container::new(
        Column::with_children([
            text(label).into(),
            text_input(placeholder, value)
                .on_input(on_input)
                .padding([theme::SPACING_SM, theme::SPACING_MD])
                .into(),
        ])
        .spacing(theme::SPACING_XS),
    )
    .padding([theme::SPACING_MD, theme::SPACING_XL])
    .into()
}

pub(super) fn view_settings(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let cfg = &player.config;

    let normalize = Container::new(
        checkbox(cfg.volume_normalization)
            .label("Normalize volume across tracks")
            .on_toggle(Message::SettingsVolumeNormalizationToggled)
            .spacing(theme::SPACING_MD),
    )
    .padding([theme::SPACING_MD, theme::SPACING_XL])
    .into();

    let download_dir = text_input_row(
        "Download directory",
        &cfg.download_dir,
        "",
        Message::SettingsDownloadDirChanged,
    );

    let cache_size = text_input_row(
        "Max stream cache size (MB)",
        &format!("{}", cfg.cache_max_size_mb),
        "1024",
        Message::SettingsCacheMaxSizeChanged,
    );

    let hist_visible = text_input_row(
        "Search history rows shown",
        &format!("{}", cfg.max_search_history_visible),
        "10",
        Message::SettingsMaxHistoryVisibleChanged,
    );

    let hist_stored = text_input_row(
        "Search history entries kept",
        &format!("{}", cfg.max_search_history_stored),
        "100",
        Message::SettingsMaxHistoryStoredChanged,
    );

    let recent = text_input_row(
        "Recently played tracks kept",
        &format!("{}", cfg.max_recently_played),
        "50",
        Message::SettingsMaxRecentlyPlayedChanged,
    );

    let content = Column::with_children([
        section_header(player, "Playback"),
        normalize,
        section_header(player, "Storage"),
        download_dir,
        cache_size,
        section_header(player, "History"),
        hist_visible,
        hist_stored,
        recent,
        Container::new(
            Button::new(text("Reset to defaults"))
                .padding([theme::SPACING_SM, theme::SPACING_MD])
                .on_press(Message::SettingsResetDefaults),
        )
        .padding([theme::SPACING_MD, theme::SPACING_XL])
        .into(),
    ])
    .spacing(theme::SPACING_XS);

    scrollable(content)
        .id(iced::widget::Id::new("settings_scroll"))
        .into()
}
