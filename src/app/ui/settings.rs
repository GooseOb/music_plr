use iced::{
    alignment,
    widget::{checkbox, scrollable, text, text_input, Column, Container, Space},
    Element, Length,
};

use super::{
    shared_components::{
        dep_install_status, scope_tab_row, section_label, text_button, text_input_row,
    },
    Message, MusicPlayer,
};
use crate::{
    app::{
        custom_provider::{ConnectionTest, CustomProviderField, CustomProviderForm},
        dependency_dialog::dep_desc,
        ui::{
            shared_components::secure_text_input_row,
            styles::{fg_accent, fg_secondary},
        },
        update::SettingsChange,
    },
    data::custom_providers::CustomProviderKind,
    deps::DepKind,
    i18n::{Direction, Language},
    load_state::LoadState,
    providers::ProviderId,
    theme::{self, AppTheme},
};

/// A labelled row of chips: the shared shape of every settings picker.
fn picker<'a, S>(
    dir: Direction,
    label: impl text::IntoFragment<'a>,
    items: impl DoubleEndedIterator<Item = (S, bool, Message)>,
) -> Element<'a, Message, AppTheme>
where
    S: text::IntoFragment<'static>,
{
    Column::with_children([text(label).into(), scope_tab_row(dir, items)])
        .spacing(theme::SPACING_SM)
        .align_x(dir.align())
        .into()
}

fn default_provider_picker(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    picker(
        player.strings.direction,
        player.strings.default_provider_lbl,
        ProviderId::defaultable().iter().map(|&provider| {
            (
                player.strings.provider(provider),
                player.config.default_provider == provider,
                Message::SettingsChanged(SettingsChange::DefaultProvider(provider)),
            )
        }),
    )
}

fn default_lyrics_provider_picker(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    picker(
        player.strings.direction,
        player.strings.default_lyrics_provider_lbl,
        player
            .config
            .enabled_lyrics_providers()
            .into_iter()
            .map(|provider| {
                (
                    provider.name(),
                    player.config.default_lyrics_provider == provider,
                    Message::SettingsChanged(SettingsChange::DefaultLyricsProvider(provider)),
                )
            }),
    )
}

fn lyrics_provider_checkbox(
    player: &MusicPlayer,
    provider: crate::lyrics::LyricsProvider,
) -> Element<'_, Message, AppTheme> {
    checkbox(player.config.lyrics_provider_enabled(provider))
        .label(provider.name())
        .on_toggle(move |enabled| {
            Message::SettingsChanged(SettingsChange::LyricsProviderEnabled(provider, enabled))
        })
        .spacing(theme::SPACING_MD)
        .into()
}

fn lyrics_section(
    player: &MusicPlayer,
) -> impl IntoIterator<Item = Element<'_, Message, AppTheme>> {
    let dir = player.strings.direction;
    let toggles = Column::with_children(
        crate::lyrics::LyricsProvider::all()
            .iter()
            .copied()
            .map(|provider| lyrics_provider_checkbox(player, provider)),
    )
    .spacing(theme::SPACING_SM)
    .align_x(dir.align());
    [default_lyrics_provider_picker(player), toggles.into()]
}

fn cookie_browser_picker(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let current = player.config.cookie_browser.as_deref();
    let items = std::iter::once((
        player.strings.cookies_off,
        current.is_none(),
        Message::SettingsChanged(SettingsChange::CookieBrowser(None)),
    ))
    .chain(crate::deps::COOKIE_BROWSERS.iter().map(|&browser| {
        (
            browser,
            current == Some(browser),
            Message::SettingsChanged(SettingsChange::CookieBrowser(Some(browser.to_string()))),
        )
    }));
    picker(
        player.strings.direction,
        player.strings.cookie_browser_lbl,
        items,
    )
}

fn translation_section(
    player: &MusicPlayer,
) -> impl IntoIterator<Item = Element<'_, Message, AppTheme>> {
    let cfg = &player.config;
    let base_url = text_input_row(
        player.strings.direction,
        player.strings.translation_base_url_lbl,
        &cfg.translation_base_url,
        crate::translate::DEFAULT_BASE_URL,
        |s| Message::SettingsChanged(SettingsChange::TranslationBaseUrl(s)),
    );
    let api_key = secure_text_input_row(
        player.strings.direction,
        player.strings.translation_api_key_lbl,
        &cfg.translation_api_key,
        "",
        |s| Message::SettingsChanged(SettingsChange::TranslationApiKey(s)),
    );
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
        .is_some_and(LoadState::is_loading);
    let refresh = text_button(player.strings.translation_refresh_models)
        .on_press_maybe((!loading).then_some(Message::RequestTranslationModels))
        .into();
    let models_panel = match &player.translation_models {
        Some(LoadState::Ready(models)) if !models.is_empty() => {
            let filter = text_input(
                player.strings.find_in_list,
                &player.translation_models_filter,
            )
            .align_x(player.strings.direction.align())
            .on_input(Message::TranslationModelsFilterChanged)
            .padding(theme::SPACING_SM)
            .into();
            let needle = crate::util::FuzzyQuery::new(&player.translation_models_filter);
            let mut visible = models.iter().filter(|name| needle.matches(name)).peekable();
            let list = if visible.peek().is_none() {
                text(player.strings.no_results_found)
                    .style(fg_secondary())
                    .into()
            } else {
                scope_tab_row(
                    player.strings.direction,
                    visible.map(|name| {
                        (
                            name.clone(),
                            *name == cfg.translation_model,
                            Message::SettingsChanged(SettingsChange::TranslationModel(
                                name.clone(),
                            )),
                        )
                    }),
                )
            };
            Some([filter, list])
        }
        _ => None,
    };
    let status = match &player.translation_models {
        None => None,
        Some(LoadState::Loading) => Some(text(player.strings.loading).style(fg_secondary()).into()),
        Some(LoadState::Failed(e)) => Some(
            text((player.strings.translation_models_failed)(e))
                .style(fg_secondary())
                .into(),
        ),
        Some(LoadState::Ready(models)) => models.is_empty().then(|| {
            text(player.strings.translation_no_models)
                .style(fg_secondary())
                .into()
        }),
    };
    [base_url, api_key, model, refresh]
        .into_iter()
        .chain(status)
        .chain(models_panel.into_iter().flatten())
}

