//! The action registry behind the palette: one entry per thing the user can run
//! by name, with the localized label, extra search terms and the key chord that
//! does the same thing.
//!
//! Every entry resolves to a [`Message`] the update loop already understands,
//! so the palette adds an input method rather than a second action path.

use super::{Message, MusicPlayer, Shortcut};
use crate::i18n::Strings;

/// One runnable action.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandId {
    PlayPause,
    NextTrack,
    PreviousTrack,
    SeekBackward,
    SeekForward,
    ToggleQueue,
    SwitchQueueTab,
    ToggleRepeat,
    Mute,
    VolumeUp,
    VolumeDown,
    ShowLyrics,
    PlayFocused,
    ToggleSelection,
    SelectAll,
    CopySelection,
    Paste,
    DeleteSelected,
    OpenMenu,
    Back,
    Forward,
    PreviousPlaylist,
    NextPlaylist,
    FocusSearch,
    FindInList,
    RunSearch,
    SplitSideBySide,
    SplitStacked,
    ClosePane,
    NextPane,
    PreviousPane,
    AddLocal,
    ImportPlaylist,
    CheckUpdates,
}

/// A registry entry. `keywords` are untranslated on purpose: they are extra
/// handles for the fuzzy filter (`lyrics` finds a label a locale words
/// differently), not UI text.
pub struct Command {
    pub id: CommandId,
    pub label: fn(&Strings) -> &'static str,
    pub keywords: &'static str,
    /// The equivalent key chord, shown as the row's trailing hint; empty when
    /// the action has no direct shortcut.
    pub keys: &'static str,
}

pub const COMMANDS: &[Command] = &[
    Command {
        id: CommandId::PlayPause,
        label: |s| s.pal_play_pause,
        keywords: "play pause resume stop",
        keys: "Space",
    },
    Command {
        id: CommandId::NextTrack,
        label: |s| s.pal_next_track,
        keywords: "next skip forward",
        keys: "N",
    },
    Command {
        id: CommandId::PreviousTrack,
        label: |s| s.pal_prev_track,
        keywords: "previous back prev",
        keys: "P",
    },
    Command {
        id: CommandId::SeekBackward,
        label: |s| s.pal_seek_back,
        keywords: "seek back rewind",
        keys: ",",
    },
    Command {
        id: CommandId::SeekForward,
        label: |s| s.pal_seek_forward,
        keywords: "seek forward",
        keys: ".",
    },
    Command {
        id: CommandId::ToggleQueue,
        label: |s| s.pal_toggle_queue,
        keywords: "queue panel show hide",
        keys: "Q",
    },
    Command {
        id: CommandId::SwitchQueueTab,
        label: |s| s.pal_queue_tab,
        keywords: "queue tab recently played",
        keys: "T",
    },
    Command {
        id: CommandId::ToggleRepeat,
        label: |s| s.pal_toggle_repeat,
        keywords: "repeat loop",
        keys: "R",
    },
    Command {
        id: CommandId::Mute,
        label: |s| s.pal_mute,
        keywords: "mute silence sound",
        keys: "M",
    },
    Command {
        id: CommandId::VolumeUp,
        label: |s| s.pal_volume_up,
        keywords: "volume louder sound",
        keys: "=",
    },
    Command {
        id: CommandId::VolumeDown,
        label: |s| s.pal_volume_down,
        keywords: "volume quieter sound",
        keys: "-",
    },
    Command {
        id: CommandId::ShowLyrics,
        label: |s| s.pal_show_lyrics,
        keywords: "lyrics text words",
        keys: "Shift+L",
    },
    Command {
        id: CommandId::PlayFocused,
        label: |s| s.pal_play_focused,
        keywords: "play focused hovered track",
        keys: "Enter",
    },
    Command {
        id: CommandId::ToggleSelection,
        label: |s| s.pal_toggle_selection,
        keywords: "toggle select selection",
        keys: "Ctrl+Space",
    },
    Command {
        id: CommandId::SelectAll,
        label: |s| s.pal_select_all,
        keywords: "select all",
        keys: "Ctrl+A",
    },
    Command {
        id: CommandId::CopySelection,
        label: |s| s.pal_copy,
        keywords: "copy clipboard",
        keys: "Ctrl+C",
    },
    Command {
        id: CommandId::Paste,
        label: |s| s.pal_paste,
        keywords: "paste clipboard",
        keys: "Ctrl+V",
    },
    Command {
        id: CommandId::DeleteSelected,
        label: |s| s.pal_delete_selected,
        keywords: "delete remove trash",
        keys: "Del",
    },
    Command {
        id: CommandId::OpenMenu,
        label: |s| s.pal_open_menu,
        keywords: "context menu right click",
        keys: "Menu",
    },
    Command {
        id: CommandId::Back,
        label: |s| s.pal_back,
        keywords: "back history previous",
        keys: "Alt+Left",
    },
    Command {
        id: CommandId::Forward,
        label: |s| s.pal_forward,
        keywords: "forward history next",
        keys: "Alt+Right",
    },
    Command {
        id: CommandId::PreviousPlaylist,
        label: |s| s.pal_prev_playlist,
        keywords: "previous playlist",
        keys: "Alt+Up",
    },
    Command {
        id: CommandId::NextPlaylist,
        label: |s| s.pal_next_playlist,
        keywords: "next playlist",
        keys: "Alt+Down",
    },
    Command {
        id: CommandId::FocusSearch,
        label: |s| s.pal_focus_search,
        keywords: "search focus bar",
        keys: "/",
    },
    Command {
        id: CommandId::FindInList,
        label: |s| s.find_in_list,
        keywords: "find filter list",
        keys: "Ctrl+F",
    },
    Command {
        id: CommandId::RunSearch,
        label: |s| s.pal_run_search,
        keywords: "run search submit",
        keys: "Alt+Enter",
    },
    Command {
        id: CommandId::SplitSideBySide,
        label: |s| s.pal_split_side,
        keywords: "split pane horizontal side",
        keys: "\\",
    },
    Command {
        id: CommandId::SplitStacked,
        label: |s| s.pal_split_stacked,
        keywords: "split pane vertical stacked",
        keys: "Shift+\\",
    },
    Command {
        id: CommandId::ClosePane,
        label: |s| s.pal_close_pane,
        keywords: "close pane",
        keys: "Ctrl+W",
    },
    Command {
        id: CommandId::NextPane,
        label: |s| s.pal_next_pane,
        keywords: "next pane focus",
        keys: "Ctrl+Tab",
    },
    Command {
        id: CommandId::PreviousPane,
        label: |s| s.pal_prev_pane,
        keywords: "previous pane focus",
        keys: "Ctrl+Shift+Tab",
    },
    Command {
        id: CommandId::AddLocal,
        label: |s| s.add_local,
        keywords: "add local files import",
        keys: "",
    },
    Command {
        id: CommandId::ImportPlaylist,
        label: |s| s.import_playlist,
        keywords: "import playlist csv file",
        keys: "",
    },
    Command {
        id: CommandId::CheckUpdates,
        label: |s| s.check_for_updates,
        keywords: "update version upgrade",
        keys: "",
    },
];

