use std::fmt::{self, Write};

use serde::{Deserialize, Serialize};

use crate::providers::{ErrorKind, ProviderError, ProviderId};

/// Generous upper bound for one submenu label, so `with_search_marker` fills a single buffer.
const SUBMENU_LABEL_CAPACITY: usize = 96;

pub mod ar;
pub mod be;
pub mod de;
pub mod en;
pub mod es;
pub mod fr;
pub mod hi;
pub mod ja;
pub mod pl;
pub mod pt_br;
pub mod ru;
pub mod uk;
pub mod zh_cn;

mod plural;

/// Writing direction of a language. The layout mirrors itself for [`Rtl`]:
/// rows put their first child on the right and paragraphs align right.
///
/// [`Rtl`]: Direction::Rtl
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Direction {
    #[default]
    Ltr,
    Rtl,
}

impl Direction {
    pub fn is_rtl(self) -> bool {
        matches!(self, Self::Rtl)
    }
}

/// Declares the `Language` enum and its `ALL` / `label` / `strings` machinery
/// from a single list, so adding a language only requires creating its module
/// and appending one entry here — the enum itself is never hand-edited.
///
/// Each entry is `Variant => ("display label", module)` where `module` exposes
/// a `pub const STRINGS: Strings`. The first entry is the `#[default]`.
macro_rules! languages {
    (
        $first:ident => ($fl:literal, $fm:ident, $fp:literal)
        $(, $variant:ident => ($label:literal, $module:ident, $prefix:literal) )*
        $(,)?
    ) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
        pub enum Language {
            #[default]
            $first,
            $( $variant, )*
        }

        impl Language {
            pub const ALL: [Language; [$( Language::$variant, )* Language::$first ].len()] =
                [ $( Language::$variant, )* Language::$first ];

            pub fn label(self) -> &'static str {
                match self {
                    Language::$first => $fl,
                    $( Language::$variant => $label, )*
                }
            }

            pub fn strings(self) -> &'static Strings {
                match self {
                    Language::$first => &$fm::STRINGS,
                    $( Language::$variant => &$module::STRINGS, )*
                }
            }

            pub fn direction(self) -> Direction {
                self.strings().direction
            }

            pub fn from_system_locale() -> Option<Self> {
                let locale = sys_locale::get_locale()?;
                let prefix = locale.split(['-', '_', '.']).next()?;
                Some(match prefix {
                    $fp => Language::$first,
                    $( $prefix => Language::$variant, )*
                    _ => return None,
                })
            }
        }
    };
}

languages! {
    Ar => ("العربية", ar, "ar"),
    Be => ("Беларуская", be, "be"),
    De => ("Deutsch", de, "de"),
    En => ("English", en, "en"),
    Es => ("Español", es, "es"),
    Fr => ("Français", fr, "fr"),
    Hi => ("हिन्दी", hi, "hi"),
    Ja => ("日本語", ja, "ja"),
    Pl => ("Polski", pl, "pl"),
    PtBr => ("Português (Brasil)", pt_br, "pt"),
    Ru => ("Русский", ru, "ru"),
    Uk => ("Українська", uk, "uk"),
    ZhCn => ("简体中文", zh_cn, "zh"),
}

/// All user-facing strings for one language. Simple labels are `&'static
/// str`; parameterized messages are function fields so each language can
/// handle word order and plural forms itself.
pub struct Strings {
    pub search: &'static str,
    pub downloads: &'static str,
    pub settings: &'static str,
    pub library: &'static str,
    pub nothing_saved_yet: &'static str,
    pub new_playlist_name: &'static str,

    pub queue: &'static str,
    pub recently_played: &'static str,
    pub now_playing_from: &'static str,
    pub now_playing: &'static str,
    pub up_next: &'static str,
    pub no_track_playing: &'static str,
    pub no_more_tracks_in_queue: &'static str,
    pub no_recently_played_tracks: &'static str,

    pub not_playing: &'static str,
    pub no_tracks_found: &'static str,

    pub searching: &'static str,
    pub loading: &'static str,
    pub load_more: &'static str,
    pub no_results_found: &'static str,
    pub no_recent_searches: &'static str,
    pub generating_radio: &'static str,

