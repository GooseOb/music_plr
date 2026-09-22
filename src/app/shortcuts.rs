//! In-app keyboard cheatsheet (`?` / `F1`, `Dialog::Shortcuts`).
//!
//! Key combos stay fixed; every label resolves through `Strings` so the
//! dialog follows the app language. Keep in sync with `update/input.rs`
//! and the README shortcut table.

use crate::i18n::Strings;

pub struct HelpSection {
    pub title: fn(&Strings) -> &'static str,
    pub rows: &'static [HelpRow],
}

pub struct HelpRow {
    pub keys: &'static str,
    pub action: fn(&Strings) -> &'static str,
}

pub const SECTIONS: &[HelpSection] = &[
    HelpSection {
        title: |tr| tr.sc_sec_list,
        rows: &[
            HelpRow {
                keys: "Up / Down or j / k",
                action: |tr| tr.sc_move,
            },
            HelpRow {
                keys: "h / l or Left / Right",
                action: |tr| tr.sc_focus_panel,
            },
            HelpRow {
                keys: "gg / G or Home / End",
                action: |tr| tr.sc_first_last,
            },
            HelpRow {
                keys: "PgUp / PgDn or Ctrl+U / Ctrl+D",
                action: |tr| tr.sc_page,
            },
            HelpRow {
                keys: "Enter",
                action: |tr| tr.sc_play,
            },
        ],
    },
    HelpSection {
        title: |tr| tr.sc_sec_select,
        rows: &[
            HelpRow {
                keys: "Shift+Up / Down (J / K)",
                action: |tr| tr.sc_extend,
            },
            HelpRow {
                keys: "Ctrl+Space",
                action: |tr| tr.sc_toggle_sel,
            },
            HelpRow {
                keys: "Ctrl+A",
                action: |tr| tr.sc_select_all,
            },
            HelpRow {
                keys: "Ctrl+C / Ctrl+V",
                action: |tr| tr.sc_copy_paste,
            },
            HelpRow {
                keys: "Del",
                action: |tr| tr.sc_delete,
            },
            HelpRow {
                keys: "Esc",
                action: |tr| tr.sc_esc,
            },
        ],
    },
    HelpSection {
        title: |tr| tr.sc_sec_search,
        rows: &[
            HelpRow {
                keys: "/",
                action: |tr| tr.sc_focus_search,
            },
            HelpRow {
                keys: "Alt+P or Alt+1..5",
                action: |tr| tr.sc_stage_provider,
            },
            HelpRow {
                keys: "Alt+S",
                action: |tr| tr.sc_stage_scope,
            },
            HelpRow {
                keys: "Alt+Enter",
                action: |tr| tr.sc_run_search,
            },
            HelpRow {
                keys: "Ctrl+F",
                action: |tr| tr.sc_find_list,
            },
        ],
    },
    HelpSection {
        title: |tr| tr.sc_sec_playlists,
        rows: &[
            HelpRow {
                keys: "Alt+Up / Down",
                action: |tr| tr.sc_prev_next_pl,
            },
            HelpRow {
                keys: "Ctrl+K",
                action: |tr| tr.sc_jump,
            },
            HelpRow {
                keys: "Alt+Left / Right",
                action: |tr| tr.sc_back_fwd,
            },
        ],
    },
    HelpSection {
        title: |tr| tr.sc_sec_playback,
        rows: &[
            HelpRow {
                keys: "Space",
                action: |tr| tr.sc_play_pause,
            },
            HelpRow {
                keys: "N / P",
                action: |tr| tr.sc_next_prev,
            },
            HelpRow {
                keys: "M",
                action: |tr| tr.sc_mute,
            },
            HelpRow {
                keys: "- / =",
                action: |tr| tr.sc_volume,
            },
            HelpRow {
                keys: ", / .",
                action: |tr| tr.sc_seek,
            },
            HelpRow {
                keys: "Q / R / Shift+L / T",
                action: |tr| tr.sc_toggles,
            },
        ],
    },
    HelpSection {
        title: |tr| tr.sc_sec_panes,
        rows: &[
            HelpRow {
                keys: "\\ or Shift+\\",
                action: |tr| tr.sc_split,
            },
            HelpRow {
                keys: "Ctrl+W",
                action: |tr| tr.sc_close_pane,
            },
            HelpRow {
                keys: "Ctrl+arrows",
                action: |tr| tr.sc_adj_pane,
            },
            HelpRow {
                keys: "Ctrl+Tab or Ctrl+1..4",
                action: |tr| tr.sc_cycle_pane,
            },
        ],
    },
    HelpSection {
        title: |tr| tr.sc_sec_menu,
        rows: &[
            HelpRow {
                keys: "Menu / Shift+F10 / Ctrl+Enter",
                action: |tr| tr.sc_open_menu,
            },
            HelpRow {
                keys: "Up / Down / Left / Right / Enter",
                action: |tr| tr.sc_nav_menu,
            },
        ],
    },
];
