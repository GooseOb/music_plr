use iced::{
    alignment,
    widget::{checkbox, scrollable, text, Button, Column, Container},
    Element,
};

use crate::{
    app::ui::styles::fg_accent,
    providers::ProviderId,
    theme::{self, AppTheme},
};

use super::{
    shared_components::{scope_tab_row, text_input_row},
    Message, MusicPlayer,
};

fn default_provider_section(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let row = scope_tab_row(ProviderId::defaultable().iter().map(|&provider| {
        (
            provider.label().to_string(),
            player.config.default_provider == provider,
            Message::SettingsDefaultProviderChanged(provider),
        )
    }));
    Column::with_children([text("Default stream & download provider").into(), row])
        .spacing(theme::SPACING_SM)
        .align_x(alignment::Horizontal::Left)
        .into()
}

fn section<'a>(
    label: &'a str,
    children: impl IntoIterator<Item = Element<'a, Message, AppTheme>>,
) -> Element<'a, Message, AppTheme> {
    Column::with_children([
        text(label)
            .size(theme::TEXT_SIZE_LG)
            .style(fg_accent())
            .into(),
        Column::with_children(children)
            .spacing(theme::SPACING_SM)
            .into(),
    ])
    .spacing(theme::SPACING_MD)
    .into()
}

pub(super) fn view_settings(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let cfg = &player.config;

    let normalize = checkbox(cfg.volume_normalization)
        .label("Normalize volume across tracks")
        .on_toggle(Message::SettingsVolumeNormalizationToggled)
        .spacing(theme::SPACING_MD)
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
        section("Playback", [normalize, default_provider_section(player)]),
        section("Storage", [download_dir, cache_size]),
        section("History", [hist_visible, hist_stored, recent]),
        Button::new(text("Reset to defaults"))
            .padding([theme::SPACING_SM, theme::SPACING_MD])
            .on_press(Message::SettingsResetDefaults)
            .into(),
    ])
    .spacing(theme::SPACING_XL);

    scrollable(Container::new(content).padding([theme::SPACING_MD, theme::SPACING_XL]))
        .id(iced::widget::Id::new("settings_scroll"))
        .into()
}