    pub not_an_artist_page: &'static str,
    pub provided_by: &'static str,
    pub retry: &'static str,
    pub nothing_here: &'static str,
    pub most_popular_songs: &'static str,
    pub albums: &'static str,
    pub playlists: &'static str,
    pub fans_also_like: &'static str,

    pub looking_up_lyrics: &'static str,
    pub play_a_track_for_lyrics: &'static str,
    pub lyrics_selectable: &'static str,
    pub lyrics_synced: &'static str,
    pub lyrics_plain: &'static str,
    pub lyrics_translations: &'static str,
    pub lyrics_original: &'static str,
    pub add_custom: &'static str,
    pub edit_lyrics: &'static str,
    pub lyrics_editor_hint: &'static str,
    pub lyrics_name: &'static str,
    pub translate_with_ai: &'static str,
    pub translate_title: &'static str,
    pub translate_language_lbl: &'static str,
    pub translate_prompt_lbl: &'static str,
    pub translate_submit: &'static str,
    pub translate_reset: &'static str,
    pub translating: &'static str,
    pub translation_ready: &'static str,
    pub translation_language_empty: &'static str,
    pub no_lyrics_to_translate: &'static str,
    pub translation_not_configured: &'static str,
    pub translation_failed: fn(&str) -> String,
    pub sec_translation: &'static str,
    pub translation_base_url_lbl: &'static str,
    pub translation_api_key_lbl: &'static str,
    pub translation_model_lbl: &'static str,
    pub translation_refresh_models: &'static str,
    pub translation_no_models: &'static str,
    pub translation_models_failed: fn(&str) -> String,

    pub playlist_not_found: &'static str,
    pub save_as_playlist: &'static str,
    pub liked_songs: &'static str,
    pub add_local: &'static str,
    pub downloaded_tracks: &'static str,
    pub no_downloaded_tracks: &'static str,

    pub ctx_play: &'static str,
    pub ctx_play_local: &'static str,
    pub ctx_edit: &'static str,
    pub ctx_go_to_artist: &'static str,
    pub ctx_add_to_queue: &'static str,
    pub ctx_add_to_playlist: &'static str,
    pub ctx_download: &'static str,
    pub song_radio: &'static str,
    pub artist_radio: &'static str,
    pub ctx_remove_from_queue: &'static str,
    pub ctx_remove_from_playlist: &'static str,
    pub ctx_remove_from_recent: &'static str,
    pub ctx_add_to_trashbin: &'static str,
    pub ctx_remove_from_trashbin: &'static str,
    pub ctx_clear_cache: &'static str,
    pub ctx_open: &'static str,
    pub ctx_open_in_browser: &'static str,
    pub ctx_add_to_library: &'static str,
    pub ctx_remove_from_library: &'static str,
    pub trashbin: &'static str,
    pub sub_play_via: fn(&str, bool) -> String,
    pub sub_download_from: fn(&str, bool) -> String,
    pub sub_via: fn(&str, bool) -> String,
    pub sub_on: fn(&str, bool) -> String,
    pub sub_clear_cache_from: fn(&str, bool) -> String,

    pub current: &'static str,
    pub select: &'static str,
    pub find: &'static str,
    pub finding: &'static str,
    pub found_on: fn(&str) -> String,
    pub not_linked: &'static str,
    pub providers: &'static str,
    pub provider_local: &'static str,
    pub no_provider_data: &'static str,
    pub save: &'static str,
    pub cancel: &'static str,
    pub confirm: &'static str,
    pub edit_track: &'static str,
    pub manual_search: &'static str,
    pub manual_search_on: fn(&str) -> String,
    pub lbl_title: &'static str,
    pub lbl_artist: &'static str,
    pub ph_track_title: &'static str,
    pub ph_track_artist: &'static str,
    pub delete: &'static str,
    pub delete_playlist_q: &'static str,
    pub tracks_wont_be_deleted: &'static str,
    pub lbl_id: &'static str,
    pub lbl_url: &'static str,
    pub lbl_artist_id: &'static str,
    pub lbl_duration_secs: &'static str,
    pub lbl_thumbnail: &'static str,
    pub lbl_album: &'static str,