impl CommandId {
    /// The message that runs this action against the live player (the focused
    /// pane owns the view-level ones).
    pub fn message(self, player: &MusicPlayer) -> Message {
        let pane = player.focused_pane_id;
        match self {
            Self::PlayPause => Message::TogglePlayPause,
            Self::NextTrack => Message::NextTrack,
            Self::PreviousTrack => Message::PreviousTrack,
            Self::SeekBackward => Message::Shortcut(Shortcut::SeekSecs(-5.0)),
            Self::SeekForward => Message::Shortcut(Shortcut::SeekSecs(5.0)),
            Self::ToggleQueue => Message::ToggleQueue,
            Self::SwitchQueueTab => Message::Shortcut(Shortcut::CycleQueueTab),
            Self::ToggleRepeat => Message::ToggleRepeat,
            Self::Mute => Message::Shortcut(Shortcut::ToggleMute),
            Self::VolumeUp => Message::Shortcut(Shortcut::VolumeUp),
            Self::VolumeDown => Message::Shortcut(Shortcut::VolumeDown),
            Self::ShowLyrics => Message::ShowLyrics(pane),
            Self::PlayFocused => Message::Shortcut(Shortcut::ActivateHovered),
            Self::ToggleSelection => Message::Shortcut(Shortcut::ToggleSelection),
            Self::SelectAll => Message::Shortcut(Shortcut::SelectAll),
            Self::CopySelection => Message::Shortcut(Shortcut::CopySelection),
            Self::Paste => Message::Shortcut(Shortcut::PasteClipboard),
            Self::DeleteSelected => Message::Shortcut(Shortcut::DeleteHovered),
            Self::OpenMenu => Message::Shortcut(Shortcut::OpenMenu),
            Self::Back => Message::NavigateBack(pane),
            Self::Forward => Message::NavigateForward(pane),
            Self::PreviousPlaylist => Message::Shortcut(Shortcut::CyclePlaylist(-1)),
            Self::NextPlaylist => Message::Shortcut(Shortcut::CyclePlaylist(1)),
            Self::FocusSearch => Message::Shortcut(Shortcut::FocusSearch),
            Self::FindInList => Message::Shortcut(Shortcut::OpenTrackSearch),
            Self::RunSearch => Message::Shortcut(Shortcut::ExecuteSearch),
            Self::SplitSideBySide => Message::SplitHorizontal(pane),
            Self::SplitStacked => Message::SplitVertical(pane),
            Self::ClosePane => Message::ClosePane(pane),
            Self::NextPane => Message::Shortcut(Shortcut::FocusNext(1)),
            Self::PreviousPane => Message::Shortcut(Shortcut::FocusNext(-1)),
            Self::AddLocal => Message::AddLocalMusic,
            Self::ImportPlaylist => Message::OpenImportPlaylist,
            Self::CheckUpdates => Message::CheckForUpdates,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Language;

    #[test]
    fn command_ids_are_unique() {
        for (i, command) in COMMANDS.iter().enumerate() {
            for other in &COMMANDS[i + 1..] {
                assert_ne!(command.id, other.id, "{:?} is listed twice", command.id);
            }
        }
    }

    #[test]
    fn every_command_has_a_label_in_every_language() {
        for language in Language::ALL {
            let strings = language.strings();
            for command in COMMANDS {
                assert!(
                    !(command.label)(strings).is_empty(),
                    "{:?} in {language:?}",
                    command.id
                );
            }
        }
    }
}
