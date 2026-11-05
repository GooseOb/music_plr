use super::{
    CardMenuState, ContextMenuState, DependencyDialog, EditTrackState, ImportPlaylistDialog,
    Palette, TranslateDialog,
};
use crate::{data::server_playlists::ServerPlaylistId, providers::ProviderId};

/// The name field of the create-playlist dialog for a server. Renames happen
/// inline on the playlist pages, so creating is the field's only user.
#[derive(Debug, Clone)]
pub struct ServerPlaylistNameDialog {
    pub provider: ProviderId,
    pub name: String,
}

impl ServerPlaylistNameDialog {
    pub fn new_playlist(provider: ProviderId) -> Self {
        Self {
            provider,
            name: String::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ServerPlaylistDeleteDialog {
    pub target: ServerPlaylistId,
    pub name: String,
}

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
    ServerPlaylistName(ServerPlaylistNameDialog),
    ServerPlaylistDelete(ServerPlaylistDeleteDialog),
}