    pub sec_playback: &'static str,
    pub sec_storage: &'static str,
    pub sec_history: &'static str,
    pub sec_appearance: &'static str,
    pub sec_dependencies: &'static str,
    pub theme_lbl: &'static str,
    pub language_lbl: &'static str,
    pub default_provider_lbl: &'static str,
    pub cookie_browser_lbl: &'static str,
    pub cookies_off: &'static str,
    pub normalize_volume_lbl: &'static str,
    pub download_dir_lbl: &'static str,
    pub cache_size_lbl: &'static str,
    pub cache_used: fn(&str) -> String,
    pub hist_rows_lbl: &'static str,
    pub hist_entries_lbl: &'static str,
    pub recent_kept_lbl: &'static str,
    pub reset_defaults: &'static str,

    pub find_in_list: &'static str,
    pub scope_songs: &'static str,
    pub scope_videos: &'static str,
    pub scope_artists: &'static str,
    pub scope_albums: &'static str,
    pub scope_playlists: &'static str,
    pub radio_word_song: &'static str,
    pub radio_word_artist: &'static str,

    pub lyrics_copied: &'static str,
    pub saved_to_library: &'static str,
    pub removed_from_library: &'static str,
    pub no_lyrics_found: &'static str,
    pub lyrics_saved: &'static str,
    pub lyrics_deleted: &'static str,
    pub lyrics_empty: &'static str,
    pub lyrics_name_empty: &'static str,
    pub select_playlist_drop: &'static str,
    pub reordered_playlist: &'static str,
    pub max_panes_reached: &'static str,

    pub n_saved: fn(usize) -> String,
    pub n_plays: fn(u64) -> String,
    pub search_youtube: &'static str,
    pub search_local: &'static str,
    pub search_provider: fn(&str) -> String,
    pub added: fn(usize) -> String,
    pub added_to: fn(usize, &str) -> String,
    pub removed_n: fn(usize) -> String,
    pub removed_from: fn(usize, &str) -> String,
    pub pasted_into: fn(usize, &str) -> String,
    pub saved_title: fn(&str) -> String,
    pub playlist_created: fn(&str) -> String,
    pub added_local: fn(usize) -> String,
    pub downloading_n: fn(usize) -> String,
    pub resolving_on: fn(&str, &str) -> String,
    pub resolving_player_client: &'static str,
    pub resolved_player_client: fn(&str) -> String,
    pub player_client_unavailable: &'static str,
    pub provider_no_radio: fn(&str) -> String,
    pub generating_radio_for: fn(&str, &str) -> String,
    pub could_not_find_on: fn(&str, &str) -> String,
    pub opening: fn(&str) -> String,
    pub opening_artist: fn(&str) -> String,
    pub download_complete: fn(&str) -> String,
    pub failed_resolve_on: fn(&str, &str, &str) -> String,
    pub couldnt_load: fn(&str) -> String,
    pub couldnt_load_lyrics: fn(&str) -> String,
    pub search_failed: fn(&str) -> String,
    pub error_network: &'static str,
    pub error_rate_limited: &'static str,
    pub error_denied: &'static str,
    pub error_unavailable: &'static str,
    pub error_tool_missing: &'static str,
    pub error_parse: &'static str,
    pub error_other: &'static str,
    pub cache_cleared_for: fn(&str) -> String,
    pub ctx_add_to_queue_n: fn(usize) -> String,
    pub ctx_add_to_playlist_n: fn(usize) -> String,
    pub ctx_download_n: fn(usize) -> String,
    pub ctx_remove_from_queue_n: fn(usize) -> String,
    pub ctx_remove_from_playlist_n: fn(usize) -> String,
    pub ctx_remove_from_recent_n: fn(usize) -> String,
    pub ctx_add_to_trashbin_n: fn(usize) -> String,
    pub ctx_remove_from_trashbin_n: fn(usize) -> String,
    pub open_in_browser_failed: fn(&str) -> String,
    pub creating_playlist: fn(&str) -> String,