fn section<'a>(
    dir: Direction,
    label: &'a str,
    children: impl IntoIterator<Item = Element<'a, Message, AppTheme>>,
) -> Element<'a, Message, AppTheme> {
    Column::with_children([
        section_label(dir, label),
        Column::with_children(children)
            .spacing(theme::SPACING_SM)
            .align_x(dir.align())
            .into(),
    ])
    .spacing(theme::SPACING_MD)
    .align_x(dir.align())
    .into()
}

fn updates_section(
    player: &MusicPlayer,
) -> impl IntoIterator<Item = Element<'_, Message, AppTheme>> {
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

    [
        text_button(tr.check_for_updates)
            .on_press(Message::CheckForUpdates)
            .into(),
        check_on_startup,
        text((tr.current_version)(crate::app::update::APP_VERSION))
            .style(fg_secondary())
            .into(),
        status,
    ]
}

fn language_picker(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    picker(
        player.strings.direction,
        player.strings.language_lbl,
        Language::ALL.iter().map(|&language| {
            (
                language.label(),
                player.config.language == language,
                Message::SettingsChanged(SettingsChange::Language(language)),
            )
        }),
    )
}

fn theme_picker(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    picker(
        player.strings.direction,
        player.strings.theme_lbl,
        theme::ThemeKind::ALL.iter().map(|&kind| {
            (
                kind.label(),
                player.config.theme_kind == kind,
                Message::SettingsChanged(SettingsChange::Theme(kind)),
            )
        }),
    )
}

/// One provider's on/off switch. Switched-off providers are hidden from every
/// picker (search bar, default provider, context menus, artist headers,
/// server-playlist groups); their existing tracks keep playing. `labelled`
/// draws the provider's name next to the box, which the custom-provider rows
/// skip because their own row already shows it.
fn provider_checkbox(
    player: &MusicPlayer,
    provider: ProviderId,
    labelled: bool,
) -> Element<'_, Message, AppTheme> {
    let toggle = checkbox(!player.config.disabled_providers.contains(&provider))
        .on_toggle(move |enabled| {
            Message::SettingsChanged(SettingsChange::ProviderEnabled(provider, enabled))
        })
        .spacing(theme::SPACING_MD);
    if labelled {
        toggle.label(player.strings.provider(provider)).into()
    } else {
        toggle.into()
    }
}

/// The built-in providers' checkboxes plus the custom-provider list.
fn providers_section(
    player: &MusicPlayer,
) -> impl IntoIterator<Item = Element<'_, Message, AppTheme>> {
    let tr = &player.strings;
    let dir = tr.direction;
    let builtin = Column::with_children(
        ProviderId::builtin_all()
            .iter()
            .copied()
            .filter(|provider| *provider != ProviderId::Local)
            .map(|provider| provider_checkbox(player, provider, true)),
    )
    .spacing(theme::SPACING_SM)
    .align_x(dir.align());

    [
        default_provider_picker(player),
        builtin.into(),
        text(tr.sec_providers).style(fg_secondary()).into(),
        custom_providers_section(player),
    ]
}

