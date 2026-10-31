//! Keyboard shortcuts decoded at the event level.
//!
//! The subscription matches the raw key event (code + modifiers) into a
//! [`Shortcut`] and only then emits a message, so `update` never sees keys
//! nothing would happen for — including OS auto-repeat on unmapped keys.
//! Whatever still depends on live state (focused pane, open dialog, vim
//! prefix timing, list-vs-history mode) is resolved in
//! [`MusicPlayer::apply_shortcut`](crate::app::MusicPlayer::apply_shortcut).
//! The subscription gates which matcher runs by open dialog, mirroring how
//! the old handler swallowed everything but menu/jump keys while those
//! dialogs were open.

use iced::{
    event::Status,
    keyboard::{
        key::{Code, Physical},
        Modifiers,
    },
    window, Event,
};

use super::{message::Message, pane::PaneDir};

/// One actionable key chord. Modifiers, shift variants, and digit indices
/// are resolved here; only live state is left to `update`.
#[derive(Debug, Clone, Copy)]
pub enum Shortcut {
    OpenTrackSearch,
    OpenPalette,
    OpenPlaylistPicker,
    FocusSearch,
    SplitVertical,
    SplitHorizontal,
    ClosePane,
    ToggleSelection,
    PlayPause,
    OpenMenu,
    ExecuteSearch,
    Escape,
    DeleteHovered,
    FocusNeighbor(PaneDir),
    NavBack,
    NavForward,
    CyclePlaylist(isize),
    ToggleListFocus,
    OpenLyrics,
    HoverMove { dir: isize, extend: bool },
    VimG,
    EdgeTo(bool),
    PageStep(isize),
    HalfPageStep(isize),
    CycleProvider(isize),
    CycleScope(isize),
    ProviderSlot(usize),
    PaneIndex(usize),
    FocusNext(isize),
    ActivateHovered,
    NextMaybeSearch(bool),
    PrevTrack,
    CycleQueueTab,
    ToggleMute,
    VolumeDown,
    VolumeUp,
    SeekSecs(f32),
    SelectAll,
    CopySelection,
    PasteClipboard,
    MenuUp,
    MenuDown,
    MenuAcross,
    MenuActivate,
    PaletteStep(isize),
    PaletteEdge(bool),
    PaletteConfirm { play: bool },
}

fn mods(modifiers: Modifiers) -> (bool, bool, bool) {
    let ctrl = modifiers.control() || modifiers.logo();
    (ctrl, modifiers.alt(), modifiers.shift())
}

