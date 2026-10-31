use iced::{
    alignment,
    widget::{checkbox, scrollable, text, text_input, Column, Container, Space},
    Element, Length,
};

use super::{
    shared_components::{dep_install_status, scope_tab_row, text_button, text_input_row},
    Message, MusicPlayer,
};
use crate::{
    app::{
        dependency_dialog::dep_desc,
        ui::styles::{fg_accent, fg_secondary},
        update::SettingsChange,
    },
    deps::DepKind,
    i18n::Language,
    providers::ProviderId,
    theme::{self, AppTheme},
};

fn default_provider_section(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let row = scope_tab_row(
        player.strings.direction,
        ProviderId::defaultable().iter().map(|&provider| {
            (
                player.strings.provider(provider),
                player.config.default_provider == provider,
                Message::SettingsChanged(SettingsChange::DefaultProvider(provider)),
            )
        }),
    );
    Column::with_children([text(player.strings.default_provider_lbl).into(), row])
        .spacing(theme::SPACING_SM)
        .align_x(player.strings.direction.align())
        .into()
}

fn cookie_browser_section(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let current = player.config.cookie_browser.as_deref();
    let mut items = Vec::with_capacity(crate::deps::COOKIE_BROWSERS.len() + 1);
    items.push((
        player.strings.cookies_off,
        current.is_none(),
        Message::SettingsChanged(SettingsChange::CookieBrowser(None)),
    ));
    for browser in crate::deps::COOKIE_BROWSERS {
        let browser = *browser;
        items.push((
            browser,
            current == Some(browser),
            Message::SettingsChanged(SettingsChange::CookieBrowser(Some(browser.to_string()))),
        ));
    }
    let row = scope_tab_row(player.strings.direction, items);
    Column::with_children([text(player.strings.cookie_browser_lbl).into(), row])
        .spacing(theme::SPACING_SM)
        .align_x(player.strings.direction.align())
        .into()
}

fn translation_section(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let cfg = &player.config;
    let base_url = text_input_row(
        player.strings.direction,
        player.strings.translation_base_url_lbl,
        &cfg.translation_base_url,
        crate::translate::DEFAULT_BASE_URL,
        |s| Message::SettingsChanged(SettingsChange::TranslationBaseUrl(s)),
    );
    let api_key = Column::with_children([
        Container::new(text(player.strings.translation_api_key_lbl))
            .padding([0.0, theme::SPACING_XS])
            .into(),
        text_input("", &cfg.translation_api_key)
            .secure(true)
            .align_x(player.strings.direction.align())
            .on_input(|s| Message::SettingsChanged(SettingsChange::TranslationApiKey(s)))
            .padding([theme::SPACING_SM, theme::SPACING_MD])
            .into(),
    ])
    .spacing(theme::SPACING_XS)
    .align_x(player.strings.direction.align())
    .into();
    let model = text_input_row(
        player.strings.direction,
        player.strings.translation_model_lbl,
        &cfg.translation_model,
        crate::translate::DEFAULT_MODEL,
        |s| Message::SettingsChanged(SettingsChange::TranslationModel(s)),
    );
    let loading = player
        .translation_models
        .as_ref()
        .is_some_and(crate::load_state::LoadState::is_loading);
    let refresh = text_button(player.strings.translation_refresh_models)
        .on_press_maybe((!loading).then_some(Message::RequestTranslationModels))
        .into();
    let mut children: Vec<Element<'_, Message, AppTheme>> = vec![base_url, api_key, model, refresh];
    match &player.translation_models {
        None => {}
        Some(crate::load_state::LoadState::Loading) => {
            children.push(text(player.strings.loading).style(fg_secondary()).into());
        }
        Some(crate::load_state::LoadState::Failed(e)) => {
            children.push(
                text((player.strings.translation_models_failed)(e))
                    .style(fg_secondary())
                    .into(),
            );
        }
        Some(crate::load_state::LoadState::Ready(models)) if models.is_empty() => {
            children.push(
                text(player.strings.translation_no_models)
                    .style(fg_secondary())
                    .into(),
            );
        }
        Some(crate::load_state::LoadState::Ready(models)) => {
            children.push(
                text_input(
                    player.strings.find_in_list,
                    &player.translation_models_filter,
                )
                .align_x(player.strings.direction.align())
                .on_input(Message::TranslationModelsFilterChanged)
                .padding(theme::SPACING_SM)
                .into(),
            );
            let needle = crate::util::FuzzyQuery::new(&player.translation_models_filter);
            let visible: Vec<&String> = models.iter().filter(|name| needle.matches(name)).collect();
            if visible.is_empty() {
                children.push(
                    text(player.strings.no_results_found)
                        .style(fg_secondary())
                        .into(),
                );
            } else {
                children.push(scope_tab_row(
                    player.strings.direction,
                    visible.into_iter().map(|name| {
                        (
                            name.clone(),
                            *name == cfg.translation_model,
                            Message::SettingsChanged(SettingsChange::TranslationModel(
                                name.clone(),
                            )),
                        )
                    }),
                ));
            }
        }
    }
    Column::with_children(children)
        .spacing(theme::SPACING_SM)
        .align_x(player.strings.direction.align())
        .into()
}

