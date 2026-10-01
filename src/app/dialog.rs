use super::{
    ContextMenuState, DependencyDialog, EditTrackState, ImportPlaylistDialog, PlaylistJump,
    TranslateDialog,
};

#[derive(Debug, Clone)]
pub enum Dialog {
    Dependencies(DependencyDialog),
    PlaylistJump(PlaylistJump),
    Shortcuts,
    DeleteConfirm(usize),
    Edit(EditTrackState),
    Import(ImportPlaylistDialog),
    Translate(TranslateDialog),
    ContextMenu(ContextMenuState),
}
