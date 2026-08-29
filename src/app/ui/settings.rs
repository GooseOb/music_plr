use iced::{
    alignment,
    widget::{checkbox, scrollable, text, Button, Column, Container, Row, Space},
    Element, Length,
};

use super::{
    shared_components::{scope_tab_row, text_input_row},
    Message, MusicPlayer,
};
use crate::{
    app::ui::styles::{fg_accent, icon_accent},
    i18n::Language,
    icons,
    providers::ProviderId,
    theme::{self, AppTheme},
};

fn default_provider_section(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let row = scope_tab_row(ProviderId::defaultable().iter().map(|&provider| {
        (
            provider.label().to_string(),
            player.config.default_provider == provider,
            Message::SettingsDefaultProviderChanged(provider),
        )
    }));
    Column::with_children([text(player.strings.default_provider_lbl).into(), row])
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

fn language_section(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let row = scope_tab_row(Language::ALL.iter().map(|&language| {
        (
            language.label().to_string(),
            player.config.language == language,
            Message::SettingsLanguageChanged(language),
        )
    }));
    Column::with_children([text(player.strings.language_lbl).into(), row])
        .spacing(theme::SPACING_SM)
        .align_x(alignment::Horizontal::Left)
        .into()
}

fn theme_section(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let row = scope_tab_row(theme::ThemeKind::ALL.iter().map(|&kind| {
        (
            kind.label().to_string(),
            player.config.theme_kind == kind,
            Message::SettingsThemeChanged(kind),
        )
    }));
    Column::with_children([text(player.strings.theme_lbl).into(), row])
        .spacing(theme::SPACING_SM)
        .align_x(alignment::Horizontal::Left)
        .into()
}

pub(super) fn view_settings(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let cfg = &player.config;

    let footer = Container::new(
        text(crate::APP_NAME)
            .size(theme::TEXT_SIZE_XL)
            .style(fg_accent()),
    )
    .width(Length::Fill)
    .align_x(alignment::Horizontal::Center);

    let normalize = checkbox(cfg.volume_normalization)
        .label(player.strings.normalize_volume_lbl)
        .on_toggle(Message::SettingsVolumeNormalizationToggled)
        .spacing(theme::SPACING_MD)
        .into();

    let download_dir = text_input_row(
        player.strings.download_dir_lbl,
        &cfg.download_dir,
        "",
        Message::SettingsDownloadDirChanged,
    );

    let cache_size = text_input_row(
        player.strings.cache_size_lbl,
        &format!("{}", cfg.cache_max_size_mb),
        "1024",
        Message::SettingsCacheMaxSizeChanged,
    );

    let hist_visible = text_input_row(
        player.strings.hist_rows_lbl,
        &format!("{}", cfg.max_search_history_visible),
        "10",
        Message::SettingsMaxHistoryVisibleChanged,
    );

    let hist_stored = text_input_row(
        player.strings.hist_entries_lbl,
        &format!("{}", cfg.max_search_history_stored),
        "100",
        Message::SettingsMaxHistoryStoredChanged,
    );

    let recent = text_input_row(
        player.strings.recent_kept_lbl,
        &format!("{}", cfg.max_recently_played),
        "50",
        Message::SettingsMaxRecentlyPlayedChanged,
    );

    let content = Column::with_children([
        footer.into(),
        section(
            player.strings.sec_playback,
            [normalize, default_provider_section(player)],
        ),
        section(player.strings.sec_storage, [download_dir, cache_size]),
        section(
            player.strings.sec_history,
            [hist_visible, hist_stored, recent],
        ),
        Row::with_children([
            Column::with_children([
                section(player.strings.language_lbl, [language_section(player)]),
                section(player.strings.sec_appearance, [theme_section(player)]),
                Button::new(text(player.strings.reset_defaults))
                    .padding([theme::SPACING_SM, theme::SPACING_MD])
                    .on_press(Message::SettingsResetDefaults)
                    .into(),
            ])
            .spacing(theme::SPACING_XL)
            .into(),
            Space::new().width(Length::Fill).into(),
            Row::with_children([icons::icon(icons::LOGO_ICON, 128.0)
                .style(icon_accent())
                .into()])
            .height(Length::Fill)
            .align_y(alignment::Vertical::Center)
            .into(),
        ])
        .into(),
    ])
    .spacing(theme::SPACING_XL);

    scrollable(Container::new(content).padding([theme::SPACING_MD, theme::SPACING_XL]))
        .id(iced::widget::Id::new("settings_scroll"))
        .into()
}
