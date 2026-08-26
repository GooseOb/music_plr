use serde::{Deserialize, Serialize};

use crate::providers::ProviderId;

pub mod en;
pub mod pl;

/// Declares the `Language` enum and its `ALL` / `label` / `strings` machinery
/// from a single list, so adding a language only requires creating its module
/// and appending one entry here — the enum itself is never hand-edited.
///
/// Each entry is `Variant => ("display label", module)` where `module` exposes
/// a `pub const STRINGS: Strings`. The first entry is the `#[default]`.
macro_rules! languages {
    (
        $first:ident => ($fl:literal, $fm:ident)
        $(, $variant:ident => ($label:literal, $module:ident) )*
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
        }
    };
}

languages! {
    En => ("English", en),
    Pl => ("Polski", pl),
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

    pub playlist_not_found: &'static str,
    pub add_local: &'static str,
    pub downloaded_tracks: &'static str,
    pub no_downloaded_tracks: &'static str,

    pub ctx_play: &'static str,
    pub ctx_edit: &'static str,
    pub ctx_go_to_artist: &'static str,
    pub ctx_add_to_playlist: &'static str,
    pub ctx_download: &'static str,
    pub ctx_song_radio: &'static str,
    pub ctx_artist_radio: &'static str,
    pub ctx_remove_from_queue: &'static str,
    pub ctx_remove_from_playlist: &'static str,
    pub sub_play_via: &'static str,
    pub sub_download_from: &'static str,
    pub sub_via: &'static str,
    pub sub_on: &'static str,
    pub via_search_suffix: &'static str,

    pub current: &'static str,
    pub select: &'static str,
    pub find: &'static str,
    pub finding: &'static str,
    pub found_on: fn(&str) -> String,
    pub not_linked: &'static str,
    pub providers: &'static str,
    pub no_provider_data: &'static str,
    pub save: &'static str,
    pub cancel: &'static str,
    pub edit_track: &'static str,
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
    pub theme_lbl: &'static str,
    pub language_lbl: &'static str,
    pub default_provider_lbl: &'static str,
    pub normalize_volume_lbl: &'static str,
    pub download_dir_lbl: &'static str,
    pub cache_size_lbl: &'static str,
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
    pub select_playlist_drop: &'static str,
    pub reordered_playlist: &'static str,

    pub n_saved: fn(usize) -> String,
    pub n_plays: fn(usize) -> String,
    pub search_placeholder: fn(ProviderId) -> String,
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
    pub radio_label: fn(&str, &str) -> String,
    pub ctx_add_to_playlist_n: fn(usize) -> String,
    pub ctx_download_n: fn(usize) -> String,
    pub ctx_remove_from_queue_n: fn(usize) -> String,
    pub ctx_remove_from_playlist_n: fn(usize) -> String,
}
