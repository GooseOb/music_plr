use super::{
    ContextMenuState, DependencyDialog, EditTrackState, ImportPlaylistDialog, PlaylistPicker,
};

#[derive(Debug, Clone)]
pub enum Dialog {
    Dependencies(DependencyDialog),
    Picker(PlaylistPicker),
    DeleteConfirm(usize),
    Edit(EditTrackState),
    Import(ImportPlaylistDialog),
    ContextMenu(ContextMenuState),
}