fn custom_providers_section(player: &MusicPlayer) -> Element<'_, Message, AppTheme> {
    let tr = &player.strings;
    let dir = tr.direction;
    let children = player
        .custom_providers
        .providers
        .is_empty()
        .then(|| text(tr.custom_provider_none).style(fg_secondary()).into())
        .into_iter()
        .chain(player.custom_providers.providers.iter().map(|provider| {
            let info = Column::with_children([
                text(&provider.name).style(fg_accent()).into(),
                text(format!(
                    "{} · {}",
                    provider.kind.label(),
                    provider.server_url
                ))
                .style(fg_secondary())
                .into(),
            ])
            .spacing(theme::SPACING_2XS)
            .width(Length::Fill)
            .align_x(dir.align());
            let actions = dir
                .row(
                    [
                        text_button(tr.ctx_edit)
                            .on_press(Message::CustomProviderEdit(provider.id))
                            .into(),
                        text_button(tr.delete)
                            .on_press(Message::CustomProviderDelete(provider.id))
                            .into(),
                    ]
                    .into_iter(),
                )
                .spacing(theme::SPACING_XS);
            dir.row(
                [
                    provider_checkbox(
                        player,
                        ProviderId::Custom(provider.kind, provider.id),
                        false,
                    ),
                    info.into(),
                    actions.into(),
                ]
                .into_iter(),
            )
            .spacing(theme::SPACING_MD)
            .align_y(alignment::Vertical::Center)
            .into()
        }))
        .chain([player.custom_provider_form.as_ref().map_or_else(
            || {
                text_button(tr.custom_provider_add)
                    .on_press(Message::CustomProviderAdd)
                    .into()
            },
            |form| custom_provider_form(player, form),
        )]);

    Column::with_children(children)
        .spacing(theme::SPACING_MD)
        .align_x(dir.align())
        .into()
}

fn custom_provider_form<'a>(
    player: &'a MusicPlayer,
    form: &'a CustomProviderForm,
) -> Element<'a, Message, AppTheme> {
    let tr = &player.strings;
    let dir = tr.direction;
    let title = if form.id.is_some() {
        tr.ctx_edit
    } else {
        tr.custom_provider_add
    };
    let name = text_input_row(
        dir,
        tr.custom_provider_name_lbl,
        &form.draft.name,
        "",
        |value| Message::CustomProviderField(CustomProviderField::Name, value),
    );
    let kind = picker(
        dir,
        tr.custom_provider_type_lbl,
        CustomProviderKind::ALL.iter().map(|&kind| {
            (
                kind.label(),
                form.draft.kind == kind,
                Message::CustomProviderKindChanged(kind),
            )
        }),
    );
    let server_url = text_input_row(
        dir,
        tr.custom_provider_url_lbl,
        &form.draft.server_url,
        "https://music.example.com",
        |value| Message::CustomProviderField(CustomProviderField::ServerUrl, value),
    );
    let username = text_input_row(
        dir,
        tr.custom_provider_username_lbl,
        &form.draft.username,
        "",
        |value| Message::CustomProviderField(CustomProviderField::Username, value),
    );
    let password = secure_text_input_row(
        dir,
        tr.custom_provider_password_lbl,
        &form.draft.password,
        "",
        |value| Message::CustomProviderField(CustomProviderField::Password, value),
    );

    let testing = matches!(form.test, ConnectionTest::Testing);
    let actions = dir
        .row(
            [
                text_button(tr.custom_provider_test)
                    .on_press_maybe((!testing).then_some(Message::CustomProviderTest))
                    .into(),
                text_button(tr.save)
                    .on_press(Message::CustomProviderSave)
                    .into(),
                text_button(tr.cancel)
                    .on_press(Message::CustomProviderCancel)
                    .into(),
            ]
            .into_iter(),
        )
        .spacing(theme::SPACING_XS);

    let status: Element<'a, Message, AppTheme> = match &form.test {
        ConnectionTest::Idle => Space::new().into(),
        ConnectionTest::Testing => text(tr.custom_provider_testing)
            .style(fg_secondary())
            .into(),
        ConnectionTest::Ok(detail) => text((tr.custom_provider_test_ok)(detail))
            .style(fg_accent())
            .into(),
        ConnectionTest::Failed(error) => text((tr.custom_provider_test_failed)(error))
            .style(fg_secondary())
            .into(),
    };

    Column::with_children([
        text(title)
            .size(theme::TEXT_SIZE_LG)
            .style(fg_accent())
            .into(),
        name,
        kind,
        server_url,
        username,
        password,
        actions.into(),
        status,
    ])
    .spacing(theme::SPACING_SM)
    .align_x(dir.align())
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
            [normalize, cookie_browser_picker(player)],
        ),
        section(
            player.strings.direction,
            player.strings.providers,
            providers_section(player),
        ),
        section(
            player.strings.direction,
            player.strings.sec_lyrics,
            lyrics_section(player),
        ),
        section(
            player.strings.direction,
            player.strings.sec_updates,
            updates_section(player),
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
            player.strings.sec_translation,
            translation_section(player),
        ),
        section(
            player.strings.direction,
            player.strings.language_lbl,
            [language_picker(player)],
        ),
        section(
            player.strings.direction,
            player.strings.sec_appearance,
            [theme_picker(player)],
        ),
        section(
            player.strings.direction,
            player.strings.sec_dependencies,
            DepKind::all()
                .iter()
                .map(|&kind| dep_settings_row(player, kind)),
        ),
        text_button(player.strings.reset_defaults)
            .on_press(Message::SettingsResetDefaults)
            .into(),
    ])
    .spacing(theme::SPACING_XL)
    .align_x(player.strings.direction.align());

    scrollable(Container::new(content).padding([theme::SPACING_MD, theme::SPACING_XL]))
        .id(iced::widget::Id::new("settings_scroll"))
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