    pub import_playlist: &'static str,
    pub import_method_native: &'static str,
    pub import_method_filelist: &'static str,
    pub import_method_csv: &'static str,
    pub import_native_hint: &'static str,
    pub import_csv_name_col: &'static str,
    pub import_csv_artist_col: &'static str,
    pub import_csv_album_col: &'static str,
    pub import_csv_preset: &'static str,
    pub import_csv_preset_default: &'static str,
    pub import_csv_preset_exportify: &'static str,
    pub import_csv_exportify_note: &'static str,
    pub import_pattern_lbl: &'static str,
    pub import_playlist_name: &'static str,
    pub import_add_pattern: &'static str,
    pub import_select_file: &'static str,
    pub import_select_folder: &'static str,
    pub import_pattern_conflict: fn(&str, &str) -> String,
    pub import_playlists_imported: fn(usize) -> String,
    pub import_imported_into: fn(usize, &str) -> String,
    pub import_no_tracks: &'static str,
    pub import_no_match: &'static str,
    pub import_bad_file: &'static str,

    pub deps_title: &'static str,
    pub deps_intro: &'static str,
    pub deps_install: &'static str,
    pub deps_install_selected: &'static str,
    pub deps_discard: &'static str,
    pub deps_installing: &'static str,
    pub deps_installed: &'static str,
    pub deps_failed: &'static str,
    pub deps_installed_toast: &'static str,
    pub deps_yt_dlp_desc: &'static str,
    pub deps_ytmusicapi_desc: &'static str,
    pub deps_python3_desc: &'static str,
    pub deps_ytmusicapi_requires_python: &'static str,
    pub deps_play_requires_yt_dlp: &'static str,
    pub deps_source_not_playable: &'static str,
    pub deps_found_section_title: &'static str,
    pub deps_found_section_intro: &'static str,
    pub deps_found_on_system: &'static str,
    pub deps_managed_by_app: &'static str,
    pub deps_not_installed: &'static str,
    pub deps_delete: &'static str,
    pub deps_deleting: &'static str,
    pub deps_deleted: &'static str,
    pub deps_delete_failed: &'static str,

    pub sec_updates: &'static str,
    pub check_for_updates: &'static str,
    pub check_updates_on_startup_lbl: &'static str,
    pub checking_for_updates: &'static str,
    pub up_to_date: &'static str,
    pub current_version: fn(&str) -> String,
    pub update_available: fn(&str) -> String,
    pub update_now: &'static str,
    pub updating: &'static str,
    pub update_applied: fn(&str) -> String,
    pub package_managed: &'static str,
    pub update_failed: fn(&str) -> String,
    pub percent: fn(u64) -> String,
    pub list_match_pos: fn(usize, usize) -> String,
    pub deps_failed_detail: fn(&str) -> String,
    pub deps_delete_failed_detail: fn(&str) -> String,

    pub palette_hint: &'static str,
    pub palette_commands: &'static str,
    pub palette_views: &'static str,
    pub pal_play_pause: &'static str,
    pub pal_next_track: &'static str,
    pub pal_prev_track: &'static str,
    pub pal_seek_back: &'static str,
    pub pal_seek_forward: &'static str,
    pub pal_toggle_queue: &'static str,
    pub pal_queue_tab: &'static str,
    pub pal_toggle_repeat: &'static str,
    pub pal_mute: &'static str,
    pub pal_volume_up: &'static str,
    pub pal_volume_down: &'static str,
    pub pal_show_lyrics: &'static str,
    pub pal_play_focused: &'static str,
    pub pal_toggle_selection: &'static str,
    pub pal_select_all: &'static str,
    pub pal_copy: &'static str,
    pub pal_paste: &'static str,
    pub pal_delete_selected: &'static str,
    pub pal_open_menu: &'static str,
    pub pal_back: &'static str,
    pub pal_forward: &'static str,
    pub pal_prev_playlist: &'static str,
    pub pal_next_playlist: &'static str,
    pub pal_focus_search: &'static str,
    pub pal_run_search: &'static str,
    pub pal_split_side: &'static str,
    pub pal_split_stacked: &'static str,
    pub pal_close_pane: &'static str,
    pub pal_next_pane: &'static str,
    pub pal_prev_pane: &'static str,

    pub direction: Direction,
}

impl Strings {
    pub fn search_placeholder(&self, provider: ProviderId) -> String {
        match provider {
            ProviderId::YouTube => self.search_youtube.into(),
            ProviderId::Local => self.search_local.into(),
            other => (self.search_provider)(other.name()),
        }
    }