fn section<'a>(
    dir: crate::i18n::Direction,
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
            .align_x(dir.align())
            .into(),
    ])
    .spacing(theme::SPACING_MD)
    .align_x(dir.align())
    .into()
}

fn updates_section(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let tr = &player.strings;
    let check_on_startup = checkbox(player.config.check_updates_on_startup)
        .label(tr.check_updates_on_startup_lbl)
        .on_toggle(|b| Message::SettingsChanged(SettingsChange::CheckUpdatesOnStartup(b)))
        .spacing(theme::SPACING_MD)
        .into();
    let status: Element<'_, Message, AppTheme> = match &player.update_status {
        crate::app::update::UpdateStatus::Unchecked => Space::new().into(),
        crate::app::update::UpdateStatus::Checking => {
            text(tr.checking_for_updates).style(fg_secondary()).into()
        }
        crate::app::update::UpdateStatus::UpToDate => text(tr.up_to_date).style(fg_accent()).into(),
        crate::app::update::UpdateStatus::Available { version, .. } => Column::with_children([
            text((tr.update_available)(version))
                .style(fg_accent())
                .into(),
            text_button(tr.update_now)
                .on_press(Message::UpdateApp)
                .into(),
        ])
        .spacing(theme::SPACING_SM)
        .align_x(tr.direction.align())
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
                text((tr.percent)(pct)).style(fg_secondary()).into(),
            ])
            .spacing(theme::SPACING_SM)
            .align_x(tr.direction.align())
            .into()
        }
        crate::app::update::UpdateStatus::UpdateApplied { version } => {
            text((tr.update_applied)(version)).style(fg_accent()).into()
        }
        crate::app::update::UpdateStatus::Error(e) => {
            text((tr.update_failed)(e)).style(fg_secondary()).into()
        }
        crate::app::update::UpdateStatus::PackageManaged => {
            text(tr.package_managed).style(fg_secondary()).into()
        }
    };

    Column::with_children([
        text_button(tr.check_for_updates)
            .on_press(Message::CheckForUpdates)
            .into(),
        check_on_startup,
        text((tr.current_version)(crate::app::update::APP_VERSION))
            .style(fg_secondary())
            .into(),
        status,
    ])
    .spacing(theme::SPACING_SM)
    .align_x(tr.direction.align())
    .into()
}

fn language_section(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let row = scope_tab_row(
        player.strings.direction,
        Language::ALL.iter().map(|&language| {
            (
                language.label(),
                player.config.language == language,
                Message::SettingsChanged(SettingsChange::Language(language)),
            )
        }),
    );
    Column::with_children([text(player.strings.language_lbl).into(), row])
        .spacing(theme::SPACING_SM)
        .align_x(player.strings.direction.align())
        .into()
}

fn theme_section(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let row = scope_tab_row(
        player.strings.direction,
        theme::ThemeKind::ALL.iter().map(|&kind| {
            (
                kind.label(),
                player.config.theme_kind == kind,
                Message::SettingsChanged(SettingsChange::Theme(kind)),
            )
        }),
    );
    Column::with_children([text(player.strings.theme_lbl).into(), row])
        .spacing(theme::SPACING_SM)
        .align_x(player.strings.direction.align())
        .into()
}

