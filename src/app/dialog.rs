use super::{
    CardMenuState, ContextMenuState, DependencyDialog, EditTrackState, ImportPlaylistDialog,
    Palette, TranslateDialog,
};
use crate::{data::server_playlists::ServerPlaylistId, providers::ProviderId};

/// One name field shared by the two playlist-creation flows: creating a
/// playlist on a server (`id: None`) and renaming an existing one
/// (`id: Some`). Both submit the same message, so the field has one handler.
#[derive(Debug, Clone)]
pub struct ServerPlaylistNameDialog {
    pub provider: ProviderId,
    /// `Some` when renaming, `None` when creating.
    pub id: Option<String>,
    pub name: String,
}

impl ServerPlaylistNameDialog {
    pub fn new_playlist(provider: ProviderId) -> Self {
        Self {
            provider,
            id: None,
            name: String::new(),
        }
    }

    pub fn rename(target: &ServerPlaylistId, name: &str) -> Self {
        Self {
            provider: target.provider,
            id: Some(target.id.clone()),
            name: name.to_string(),
        }
    }

    pub fn target(&self) -> Option<ServerPlaylistId> {
        self.id.as_ref().map(|id| ServerPlaylistId {
            provider: self.provider,
            id: id.clone(),
        })
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