    /// Display name for `provider`: brand names are shared across locales,
    /// `Local` is translated.
    pub fn provider(&self, provider: ProviderId) -> &'static str {
        match provider {
            ProviderId::Local => self.provider_local,
            other => other.name(),
        }
    }

    /// Localized sentence for a classified provider failure.
    pub fn error_summary(&self, kind: ErrorKind) -> &'static str {
        match kind {
            ErrorKind::Network => self.error_network,
            ErrorKind::RateLimited => self.error_rate_limited,
            ErrorKind::Denied => self.error_denied,
            ErrorKind::Unavailable => self.error_unavailable,
            ErrorKind::ToolMissing => self.error_tool_missing,
            ErrorKind::Parse => self.error_parse,
            ErrorKind::Other => self.error_other,
        }
    }

    /// The reason to show where the surrounding UI already names the failed
    /// operation ("Search failed: …"): the localized sentence for a recognized
    /// kind, else the provider's own message, which is the only text that
    /// names the cause then.
    pub fn error_reason<'a>(&'a self, error: &'a ProviderError) -> &'a str {
        match error.kind {
            ErrorKind::Other if !error.detail.is_empty() => &error.detail,
            kind => self.error_summary(kind),
        }
    }
}

/// Builds a submenu label in one buffer, appending `marker` when the action falls back to a search.
pub(super) fn with_search_marker(label: fmt::Arguments<'_>, search: bool, marker: &str) -> String {
    let mut out = String::with_capacity(SUBMENU_LABEL_CAPACITY);
    let _ = out.write_fmt(label);
    if search {
        out.push(' ');
        let _ = out.write_str(marker);
    }
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;

    #[test]
    fn submenu_labels_name_the_provider() {
        for lang in Language::ALL {
            let s = lang.strings();
            let labels = [
                (s.sub_play_via)("YouTube", true),
                (s.sub_download_from)("YouTube", true),
                (s.sub_via)("YouTube", false),
                (s.sub_on)("YouTube", true),
                (s.sub_clear_cache_from)("YouTube", false),
            ];
            for label in labels {
                assert!(label.contains("YouTube"), "{lang:?}: {label}");
                assert_eq!(label.trim(), label, "{lang:?}: {label}");
            }
        }
    }

    #[test]
    fn search_fallback_changes_the_label() {
        for lang in Language::ALL {
            let s = lang.strings();
            for build in [s.sub_play_via, s.sub_download_from, s.sub_on] {
                assert_ne!(build("YouTube", false), build("YouTube", true), "{lang:?}");
            }
        }
    }

    #[test]
    fn only_arabic_reads_right_to_left() {
        for lang in Language::ALL {
            assert_eq!(lang.direction().is_rtl(), lang == Language::Ar, "{lang:?}");
        }
    }

    #[test]
    fn error_summaries_are_translated() {
        let english = Language::En.strings();
        let kinds = [
            ErrorKind::Network,
            ErrorKind::RateLimited,
            ErrorKind::Denied,
            ErrorKind::Unavailable,
            ErrorKind::ToolMissing,
            ErrorKind::Parse,
            ErrorKind::Other,
        ];
        for lang in Language::ALL {
            let s = lang.strings();
            for kind in kinds {
                let summary = s.error_summary(kind);
                assert!(!summary.trim().is_empty(), "{lang:?} {kind:?}");
                if lang != Language::En {
                    assert_ne!(summary, english.error_summary(kind), "{lang:?} {kind:?}");
                }
            }
        }
    }

    #[test]
    fn unknown_failures_keep_the_provider_message() {
        let s = Language::En.strings();
        let unknown = ProviderError::from_text("provider SDK did something odd");
        assert_eq!(s.error_reason(&unknown), "provider SDK did something odd");
        let known = ProviderError::from_text("ERROR: Video unavailable");
        assert_eq!(s.error_reason(&known), s.error_unavailable);
    }

    #[test]
    fn provider_names_are_localized() {
        let english = Language::En.strings();
        for lang in Language::ALL {
            let s = lang.strings();
            for &provider in ProviderId::all() {
                let name = s.provider(provider);
                assert!(!name.trim().is_empty(), "{lang:?} {provider:?}");
                if provider == ProviderId::Local {
                    if lang != Language::En {
                        assert_ne!(name, english.provider(provider), "{lang:?}");
                    }
                } else {
                    assert_eq!(name, provider.name(), "{lang:?} {provider:?}");
                }
            }
        }
    }

    #[test]
    fn with_search_marker_fills_one_buffer() {
        let plain = with_search_marker(
            format_args!("Play via {provider}", provider = "X"),
            false,
            "(search)",
        );
        assert_eq!(plain, "Play via X");
        assert_eq!(plain.capacity(), SUBMENU_LABEL_CAPACITY);
        let marked = with_search_marker(
            format_args!("Play via {provider}", provider = "X"),
            true,
            "(search)",
        );
        assert_eq!(marked, "Play via X (search)");
        assert_eq!(marked.capacity(), SUBMENU_LABEL_CAPACITY);
    }

    #[test]
    fn submenu_labels_fit_the_shared_buffer() {
        for lang in Language::ALL {
            let s = lang.strings();
            let builds = [
                s.sub_play_via,
                s.sub_download_from,
                s.sub_via,
                s.sub_on,
                s.sub_clear_cache_from,
            ];
            for build in builds {
                for provider in ProviderId::all() {
                    for search in [false, true] {
                        let label = build(provider.name(), search);
                        assert!(
                            label.len() <= SUBMENU_LABEL_CAPACITY,
                            "{lang:?} {provider:?} search={search}: {label}"
                        );
                    }
                }
            }
        }
    }

    const COUNT_PROBES: [u64; 10] = [0, 1, 2, 5, 11, 21, 22, 25, 100, 102];

    type Counted = fn(usize) -> String;
    type CountedInto = fn(usize, &str) -> String;

    /// How many distinct wordings the message produces once the number itself
    /// is blanked out, i.e. 1 means the count changes nothing but the digits.
    fn count_shapes(build: impl Fn(u64) -> String) -> usize {
        COUNT_PROBES
            .iter()
            .map(|&n| build(n).replace(&n.to_string(), "N"))
            .collect::<BTreeSet<_>>()
            .len()
    }

    #[test]
    fn track_counts_inflect_where_the_language_does() {
        const INFLECTING: &[Language] = &[
            Language::Ar,
            Language::Be,
            Language::En,
            Language::Es,
            Language::Fr,
            Language::Pl,
            Language::PtBr,
            Language::Ru,
            Language::Uk,
        ];
        for &lang in INFLECTING {
            let s = lang.strings();
            let builds: [(&str, Counted); 13] = [
                ("added", s.added),
                ("removed_n", s.removed_n),
                ("added_local", s.added_local),
                ("downloading_n", s.downloading_n),
                ("ctx_add_to_queue_n", s.ctx_add_to_queue_n),
                ("ctx_add_to_playlist_n", s.ctx_add_to_playlist_n),
                ("ctx_download_n", s.ctx_download_n),
                ("ctx_remove_from_queue_n", s.ctx_remove_from_queue_n),
                ("ctx_remove_from_playlist_n", s.ctx_remove_from_playlist_n),
                ("ctx_remove_from_recent_n", s.ctx_remove_from_recent_n),
                ("ctx_add_to_trashbin_n", s.ctx_add_to_trashbin_n),
                ("ctx_remove_from_trashbin_n", s.ctx_remove_from_trashbin_n),
                ("import_playlists_imported", s.import_playlists_imported),
            ];
            for (field, build) in builds {
                assert!(
                    count_shapes(|n| build(n as usize)) > 1,
                    "{lang:?}: {field} does not use a plural form"
                );
            }
            let into_builds: [(&str, CountedInto); 3] = [
                ("added_to", s.added_to),
                ("removed_from", s.removed_from),
                ("pasted_into", s.pasted_into),
            ];
            for (field, build) in into_builds {
                assert!(
                    count_shapes(|n| build(n as usize, "Queue")) > 1,
                    "{lang:?}: {field} does not use a plural form"
                );
            }
        }
    }

    #[test]
    fn play_counts_inflect_where_the_language_does() {
        const INFLECTING: &[Language] = &[
            Language::Ar,
            Language::Be,
            Language::De,
            Language::En,
            Language::Es,
            Language::Fr,
            Language::Pl,
            Language::PtBr,
            Language::Ru,
            Language::Uk,
        ];
        for &lang in INFLECTING {
            let n_plays = lang.strings().n_plays;
            assert!(
                count_shapes(n_plays) > 1,
                "{lang:?}: n_plays does not use a plural form"
            );
        }
    }
}
