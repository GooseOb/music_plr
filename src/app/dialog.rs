use super::{
    ContextMenuState, DependencyDialog, EditTrackState, ImportPlaylistDialog, PlaylistJump,
    PlaylistPicker,
};

#[derive(Debug, Clone)]
pub enum Dialog {
    Dependencies(DependencyDialog),
    Picker(PlaylistPicker),
    PlaylistJump(PlaylistJump),
    Shortcuts,
    DeleteConfirm(usize),
    Edit(EditTrackState),
    Import(ImportPlaylistDialog),
    ContextMenu(ContextMenuState),
}
