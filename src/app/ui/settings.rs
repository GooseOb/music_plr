use iced::{
    alignment,
    widget::{checkbox, scrollable, text, Button, Column, Container, Row, Space},
    Element, Length,
};

use super::{
    shared_components::{dep_install_status, scope_tab_row, text_input_row},
    Message, MusicPlayer,
};
use crate::{
    app::{
        dependency_dialog::dep_desc,
        ui::styles::{fg_accent, fg_secondary},
    },
    deps::DepKind,
    i18n::Language,
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

fn updates_section(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let tr = &player.strings;
    let status: Element<'_, Message, AppTheme> = match &player.update_status {
        crate::app::update::UpdateStatus::Unchecked
        | crate::app::update::UpdateStatus::Checking => {
            text(tr.checking_for_updates).style(fg_secondary()).into()
        }
        crate::app::update::UpdateStatus::UpToDate => text(tr.up_to_date).style(fg_accent()).into(),
        crate::app::update::UpdateStatus::Available { version, .. } => Column::with_children([
            text((tr.update_available)(version))
                .style(fg_accent())
                .into(),
            Button::new(text(tr.update_now))
                .padding([theme::SPACING_XS, theme::SPACING_MD])
                .on_press(Message::UpdateApp)
                .into(),
        ])
        .spacing(theme::SPACING_SM)
        .into(),
        crate::app::update::UpdateStatus::Updating { progress } => {
            let (downloaded, total) = *progress;
            let pct = if total > 0 {
                (downloaded * 100).checked_div(total).unwrap_or(0).min(100)
            } else {
                0
            };
            Column::with_children([
                text(tr.updating).into(),
                text(format!("{pct}%")).style(fg_secondary()).into(),
            ])
            .spacing(theme::SPACING_SM)
            .into()
        }
        crate::app::update::UpdateStatus::UpdateApplied => {
            text((tr.update_applied)(crate::app::update::APP_VERSION))
                .style(fg_accent())
                .into()
        }
        crate::app::update::UpdateStatus::Error(e) => {
            text((tr.update_failed)(e)).style(fg_secondary()).into()
        }
        crate::app::update::UpdateStatus::PackageManaged => {
            text(tr.package_managed).style(fg_secondary()).into()
        }
    };

    Column::with_children([
        Button::new(text(tr.check_for_updates))
            .padding([theme::SPACING_XS, theme::SPACING_MD])
            .on_press(Message::CheckForUpdates)
            .into(),
        text((tr.current_version)(crate::app::update::APP_VERSION))
            .style(fg_secondary())
            .into(),
        status,
    ])
    .spacing(theme::SPACING_SM)
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
        section(
            player.strings.sec_dependencies,
            [dep_settings_section(player)],
        ),
        section(player.strings.sec_updates, [updates_section(player)]),
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
    ])
    .spacing(theme::SPACING_XL);

    scrollable(Container::new(content).padding([theme::SPACING_MD, theme::SPACING_XL]))
        .id(iced::widget::Id::new("settings_scroll"))
        .into()
}

fn dep_settings_section(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let rows: Vec<Element<'_, Message, AppTheme>> = DepKind::all()
        .iter()
        .map(|&kind| dep_settings_row(player, kind))
        .collect();
    Column::with_children(rows)
        .spacing(theme::SPACING_MD)
        .into()
}

/// A row in the Settings Dependencies section: name, description, live status,
/// and Install/Delete buttons. Even deps already present on the system can be
/// (re)installed as a managed copy, with a "Found on system" note.
fn dep_settings_row(player: &MusicPlayer, kind: DepKind) -> Element<'_, Message, AppTheme> {
    let tr = &player.strings;
    let op = player.dep_ops.get(&kind);
    let installing = op.is_some_and(|o| o.installing);
    let deleting = op.is_some_and(|o| o.deleting);

    let status: Element<'_, Message, AppTheme> = if let Some(el) = dep_install_status(op, tr) {
        el
    } else if deleting {
        text(tr.deps_deleting).style(fg_secondary()).into()
    } else if let Some(res) = op.and_then(|o| o.delete_result.as_ref()) {
        match res {
            Ok(()) => text(tr.deps_deleted).style(fg_accent()).into(),
            Err(e) => text(format!("{}: {}", tr.deps_delete_failed, e))
                .style(fg_secondary())
                .into(),
        }
    } else if crate::deps::installed_via_app(kind) {
        text(tr.deps_managed_by_app).style(fg_accent()).into()
    } else if crate::deps::is_available(kind) {
        text(tr.deps_found_on_system).style(fg_accent()).into()
    } else {
        text(tr.deps_not_installed).style(fg_secondary()).into()
    };

    let app_managed = crate::deps::installed_via_app(kind);
    let install_btn: Element<'_, Message, AppTheme> = if kind.auto_installable() && !app_managed {
        Button::new(text(tr.deps_install))
            .padding([theme::SPACING_XS, theme::SPACING_MD])
            .on_press_maybe(if installing || deleting {
                None
            } else {
                Some(Message::DepSettingsInstall(kind))
            })
            .into()
    } else {
        Space::new().into()
    };

    let delete_btn: Element<'_, Message, AppTheme> = if app_managed {
        Button::new(text(tr.deps_delete))
            .padding([theme::SPACING_XS, theme::SPACING_MD])
            .on_press_maybe(if installing || deleting {
                None
            } else {
                Some(Message::DepSettingsDelete(kind))
            })
            .into()
    } else {
        Space::new().into()
    };

    Column::with_children([
        text(kind.name()).style(fg_accent()).into(),
        text(dep_desc(tr, kind)).style(fg_secondary()).into(),
        Row::with_children([install_btn, delete_btn])
            .spacing(theme::SPACING_SM)
            .into(),
        status,
    ])
    .spacing(theme::SPACING_XS)
    .into()
}