#[allow(clippy::too_many_lines)]
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
        .on_toggle(|b| Message::SettingsChanged(SettingsChange::VolumeNormalization(b)))
        .spacing(theme::SPACING_MD)
        .into();

    let download_dir = text_input_row(
        player.strings.direction,
        player.strings.download_dir_lbl,
        &cfg.download_dir,
        "",
        |s| Message::SettingsChanged(SettingsChange::DownloadDir(s)),
    );

    let cache_size = {
        let row = text_input_row(
            player.strings.direction,
            player.strings.cache_size_lbl,
            &format!("{}", cfg.cache_max_size_mb),
            "1024",
            |s| Message::SettingsChanged(SettingsChange::CacheMaxSize(s)),
        );
        let used = text((player.strings.cache_used)(&crate::util::format_bytes(
            player.stream_cache.used_bytes(),
        )))
        .style(fg_secondary());
        Column::with_children([row, used.into()])
            .spacing(theme::SPACING_XS)
            .align_x(player.strings.direction.align())
            .into()
    };

    let hist_visible = text_input_row(
        player.strings.direction,
        player.strings.hist_rows_lbl,
        &format!("{}", cfg.max_search_history_visible),
        "10",
        |s| Message::SettingsChanged(SettingsChange::MaxHistoryVisible(s)),
    );

    let hist_stored = text_input_row(
        player.strings.direction,
        player.strings.hist_entries_lbl,
        &format!("{}", cfg.max_search_history_stored),
        "100",
        |s| Message::SettingsChanged(SettingsChange::MaxHistoryStored(s)),
    );

    let recent = text_input_row(
        player.strings.direction,
        player.strings.recent_kept_lbl,
        &format!("{}", cfg.max_recently_played),
        "50",
        |s| Message::SettingsChanged(SettingsChange::MaxRecentlyPlayed(s)),
    );

    let content = Column::with_children([
        footer.into(),
        section(
            player.strings.direction,
            player.strings.sec_playback,
            [
                normalize,
                default_provider_section(player),
                cookie_browser_section(player),
            ],
        ),
        section(
            player.strings.direction,
            player.strings.sec_translation,
            [translation_section(player)],
        ),
        section(
            player.strings.direction,
            player.strings.sec_storage,
            [download_dir, cache_size],
        ),
        section(
            player.strings.direction,
            player.strings.sec_history,
            [hist_visible, hist_stored, recent],
        ),
        section(
            player.strings.direction,
            player.strings.sec_dependencies,
            [dep_settings_section(player)],
        ),
        section(
            player.strings.direction,
            player.strings.sec_updates,
            [updates_section(player)],
        ),
        Column::with_children([
            section(
                player.strings.direction,
                player.strings.language_lbl,
                [language_section(player)],
            ),
            section(
                player.strings.direction,
                player.strings.sec_appearance,
                [theme_section(player)],
            ),
            text_button(player.strings.reset_defaults)
                .on_press(Message::SettingsResetDefaults)
                .into(),
        ])
        .spacing(theme::SPACING_XL)
        .align_x(player.strings.direction.align())
        .into(),
    ])
    .spacing(theme::SPACING_XL)
    .align_x(player.strings.direction.align());

    scrollable(Container::new(content).padding([theme::SPACING_MD, theme::SPACING_XL]))
        .id(iced::widget::Id::new("settings_scroll"))
        .into()
}

fn dep_settings_section(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let rows = DepKind::all()
        .iter()
        .map(|&kind| dep_settings_row(player, kind));
    Column::with_children(rows)
        .spacing(theme::SPACING_MD)
        .align_x(player.strings.direction.align())
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
            Err(e) => text((tr.deps_delete_failed_detail)(e.as_str()))
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
    let install_delete_btn: Element<'_, Message, AppTheme> = if app_managed {
        text_button(tr.deps_delete)
            .on_press_maybe(if installing || deleting {
                None
            } else {
                Some(Message::DepSettingsDelete(kind))
            })
            .into()
    } else if kind.auto_installable() {
        text_button(tr.deps_install)
            .on_press_maybe(if installing || deleting {
                None
            } else {
                Some(Message::DepSettingsInstall(kind))
            })
            .into()
    } else {
        Space::new().into()
    };

    Column::with_children([
        text(kind.name()).style(fg_accent()).into(),
        text(dep_desc(tr, kind)).style(fg_secondary()).into(),
        install_delete_btn,
        status,
    ])
    .spacing(theme::SPACING_XS)
    .align_x(tr.direction.align())
    .into()
}