/// Main table: active with no dialog or a non-menu/jump dialog open.
#[allow(clippy::too_many_lines)]
// `iced::event::listen_with` hands the event over by value, so the by-value
// parameter is mandated by the API.
#[allow(clippy::needless_pass_by_value)]
pub fn match_main(event: Event, status: Status, _window: window::Id) -> Option<Message> {
    let Event::Keyboard(iced::keyboard::Event::KeyPressed {
        physical_key: Physical::Code(code),
        modifiers,
        ..
    }) = event
    else {
        return None;
    };
    if status != Status::Ignored {
        return None;
    }
    let (ctrl, alt, shift) = mods(modifiers);
    let s = |v: Shortcut| Some(Message::Shortcut(v));
    match code {
        Code::KeyF if ctrl && !alt => s(Shortcut::OpenTrackSearch),
        Code::KeyK if ctrl && !alt => s(Shortcut::OpenPlaylistPicker),
        Code::Slash if !ctrl && !alt && !shift => s(Shortcut::FocusSearch),
        Code::Slash if !ctrl && !alt && shift => s(Shortcut::OpenPalette),
        Code::F1 if !ctrl && !alt => s(Shortcut::OpenPalette),
        Code::Backslash if !ctrl && !alt && !shift => s(Shortcut::SplitVertical),
        Code::Backslash if !ctrl && !alt && shift => s(Shortcut::SplitHorizontal),
        Code::KeyW if ctrl && !alt => s(Shortcut::ClosePane),
        Code::Space if ctrl && !alt => s(Shortcut::ToggleSelection),
        Code::Space if !ctrl && !alt => s(Shortcut::PlayPause),
        Code::ContextMenu => s(Shortcut::OpenMenu),
        Code::F10 if shift && !ctrl => s(Shortcut::OpenMenu),
        Code::Enter if ctrl && !alt => s(Shortcut::OpenMenu),
        Code::Enter if alt && !ctrl => s(Shortcut::ExecuteSearch),
        Code::Escape => s(Shortcut::Escape),
        Code::Delete => s(Shortcut::DeleteHovered),
        Code::ArrowLeft if ctrl && !alt => s(Shortcut::FocusNeighbor(PaneDir::Left)),
        Code::ArrowRight if ctrl && !alt => s(Shortcut::FocusNeighbor(PaneDir::Right)),
        Code::ArrowUp if ctrl && !alt => s(Shortcut::FocusNeighbor(PaneDir::Up)),
        Code::ArrowDown if ctrl && !alt => s(Shortcut::FocusNeighbor(PaneDir::Down)),
        Code::ArrowLeft if alt && !ctrl => s(Shortcut::NavBack),
        Code::ArrowRight if alt && !ctrl => s(Shortcut::NavForward),
        Code::ArrowUp if alt && !ctrl => s(Shortcut::CyclePlaylist(-1)),
        Code::ArrowDown if alt && !ctrl => s(Shortcut::CyclePlaylist(1)),
        Code::ArrowLeft | Code::ArrowRight | Code::KeyH if !ctrl && !alt => {
            s(Shortcut::ToggleListFocus)
        }
        Code::KeyL if !ctrl && !alt && !shift => s(Shortcut::ToggleListFocus),
        Code::KeyL if !ctrl && !alt && shift => s(Shortcut::OpenLyrics),
        Code::ArrowUp | Code::KeyK if !ctrl && !alt => s(Shortcut::HoverMove {
            dir: -1,
            extend: shift,
        }),
        Code::ArrowDown | Code::KeyJ if !ctrl && !alt => s(Shortcut::HoverMove {
            dir: 1,
            extend: shift,
        }),
        Code::KeyG if shift && !ctrl && !alt => s(Shortcut::EdgeTo(false)),
        Code::KeyG if !ctrl && !alt && !shift => s(Shortcut::VimG),
        Code::Home if !ctrl && !alt => s(Shortcut::EdgeTo(true)),
        Code::End if !ctrl && !alt => s(Shortcut::EdgeTo(false)),
        Code::PageUp if !ctrl && !alt => s(Shortcut::PageStep(-1)),
        Code::PageDown if !ctrl && !alt => s(Shortcut::PageStep(1)),
        Code::KeyU if ctrl && !alt => s(Shortcut::HalfPageStep(-1)),
        Code::KeyD if ctrl && !alt => s(Shortcut::HalfPageStep(1)),
        Code::KeyP if alt && !ctrl => s(Shortcut::CycleProvider(if shift { -1 } else { 1 })),
        Code::KeyS if alt && !ctrl => s(Shortcut::CycleScope(if shift { -1 } else { 1 })),
        Code::Digit1 | Code::Digit2 | Code::Digit3 | Code::Digit4 | Code::Digit5
            if alt && !ctrl =>
        {
            s(Shortcut::ProviderSlot(match code {
                Code::Digit1 => 0,
                Code::Digit2 => 1,
                Code::Digit3 => 2,
                Code::Digit4 => 3,
                _ => 4,
            }))
        }
        Code::Digit1 | Code::Digit2 | Code::Digit3 | Code::Digit4 if ctrl && !alt => {
            s(Shortcut::PaneIndex(match code {
                Code::Digit1 => 0,
                Code::Digit2 => 1,
                Code::Digit3 => 2,
                _ => 3,
            }))
        }
        Code::Tab if ctrl && !alt => s(Shortcut::FocusNext(if shift { -1 } else { 1 })),
        Code::Enter => s(Shortcut::ActivateHovered),
        Code::KeyN if !ctrl && !alt => s(Shortcut::NextMaybeSearch(shift)),
        Code::KeyP if !ctrl && !alt => s(Shortcut::PrevTrack),
        Code::KeyQ if !ctrl && !alt => Some(Message::ToggleQueue),
        Code::KeyR if !ctrl && !alt => Some(Message::ToggleRepeat),
        Code::KeyT if !ctrl && !alt => s(Shortcut::CycleQueueTab),
        Code::KeyM if !ctrl && !alt => s(Shortcut::ToggleMute),
        Code::Minus if !ctrl && !alt => s(Shortcut::VolumeDown),
        Code::Equal if !ctrl && !alt => s(Shortcut::VolumeUp),
        Code::Comma if !ctrl && !alt => s(Shortcut::SeekSecs(if shift { -10.0 } else { -5.0 })),
        Code::Period if !ctrl && !alt => s(Shortcut::SeekSecs(if shift { 10.0 } else { 5.0 })),
        Code::KeyA if ctrl && !alt => s(Shortcut::SelectAll),
        Code::KeyC if ctrl && !alt => s(Shortcut::CopySelection),
        Code::KeyV if ctrl && !alt => s(Shortcut::PasteClipboard),
        _ => None,
    }
}

/// Context-menu table: arrows/enter drive the menu, everything else is
/// swallowed exactly like before.
#[allow(clippy::needless_pass_by_value)]
pub fn match_menu(event: Event, status: Status, _window: window::Id) -> Option<Message> {
    let Event::Keyboard(iced::keyboard::Event::KeyPressed {
        physical_key: Physical::Code(code),
        ..
    }) = event
    else {
        return None;
    };
    if status != Status::Ignored {
        return None;
    }
    let s = |v: Shortcut| Some(Message::Shortcut(v));
    match code {
        Code::Escape => s(Shortcut::Escape),
        Code::ArrowUp => s(Shortcut::MenuUp),
        Code::ArrowDown => s(Shortcut::MenuDown),
        Code::ArrowLeft | Code::ArrowRight => s(Shortcut::MenuAcross),
        Code::Enter => s(Shortcut::MenuActivate),
        _ => None,
    }
}

/// Palette table: ctrl/alt combos are swallowed, shift only steers Enter into
/// run-and-play. The palette input sets no `on_submit`, so a focused Enter
/// arrives here with its shift state intact.
#[allow(clippy::needless_pass_by_value)]
pub fn match_palette(event: Event, status: Status, _window: window::Id) -> Option<Message> {
    let Event::Keyboard(iced::keyboard::Event::KeyPressed {
        physical_key: Physical::Code(code),
        modifiers,
        ..
    }) = event
    else {
        return None;
    };
    if status != Status::Ignored {
        return None;
    }
    if modifiers.control() || modifiers.logo() || modifiers.alt() {
        return None;
    }
    let s = |v: Shortcut| Some(Message::Shortcut(v));
    match code {
        Code::Escape => s(Shortcut::Escape),
        Code::ArrowUp | Code::KeyK => s(Shortcut::PaletteStep(-1)),
        Code::ArrowDown | Code::KeyJ => s(Shortcut::PaletteStep(1)),
        Code::Home => s(Shortcut::PaletteEdge(true)),
        Code::End => s(Shortcut::PaletteEdge(false)),
        Code::Enter => s(Shortcut::PaletteConfirm {
            play: modifiers.shift(),
        }),
        _ => None,
    }
}
