use super::{
    CardMenuState, ContextMenuState, DependencyDialog, EditTrackState, ImportPlaylistDialog,
    Palette, TranslateDialog,
};

#[derive(Debug, Clone)]
pub enum Dialog {
    Dependencies(DependencyDialog),
    Palette(Palette),
    DeleteConfirm(usize),
    Edit(EditTrackState),
    Import(ImportPlaylistDialog),
    Translate(TranslateDialog),
    ContextMenu(ContextMenuState),
    CardMenu(CardMenuState),
}
